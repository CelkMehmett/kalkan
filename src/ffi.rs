//! Bağımlılıksız C ABI: WebAssembly'de JavaScript'ten, yerelde C/Go/Node/Bun vb.
//! dillerden çağırmak için. Yalnızca `ffi` özelliği açıkken derlenir.
//!
//! # Bellek sözleşmesi
//! - Girdi: çağıran `kalkan_alloc(n)` ile tampon alır, UTF-8 baytları yazar,
//!   işlevi `(ptr, len)` ile çağırır, sonra `kalkan_free(ptr, n)` ile bırakır.
//!   Ayırma boyutu tamponun içinde saklanır; `kalkan_free`'e yanlış `len`
//!   geçirilmesi güvenli olsa da doğru değeri geçirin.
//! - Çıktı: metin döndüren işlevler sonucu süreç genelindeki iç tampona yazar ve
//!   **uzunluğunu** döndürür; içerik `kalkan_out_ptr()` adresinden okunur ve bir
//!   sonraki çağrıya kadar geçerlidir. Tampon süreç genelinde paylaşıldığı için
//!   eşzamanlı çağrılarda yalnızca son tamamlanan çağrının çıktısı okunmalıdır.
//!   Hata durumunda `u32::MAX` döner, mesaj aynı tampondadır.
//! - Konumlar (detect/JSON) **UTF-16 kod birimi** cinsindendir; JavaScript
//!   dizgileriyle (`text.slice(start, end)`) doğrudan uyumludur.

#![allow(unsafe_code)]

use std::sync::Mutex;

use crate::{Kalkan, Kind, Match, Session, Strategy, StreamRestorer};

const ERR: u32 = u32::MAX;
/// `kalkan_alloc` başlığının boyutu (kapasiteyi saklar).
const ALLOC_HEADER: usize = 8;

/// Tutamaçları (1 tabanlı) yeniden kullanılabilir yuvalarda tutar. Serbest
/// bırakılan yuvalar yeniden kullanıldığı için uzun ömürlü süreçlerde bellek
/// sınırsız büyümez.
struct Slots<T> {
    items: Vec<Option<T>>,
    free: Vec<usize>,
}

impl<T> Slots<T> {
    const fn new() -> Self {
        Self { items: Vec::new(), free: Vec::new() }
    }

    fn insert(&mut self, value: T) -> u32 {
        if let Some(i) = self.free.pop() {
            self.items[i] = Some(value);
            (i + 1) as u32
        } else {
            self.items.push(Some(value));
            self.items.len() as u32
        }
    }

    fn get_mut(&mut self, handle: u32) -> Option<&mut T> {
        let i = (handle as usize).checked_sub(1)?;
        self.items.get_mut(i)?.as_mut()
    }

    fn take(&mut self, handle: u32) -> Option<T> {
        let i = (handle as usize).checked_sub(1)?;
        let value = self.items.get_mut(i)?.take()?;
        self.free.push(i);
        Some(value)
    }
}

struct State {
    sessions: Slots<Session>,
    streams: Slots<StreamRestorer>,
    out: Vec<u8>,
}

static STATE: Mutex<State> =
    Mutex::new(State { sessions: Slots::new(), streams: Slots::new(), out: Vec::new() });

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut g = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut g)
}

/// Sonucu çıktı tamponuna yazar, uzunluğunu döndürür.
fn emit(st: &mut State, r: Result<String, String>) -> u32 {
    match r {
        Ok(s) => {
            st.out = s.into_bytes();
            st.out.len() as u32
        }
        Err(e) => {
            st.out = e.into_bytes();
            ERR
        }
    }
}

/// # Safety
/// `ptr..ptr+len` geçerli, okunabilir bir bellek bölgesi olmalı (veya len == 0).
unsafe fn input<'a>(ptr: *const u8, len: usize) -> Result<&'a str, String> {
    if len == 0 {
        return Ok("");
    }
    if ptr.is_null() {
        return Err("boş işaretçi".into());
    }
    // SAFETY: çağıran sözleşmeye göre bölge geçerli.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    std::str::from_utf8(bytes).map_err(|e| format!("geçersiz UTF-8: {e}"))
}

fn session_mut(st: &mut State, h: u32) -> Result<&mut Session, String> {
    st.sessions.get_mut(h).ok_or_else(|| format!("geçersiz oturum: {h}"))
}

fn json_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Eşleşmeleri UTF-16 konumlu JSON dizisine çevirir.
fn matches_json(text: &str, ms: &[Match]) -> String {
    let mut out = String::from("[");
    let (mut byte, mut u16pos) = (0usize, 0usize);
    let mut adv = |to: usize| {
        u16pos += text[byte..to].encode_utf16().count();
        byte = to;
        u16pos
    };
    for (i, m) in ms.iter().enumerate() {
        let (s, e) = (adv(m.span.start), adv(m.span.end));
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!("{{\"kind\":\"{}\",\"start\":{s},\"end\":{e},\"confidence\":{},\"text\":", m.kind, m.confidence));
        json_str(m.text(text), &mut out);
        out.push('}');
    }
    out.push(']');
    out
}

// ---------------------------------------------------------------- bellek

/// `len` baytlık tampon ayırır. Dönen işaretçi tamponun başlangıcıdır;
/// gerçek kapasite ondan önceki başlıkta saklanır.
#[no_mangle]
pub extern "C" fn kalkan_alloc(len: usize) -> *mut u8 {
    let total = len.saturating_add(ALLOC_HEADER).max(ALLOC_HEADER);
    let mut v = Vec::<u8>::with_capacity(total);
    let capacity = v.capacity();
    let p = v.as_mut_ptr();
    let header = (capacity as u64).to_ne_bytes();
    // SAFETY: `p` en az ALLOC_HEADER baytlık ayrılmış bölgenin başı.
    unsafe { std::ptr::copy_nonoverlapping(header.as_ptr(), p, ALLOC_HEADER) };
    std::mem::forget(v);
    // SAFETY: başlık kadar ilerledik; kalan bölge en az `len` bayt.
    unsafe { p.add(ALLOC_HEADER) }
}

/// `kalkan_alloc` ile ayrılmış tamponu bırakır. `len` uyumluluğu denetlenmez.
///
/// # Safety
/// `ptr`, daha önce `kalkan_alloc` çağrısından dönmüş, serbest bırakılmamış
/// bir işaretçi olmalı.
#[no_mangle]
pub unsafe extern "C" fn kalkan_free(ptr: *mut u8, _len: usize) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: çağıran sözleşmeye göre `ptr` bir kalkan_alloc tamponu.
    let base = unsafe { ptr.sub(ALLOC_HEADER) };
    let mut header = [0u8; ALLOC_HEADER];
    // SAFETY: başlık ayrılan bölgenin içinde.
    unsafe { std::ptr::copy_nonoverlapping(base, header.as_mut_ptr(), ALLOC_HEADER) };
    let capacity = u64::from_ne_bytes(header) as usize;
    // SAFETY: kapasite tam olarak kalkan_alloc'ta ayrılan kapasite.
    drop(unsafe { Vec::from_raw_parts(base, 0, capacity) });
}

/// Son çıktının adresi.
#[no_mangle]
pub extern "C" fn kalkan_out_ptr() -> *const u8 {
    with(|st| st.out.as_ptr())
}

/// Son çıktının uzunluğu (hata mesajları dahil).
#[no_mangle]
pub extern "C" fn kalkan_out_len() -> u32 {
    with(|st| st.out.len() as u32)
}

// ---------------------------------------------------------------- oturum

/// Yeni oturum açar; tutamaç döndürür (0 = hata).
/// `strategy`: 0 token, 1 sentetik, 2 kısmi, 3 etiket.
/// `terms`: satır başına bir kuruma özel terim (UTF-8), boş olabilir.
///
/// # Safety
/// `terms_ptr..+terms_len` geçerli bir bellek bölgesi olmalı.
#[no_mangle]
pub unsafe extern "C" fn kalkan_session_new(
    strategy: u32,
    secret_lo: u32,
    secret_hi: u32,
    terms_ptr: *const u8,
    terms_len: usize,
) -> u32 {
    let Ok(terms) = (unsafe { input(terms_ptr, terms_len) }) else { return 0 };
    let strategy = match strategy {
        0 => Strategy::Token,
        1 => Strategy::Synthetic,
        2 => Strategy::Partial,
        3 => Strategy::Label,
        _ => return 0,
    };
    let k = Kalkan::builder()
        .strategy(strategy)
        .secret(((secret_hi as u64) << 32) | secret_lo as u64)
        .terms(terms.lines())
        .build();
    with(|st| st.sessions.insert(k.session()))
}

/// Oturumu kapatır (kasadaki gerçek değerler bellekten silinir).
#[no_mangle]
pub extern "C" fn kalkan_session_free(h: u32) {
    with(|st| {
        st.sessions.take(h);
    })
}

/// Metni maskeler. Dönüş: çıktı uzunluğu (veya `u32::MAX`).
///
/// # Safety
/// `ptr..ptr+len` geçerli bir bellek bölgesi olmalı.
#[no_mangle]
pub unsafe extern "C" fn kalkan_mask(h: u32, ptr: *const u8, len: usize) -> u32 {
    let text = unsafe { input(ptr, len) };
    with(|st| {
        let r = text.and_then(|t| Ok(session_mut(st, h)?.mask(t)));
        emit(st, r)
    })
}

/// Yer tutucuları geri yükler.
///
/// # Safety
/// `ptr..ptr+len` geçerli bir bellek bölgesi olmalı.
#[no_mangle]
pub unsafe extern "C" fn kalkan_restore(h: u32, ptr: *const u8, len: usize) -> u32 {
    let text = unsafe { input(ptr, len) };
    with(|st| {
        let r = text.and_then(|t| Ok(session_mut(st, h)?.restore(t)));
        emit(st, r)
    })
}

/// Tespit sonuçları, JSON: `[{"kind","start","end","confidence","text"}]` (UTF-16 konumlar).
///
/// # Safety
/// `ptr..ptr+len` geçerli bir bellek bölgesi olmalı.
#[no_mangle]
pub unsafe extern "C" fn kalkan_detect_json(h: u32, ptr: *const u8, len: usize) -> u32 {
    let text = unsafe { input(ptr, len) };
    with(|st| {
        let r = text.and_then(|t| {
            let s = session_mut(st, h)?;
            Ok(matches_json(t, &s.config().detect(t)))
        });
        emit(st, r)
    })
}

/// Model cevabı denetimi, JSON: `{"clean","new_pii":[{"kind","text"}],"echoed_originals":[],"unknown_placeholders":[]}`.
///
/// # Safety
/// `ptr..ptr+len` geçerli bir bellek bölgesi olmalı.
#[no_mangle]
pub unsafe extern "C" fn kalkan_audit_json(h: u32, ptr: *const u8, len: usize) -> u32 {
    let text = unsafe { input(ptr, len) };
    with(|st| {
        let r = text.and_then(|t| {
            let a = session_mut(st, h)?.audit(t);
            let mut o = format!("{{\"clean\":{},\"new_pii\":[", a.is_clean());
            for (i, (k, v)) in a.new_pii.iter().enumerate() {
                o.push_str(if i > 0 { ",{\"kind\":\"" } else { "{\"kind\":\"" });
                o.push_str(k.label());
                o.push_str("\",\"text\":");
                json_str(v, &mut o);
                o.push('}');
            }
            for (name, list) in [("echoed_originals", &a.echoed_originals), ("unknown_placeholders", &a.unknown_placeholders)] {
                o.push_str(&format!("],\"{name}\":["));
                for (i, v) in list.iter().enumerate() {
                    if i > 0 {
                        o.push(',');
                    }
                    json_str(v, &mut o);
                }
            }
            o.push_str("]}");
            Ok(o)
        });
        emit(st, r)
    })
}

/// Kasa, JSON nesnesi: `{"[TCKN_1]": "10000000146", ...}`.
#[no_mangle]
pub extern "C" fn kalkan_vault_json(h: u32) -> u32 {
    with(|st| {
        let r = session_mut(st, h).map(|s| {
            let mut pairs: Vec<_> = s.vault().collect();
            pairs.sort();
            let mut o = String::from("{");
            for (i, (k, v)) in pairs.into_iter().enumerate() {
                if i > 0 {
                    o.push(',');
                }
                json_str(k, &mut o);
                o.push(':');
                json_str(v, &mut o);
            }
            o.push('}');
            o
        });
        emit(st, r)
    })
}

// ---------------------------------------------------------------- akış

/// Oturumun o anki kasasıyla bir akış geri yükleyici açar (0 = hata).
#[no_mangle]
pub extern "C" fn kalkan_stream_new(h: u32) -> u32 {
    with(|st| match session_mut(st, h) {
        Ok(s) => {
            let r = s.stream();
            st.streams.insert(r)
        }
        Err(_) => 0,
    })
}

/// Akışa parça ekler; hemen gösterilebilecek metnin uzunluğunu döndürür.
///
/// # Safety
/// `ptr..ptr+len` geçerli bir bellek bölgesi olmalı.
#[no_mangle]
pub unsafe extern "C" fn kalkan_stream_push(sh: u32, ptr: *const u8, len: usize) -> u32 {
    let text = unsafe { input(ptr, len) };
    with(|st| {
        let r = text.and_then(|t| {
            st.streams.get_mut(sh).map(|s| s.push(t)).ok_or_else(|| format!("geçersiz akış: {sh}"))
        });
        emit(st, r)
    })
}

/// Akışı bitirir (kalan kuyruğu döndürür) ve kapatır.
#[no_mangle]
pub extern "C" fn kalkan_stream_finish(sh: u32) -> u32 {
    with(|st| {
        let r = st
            .streams
            .take(sh)
            .map(|mut s| s.finish())
            .ok_or_else(|| format!("geçersiz akış: {sh}"));
        emit(st, r)
    })
}

/// Türlerin etiketleri, JSON dizisi.
#[no_mangle]
pub extern "C" fn kalkan_kinds_json() -> u32 {
    with(|st| {
        let v: Vec<String> = Kind::ALL.iter().map(|k| format!("\"{}\"", k.label())).collect();
        emit(st, Ok(format!("[{}]", v.join(","))))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn call(f: impl FnOnce(*const u8, usize) -> u32, s: &str) -> Result<String, String> {
        let n = f(s.as_ptr(), s.len());
        let out = with(|st| String::from_utf8(st.out.clone()).unwrap());
        if n == ERR { Err(out) } else { Ok(out) }
    }

    #[test]
    fn roundtrip_through_c_abi() {
        let _g = TEST_LOCK.lock().unwrap();
        let terms = "Proje Atlas";
        let h = unsafe { kalkan_session_new(0, 0, 0, terms.as_ptr(), terms.len()) };
        assert!(h > 0);
        let t = "Çağrı: TC 10000000146, PROJE ATLAS";
        let m = call(|p, l| unsafe { kalkan_mask(h, p, l) }, t).unwrap();
        assert_eq!(m, "Çağrı: TC [TCKN_1], [OZEL_1]");
        assert_eq!(call(|p, l| unsafe { kalkan_restore(h, p, l) }, &m).unwrap(), t);

        // UTF-16 konumları: "Çağrı: TC " = 10 kod birimi.
        let j = call(|p, l| unsafe { kalkan_detect_json(h, p, l) }, t).unwrap();
        assert!(j.contains("\"start\":10,\"end\":21"), "{j}");

        let sh = kalkan_stream_new(h);
        let a = call(|p, l| unsafe { kalkan_stream_push(sh, p, l) }, "ok [TC").unwrap();
        let b = call(|p, l| unsafe { kalkan_stream_push(sh, p, l) }, "KN_1] bitti").unwrap();
        kalkan_stream_finish(sh);
        assert_eq!(a + &b, "ok 10000000146 bitti");

        let au = call(|p, l| unsafe { kalkan_audit_json(h, p, l) }, "[TCKN_5]").unwrap();
        assert!(au.starts_with("{\"clean\":false") && au.contains("[TCKN_5]"), "{au}");

        kalkan_session_free(h);
        assert!(call(|p, l| unsafe { kalkan_mask(h, p, l) }, "x").is_err());
        let bad = [0xffu8, 0xfe];
        assert!(call(|_, _| unsafe { kalkan_mask(1, bad.as_ptr(), 2) }, "").is_err());
    }

    #[test]
    fn freed_handles_are_reused() {
        let _g = TEST_LOCK.lock().unwrap();
        let h1 = unsafe { kalkan_session_new(0, 0, 0, std::ptr::null(), 0) };
        assert!(h1 > 0);
        kalkan_session_free(h1);
        let h2 = unsafe { kalkan_session_new(0, 0, 0, std::ptr::null(), 0) };
        assert_eq!(h1, h2);
        kalkan_session_free(h2);
    }

    #[test]
    fn free_uses_recorded_capacity() {
        let _g = TEST_LOCK.lock().unwrap();
        let p = kalkan_alloc(32);
        assert!(!p.is_null());
        unsafe { kalkan_free(p, 1) };
    }
}
