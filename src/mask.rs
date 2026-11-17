//! Maskeleme stratejileri, oturum kasası ve geri yükleme.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use crate::entity::{Kind, Match};
use crate::text::{tr_lower, tr_upper};
use crate::Kalkan;

/// Bulunan verinin yerine ne konacağı.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Strategy {
    /// `[TCKN_1]`, `[AD_SOYAD_2]` gibi numaralı yer tutucular. Geri yüklenebilir.
    /// Aynı değer (farklı yazılsa bile) hep aynı token'ı alır.
    #[default]
    Token,
    /// Biçimi korunmuş, sağlama toplamı **geçerli** sahte değerler
    /// (`10000000146` → `38291746502`). LLM gerçekçi veri görür, geri yüklenebilir.
    Synthetic,
    /// Kısmi maskeleme (`*********46`, `**** **** **** 1111`). Geri yüklenemez.
    Partial,
    /// Yalnızca tür etiketi (`[TCKN]`). Geri yüklenemez.
    Label,
}

/// Bir sohbet/istek boyunca tutulan kasa: gerçek değer ↔ yer tutucu eşlemesi.
///
/// ```
/// use kalkan::Kalkan;
/// let kalkan = Kalkan::new();
/// let mut oturum = kalkan.session();
/// let maskeli = oturum.mask("Müşteri TC 10000000146, tel 0532 123 45 67");
/// assert_eq!(maskeli, "Müşteri TC [TCKN_1], tel [TELEFON_1]");
/// let cevap = "[TCKN_1] numaralı müşteriyi [TELEFON_1] üzerinden arayın.";
/// assert_eq!(
///     oturum.restore(cevap),
///     "10000000146 numaralı müşteriyi 0532 123 45 67 üzerinden arayın."
/// );
/// ```
#[derive(Debug)]
pub struct Session {
    cfg: Kalkan,
    /// (tür, normalize değer) → yer tutucu
    forward: HashMap<(Kind, String), String>,
    /// yer tutucu → ilk görülen orijinal yazım
    reverse: HashMap<String, String>,
    counters: [u32; Kind::ALL.len()],
    /// Kasa değişene kadar yeniden kullanılan geri yükleme planı.
    restorer: Mutex<Option<Restorer<String>>>,
}

impl Clone for Session {
    fn clone(&self) -> Self {
        let restorer = self.restorer.lock().unwrap_or_else(|e| e.into_inner()).clone();
        Self {
            cfg: self.cfg.clone(),
            forward: self.forward.clone(),
            reverse: self.reverse.clone(),
            counters: self.counters,
            restorer: Mutex::new(restorer),
        }
    }
}

impl Session {
    pub(crate) fn new(cfg: Kalkan) -> Self {
        Self {
            cfg,
            forward: HashMap::new(),
            reverse: HashMap::new(),
            counters: [0; Kind::ALL.len()],
            restorer: Mutex::new(None),
        }
    }

    /// Metni maskeler.
    #[must_use]
    pub fn mask(&mut self, text: &str) -> String {
        self.mask_detailed(text).0
    }

    /// Metni maskeler ve bulunan eşleşmeleri de döndürür (kaynak metne göre aralıklar).
    #[must_use]
    pub fn mask_detailed(&mut self, text: &str) -> (String, Vec<Match>) {
        let matches = self.cfg.detect(text);
        let mut out = String::with_capacity(text.len() + matches.len() * 8);
        let mut last = 0;
        for m in &matches {
            out.push_str(&text[last..m.span.start]);
            let rep = self.replacement(m.kind, m.text(text));
            out.push_str(&rep);
            last = m.span.end;
        }
        out.push_str(&text[last..]);
        (out, matches)
    }

    /// LLM cevabındaki yer tutucuları gerçek değerlerle değiştirir.
    ///
    /// Geri yükleme planı kasa değişene kadar önbellekte tutulur; aynı oturumda
    /// birden fazla cevap geri yüklenirken kasa başına maliyet bir kez ödenir.
    #[must_use]
    pub fn restore(&self, text: &str) -> String {
        let mut cache = self.restorer.lock().unwrap_or_else(|e| e.into_inner());
        let plan = cache.get_or_insert_with(|| {
            Restorer::from_pairs(self.reverse.iter().map(|(k, v)| (k.clone(), v.clone())))
        });
        plan.replace_all(text)
    }

    /// Parça parça gelen (streaming) cevaplar için geri yükleyici.
    /// Bir yer tutucu iki parçaya bölünse bile (`"[TC"` + `"KN_1]"`) doğru çalışır.
    #[must_use]
    pub fn stream(&self) -> StreamRestorer {
        StreamRestorer::new(&self.reverse)
    }

    /// Kasadaki (yer tutucu, orijinal) çiftleri.
    pub fn vault(&self) -> impl Iterator<Item = (&str, &str)> {
        self.reverse.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Dışarıda saklanmış (yer tutucu, orijinal) çiftlerini kasaya yükler.
    /// Böylece maskeleme ve geri yükleme farklı süreçlerde/sunucularda yapılabilir.
    pub fn import_vault<I, A, B>(&mut self, pairs: I)
    where
        I: IntoIterator<Item = (A, B)>,
        A: Into<String>,
        B: Into<String>,
    {
        let mut changed = false;
        for (rep, orig) in pairs {
            self.reverse.insert(rep.into(), orig.into());
            changed = true;
        }
        if changed {
            self.restorer = Mutex::new(None);
        }
    }

    /// Oturumun kullandığı kalkan yapılandırması.
    #[must_use]
    pub fn config(&self) -> &Kalkan {
        &self.cfg
    }

    /// Kasadaki benzersiz değer sayısı.
    #[must_use]
    pub fn len(&self) -> usize {
        self.forward.len()
    }

    /// Kasa boş mu?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.forward.is_empty()
    }

    fn replacement(&mut self, kind: Kind, original: &str) -> String {
        match self.cfg.strategy {
            Strategy::Label => format!("[{}]", kind.label()),
            Strategy::Partial => partial(kind, original),
            Strategy::Token | Strategy::Synthetic => {
                let key = (kind, normalize(kind, original));
                if let Some(r) = self.forward.get(&key) {
                    return r.clone();
                }
                // Özel terimlerin "gerçekçi sahtesi" olmaz; sentetik modda da token kullanılır.
                let rep = if self.cfg.strategy == Strategy::Token || kind == Kind::Custom {
                    let c = &mut self.counters[kind as usize];
                    *c += 1;
                    format!("[{}_{}]", kind.label(), c)
                } else {
                    let mut attempt = 0u64;
                    loop {
                        let r = synthetic(kind, original, &key.1, self.cfg.secret, attempt);
                        if (r != original && !self.reverse.contains_key(&r)) || attempt > 32 {
                            break r;
                        }
                        attempt += 1;
                    }
                };
                self.forward.insert(key, rep.clone());
                self.reverse.insert(rep.clone(), original.to_string());
                self.restorer = Mutex::new(None);
                rep
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Normalizasyon: aynı değerin farklı yazımları aynı token'ı alsın.

fn digits_of(s: &str) -> String {
    s.bytes().filter(u8::is_ascii_digit).map(char::from).collect()
}

fn normalize(kind: Kind, s: &str) -> String {
    match kind {
        Kind::Phone => {
            let d = digits_of(s);
            // Ulusal 10 haneli çekirdek: +90 / 0090 / 0 önekini at.
            d[d.len().saturating_sub(10)..].to_string()
        }
        Kind::Tckn | Kind::Vkn | Kind::Card | Kind::Iban => digits_of(s),
        Kind::Email => s.to_ascii_lowercase(),
        Kind::Plate => s.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_uppercase(),
        Kind::Name | Kind::BirthDate => tr_lower(&s.split_whitespace().collect::<Vec<_>>().join(" ")),
        Kind::Custom => crate::text::fold(&s.split_whitespace().collect::<Vec<_>>().join(" ")),
        Kind::Ip => s.to_ascii_lowercase(),
        Kind::Passport => s.to_ascii_uppercase(),
    }
}

// ---------------------------------------------------------------------------
// Kısmi maskeleme

fn mask_alnum_except_last(s: &str, keep: usize, keep_letters: bool) -> String {
    let total = s.chars().filter(|c| c.is_ascii_digit()).count();
    let mut seen = 0;
    s.chars()
        .map(|c| {
            if c.is_ascii_digit() {
                seen += 1;
                if seen + keep > total { c } else { '*' }
            } else if c.is_alphabetic() && !keep_letters {
                '*'
            } else {
                c
            }
        })
        .collect()
}

fn partial(kind: Kind, s: &str) -> String {
    match kind {
        Kind::Tckn | Kind::Vkn | Kind::Phone => mask_alnum_except_last(s, 2, true),
        Kind::Card | Kind::Iban => mask_alnum_except_last(s, 4, true),
        Kind::Email => match s.split_once('@') {
            Some((local, domain)) => {
                let first: String = local.chars().take(1).collect();
                format!("{first}***@{domain}")
            }
            None => "***".into(),
        },
        Kind::Plate => {
            // İl kodu kalır: "34 ABC 123" → "34 *** ***"
            let mut out = String::with_capacity(s.len());
            for (i, c) in s.char_indices() {
                out.push(if i >= 2 && c.is_ascii_alphanumeric() { '*' } else { c });
            }
            out
        }
        Kind::Name => s
            .split(' ')
            .map(|w| {
                let mut cs = w.chars();
                match cs.next() {
                    Some(f) => format!("{f}***"),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
        Kind::Ip => match s.contains('.') {
            // "192.168.1.10" → "192.168.*.*"
            true => {
                let p: Vec<&str> = s.split('.').collect();
                format!("{}.{}.*.*", p[0], p.get(1).unwrap_or(&"*"))
            }
            // "2001:db8::1" → "2001:****"
            false => format!("{}:****", s.split(':').next().unwrap_or("")),
        },
        Kind::BirthDate => {
            // Yalnızca yıl kalır: "05.03.1990" → "**.**.1990", "5 Mart 1990" → "* **** 1990"
            let year_at = s.as_bytes().windows(4).position(|w| w.iter().all(u8::is_ascii_digit));
            s.char_indices()
                .map(|(i, c)| {
                    let in_year = year_at.is_some_and(|y| (y..y + 4).contains(&i));
                    if c.is_alphanumeric() && !in_year { '*' } else { c }
                })
                .collect()
        }
        Kind::Passport => {
            let first: String = s.chars().take(1).collect();
            format!("{first}{}", mask_alnum_except_last(&s[first.len()..], 2, true))
        }
        Kind::Custom => {
            let first: String = s.chars().take(1).collect();
            format!("{first}***")
        }
    }
}

// ---------------------------------------------------------------------------
// Sentetik (biçimi korunmuş, geçerli) değer üretimi

struct Rng(u64);

impl Rng {
    fn new(secret: u64, kind: Kind, value: &str, attempt: u64) -> Self {
        // FNV-1a ile tohum; SplitMix64 ile akış. Kriptografik değil, ama `secret`
        // olmadan sahte değerden gerçeğe gitmenin yolu yok (bilgi taşımaz).
        let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ secret;
        for b in value.bytes().chain([kind as u8, 0xFF]).chain(attempt.to_le_bytes()) {
            h ^= b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        Rng(h)
    }
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn digit(&mut self) -> u8 {
        b'0' + self.below(10) as u8
    }
    fn digits(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.digit()).collect()
    }
}

/// Orijinal yazımdaki rakamları sırayla yeni rakamlarla değiştirir; ayraçlar kalır.
fn fill_digits(template: &str, new_digits: &[u8]) -> String {
    let mut it = new_digits.iter();
    template
        .chars()
        .map(|c| if c.is_ascii_digit() { it.next().map_or(c, |&d| d as char) } else { c })
        .collect()
}

const FAKE_FIRST: &[&str] = &[
    "Aras", "Ece", "Kuzey", "Lale", "Mina", "Nehir", "Poyraz", "Rüzgar", "Sarp", "Tuana", "Umay", "Vera",
    "Yaz", "Zümra", "Atlas", "İpek", "Çağan", "Öykü", "Duru", "Toprak",
];
const FAKE_LAST: &[&str] = &[
    "Akdağ", "Bozkurtlu", "Çınaroğlu", "Demirkaya", "Erenler", "Göktaşlı", "Ilgaz", "Karapınar",
    "Mersinli", "Ortaköylü", "Sarıçam", "Tepeören", "Uludere", "Yamaçlı", "Zeytinli", "Kayalar",
];
const PLATE_LETTERS: &[u8] = b"ABCDEFGHIJKLMNOPRSTUVYZ";

fn synthetic(kind: Kind, original: &str, norm: &str, secret: u64, attempt: u64) -> String {
    use crate::checksum::*;
    let mut rng = Rng::new(secret, kind, norm, attempt);
    match kind {
        Kind::Tckn => {
            let mut d = rng.digits(9);
            d[0] = b'1' + rng.below(9) as u8;
            let (a, b) = tckn_check_digits(&d);
            d.extend([a, b]);
            fill_digits(original, &d)
        }
        Kind::Vkn => {
            let mut d = rng.digits(9);
            let c = vkn_check_digit(&d);
            d.push(c);
            fill_digits(original, &d)
        }
        Kind::Iban => {
            let mut bban = rng.digits(22);
            bban[5] = b'0'; // rezerv hane
            let check = tr_iban_check_digits(&bban);
            let mut d = check.to_vec();
            d.extend(bban);
            fill_digits(original, &d)
        }
        Kind::Card => {
            let n = original.bytes().filter(u8::is_ascii_digit).count();
            let first = original.bytes().find(u8::is_ascii_digit).unwrap_or(b'4');
            let mut body = vec![first];
            body.extend(rng.digits(n.saturating_sub(2)));
            let c = luhn_check_digit(&body);
            body.push(c);
            fill_digits(original, &body)
        }
        Kind::Phone => {
            // Önek ve operatör/alan kodu kalır, son 7 hane değişir.
            let d: Vec<u8> = original.bytes().filter(u8::is_ascii_digit).collect();
            let keep = d.len().saturating_sub(7);
            let mut nd = d[..keep].to_vec();
            nd.extend(rng.digits(7));
            fill_digits(original, &nd)
        }
        Kind::Email => format!("kisi{:06x}@example.com", rng.next() & 0xFF_FFFF),
        Kind::Plate => {
            let code = 1 + rng.below(81);
            let mut out = String::with_capacity(original.len());
            let mut nd = 0;
            for c in original.chars() {
                out.push(if c.is_ascii_digit() {
                    nd += 1;
                    match nd {
                        1 => (b'0' + (code / 10) as u8) as char,
                        2 => (b'0' + (code % 10) as u8) as char,
                        3 => (b'1' + rng.below(9) as u8) as char,
                        _ => rng.digit() as char,
                    }
                } else if c.is_ascii_uppercase() {
                    PLATE_LETTERS[rng.below(PLATE_LETTERS.len() as u64) as usize] as char
                } else {
                    c
                });
            }
            out
        }
        Kind::Ip => {
            if original.contains('.') {
                // Belgeleme için ayrılmış adres blokları (RFC 5737): gerçek bir cihaza ait olamaz.
                const NETS: [&str; 3] = ["192.0.2", "198.51.100", "203.0.113"];
                format!("{}.{}", NETS[rng.below(3) as usize], 1 + rng.below(254))
            } else {
                // RFC 3849 belgeleme öneki.
                format!("2001:db8:{:x}:{:x}::{:x}", rng.below(0xffff), rng.below(0xffff), 1 + rng.below(0xfffe))
            }
        }
        Kind::BirthDate => {
            let (y, m, d) = (1950 + rng.below(56), 1 + rng.below(12), 1 + rng.below(28));
            synth_date(original, y as u32, m as u32, d as u32)
        }
        Kind::Passport => {
            let first: String = original.chars().take(1).collect();
            let n = original.len() - first.len();
            let digits: String = rng.digits(n).into_iter().map(char::from).collect();
            format!("{first}{digits}")
        }
        Kind::Custom => format!("OZEL_{:06x}", rng.next() & 0xFF_FFFF),
        Kind::Name => {
            let words: Vec<&str> = original.split_whitespace().collect();
            let upper = original.chars().filter(|c| c.is_alphabetic()).all(char::is_uppercase);
            let n = words.len().max(1);
            let parts: Vec<String> = (0..n)
                .map(|i| {
                    // Son kelime soyad havuzundan, diğerleri ad havuzundan.
                    let pool = if n > 1 && i == n - 1 { FAKE_LAST } else { FAKE_FIRST };
                    let w = pool[rng.below(pool.len() as u64) as usize];
                    if upper { tr_upper(w) } else { w.to_string() }
                })
                .collect();
            parts.join(" ")
        }
    }
}

/// Orijinal tarih biçimini (ayraç, sıfır doldurma, yazıyla ay, büyük harf) koruyarak
/// yeni bir tarih yazar.
fn synth_date(original: &str, y: u32, m: u32, d: u32) -> String {
    const AYLAR: [&str; 12] = [
        "Ocak", "Şubat", "Mart", "Nisan", "Mayıs", "Haziran", "Temmuz", "Ağustos", "Eylül", "Ekim", "Kasım", "Aralık",
    ];
    let b = original.as_bytes();
    if b.len() >= 5 && b[..4].iter().all(u8::is_ascii_digit) && b[4] == b'-' {
        return format!("{y:04}-{m:02}-{d:02}");
    }
    let day_len = b.iter().take_while(|c| c.is_ascii_digit()).count();
    let dd = if day_len == 2 { format!("{d:02}") } else { d.to_string() };
    match b.get(day_len) {
        Some(&sep @ (b'.' | b'/' | b'-')) => {
            let month_len = b[day_len + 1..].iter().take_while(|c| c.is_ascii_digit()).count();
            let mm = if month_len == 2 { format!("{m:02}") } else { m.to_string() };
            let sep = sep as char;
            format!("{dd}{sep}{mm}{sep}{y}")
        }
        _ => {
            let upper = original.chars().filter(|c| c.is_alphabetic()).all(char::is_uppercase);
            let ay = AYLAR[m as usize - 1];
            format!("{dd} {} {y}", if upper { tr_upper(ay) } else { ay.to_string() })
        }
    }
}

// ---------------------------------------------------------------------------
// Geri yükleme

/// İlk bayta göre indekslenmiş çoklu desen değiştirici. `S`, ödünç (`&str`)
/// veya sahipli (`String`) olabilir.
#[derive(Debug, Clone)]
struct Restorer<S> {
    by_first: HashMap<u8, Vec<(S, S)>>,
}

impl<S: AsRef<str>> Restorer<S> {
    fn from_pairs(pairs: impl IntoIterator<Item = (S, S)>) -> Self {
        let mut by_first: HashMap<u8, Vec<(S, S)>> = HashMap::new();
        for (k, v) in pairs {
            if let Some(&b) = k.as_ref().as_bytes().first() {
                by_first.entry(b).or_default().push((k, v));
            }
        }
        for v in by_first.values_mut() {
            v.sort_unstable_by_key(|(k, _)| core::cmp::Reverse(k.as_ref().len())); // uzun olan önce
        }
        Self { by_first }
    }

    fn replace_all(&self, text: &str) -> String {
        self.replace_until(text, None).0
    }

    /// Soldan sağa tarar ve tam eşleşmeleri değiştirir. `pending` verilirse
    /// (akış modu), metnin *sonuna kadar uzanan* ve bir anahtarın ön eki olan
    /// bölgede durur: (çıktı, tüketilen bayt). Tam eşleşme her zaman önceliklidir;
    /// böylece biten bir anahtarın kuyruğu başka bir anahtarın başı gibi
    /// görünse bile (ör. `...220` + `0532...`) kesilmez.
    fn replace_until(&self, text: &str, pending: Option<(&HashSet<String>, usize)>) -> (String, usize) {
        let b = text.as_bytes();
        let n = b.len();
        let mut out = String::with_capacity(n);
        let mut last = 0;
        let mut i = 0;
        while i < n {
            if let Some((prefixes, max_len)) = pending {
                if n - i < max_len && text.is_char_boundary(i) && prefixes.contains(&text[i..]) {
                    break; // devamı gelecek olabilir: buradan sonrasını beklet
                }
            }
            if let Some(cands) = self.by_first.get(&b[i]) {
                if let Some((k, v)) = cands.iter().find(|(k, _)| b[i..].starts_with(k.as_ref().as_bytes())) {
                    out.push_str(&text[last..i]);
                    out.push_str(v.as_ref());
                    i += k.as_ref().len();
                    last = i;
                    continue;
                }
            }
            i += 1;
        }
        out.push_str(&text[last..i]);
        (out, i)
    }
}

/// Akış (streaming) cevaplar için geri yükleyici. Her parçayı `push` ile ver,
/// dönen metni kullanıcıya hemen ilet; sonda `finish` çağır.
///
/// Yalnızca bir yer tutucunun başlangıcı olabilecek kadar kuyruğu bekletir,
/// geri kalanı gecikmesiz akar. Oturumdan bağımsızdır (kasanın o anki kopyasını
/// taşır), bu yüzden başka bir iş parçacığına/görev'e taşınabilir.
pub struct StreamRestorer {
    restorer: Restorer<String>,
    prefixes: HashSet<String>,
    max_len: usize,
    buf: String,
}

impl StreamRestorer {
    fn new(map: &HashMap<String, String>) -> Self {
        let mut prefixes = HashSet::new();
        let mut max_len = 0;
        for k in map.keys() {
            max_len = max_len.max(k.len());
            for (i, _) in k.char_indices().skip(1) {
                prefixes.insert(k[..i].to_string());
            }
        }
        let restorer = Restorer::from_pairs(map.iter().map(|(k, v)| (k.clone(), v.clone())));
        Self { restorer, prefixes, max_len, buf: String::new() }
    }

    /// Yeni parçayı ekler; güvenle yayınlanabilecek (geri yüklenmiş) metni döndürür.
    #[must_use]
    pub fn push(&mut self, chunk: &str) -> String {
        self.buf.push_str(chunk);
        let (ready, used) = self.restorer.replace_until(&self.buf, Some((&self.prefixes, self.max_len)));
        self.buf.drain(..used);
        ready
    }

    /// Akış bitti: bekletilen kuyruğu da geri yükleyip döndürür.
    #[must_use]
    pub fn finish(&mut self) -> String {
        let rest = std::mem::take(&mut self.buf);
        self.restorer.replace_all(&rest)
    }
}

// ---------------------------------------------------------------------------
// Sızıntı denetimi

/// Model cevabının (geri yüklemeden önceki ham hali) denetim raporu.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LeakReport {
    /// Cevapta geçen ama bizim maskelemediğimiz kişisel veriler: model uydurmuş,
    /// eğitim verisinden hatırlamış veya başka bir kanaldan (araç sonucu vb.) almış olabilir.
    pub new_pii: Vec<(Kind, String)>,
    /// Modele hiç gitmemesi gereken **gerçek** değerlerin cevapta görünmesi.
    /// Boş değilse bir yerde maskeleme atlanmış demektir.
    pub echoed_originals: Vec<String>,
    /// Kasada karşılığı olmayan yer tutucular (ör. model `[TCKN_7]` uydurdu).
    /// Geri yüklenemezler ve kullanıcıya ham haliyle görünürler.
    pub unknown_placeholders: Vec<String>,
}

impl LeakReport {
    /// Hiçbir bulgu yoksa true.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.new_pii.is_empty() && self.echoed_originals.is_empty() && self.unknown_placeholders.is_empty()
    }
}

impl Session {
    /// Modelin ham cevabını denetler. Geri yüklemeden **önce** çağırın.
    ///
    /// ```
    /// let mut s = kalkan::Kalkan::new().session();
    /// let _ = s.mask("TC 10000000146");
    /// let r = s.audit("[TCKN_1] ile [TCKN_2] eşleşmiyor; IBAN TR33 0006 1005 1978 6457 8413 26");
    /// assert_eq!(r.unknown_placeholders, ["[TCKN_2]"]);
    /// assert_eq!(r.new_pii.len(), 1); // model bir IBAN üretti
    /// assert!(!r.is_clean());
    /// ```
    #[must_use]
    pub fn audit(&self, raw_response: &str) -> LeakReport {
        let mut r = LeakReport::default();
        let synthetic = self.reverse.keys().map(String::as_str).collect::<HashSet<_>>();
        let originals = self.reverse.values().map(String::as_str).collect::<HashSet<_>>();
        let mut seen_echoed = HashSet::new();
        let mut seen_unknown = HashSet::new();

        for m in self.cfg.detect(raw_response) {
            let t = m.text(raw_response);
            if synthetic.contains(t) {
                continue; // bizim sentetik değerimiz
            }
            if originals.contains(t) {
                if seen_echoed.insert(t) {
                    r.echoed_originals.push(t.to_string());
                }
            } else {
                r.new_pii.push((m.kind, t.to_string()));
            }
        }
        // Tespit edilmeyen biçimde geri dönen orijinaller (ör. gövdesi değişmiş metin).
        for &o in &originals {
            if raw_response.contains(o) && seen_echoed.insert(o) {
                r.echoed_originals.push(o.to_string());
            }
        }
        // "[ETIKET_n]" biçimli ama kasada olmayan yer tutucular.
        let b = raw_response.as_bytes();
        let mut i = 0;
        while let Some(off) = b[i..].iter().position(|&c| c == b'[') {
            let s = i + off;
            i = s + 1;
            let Some(len) = b[s..].iter().take(24).position(|&c| c == b']') else { continue };
            let tok = &raw_response[s..s + len + 1];
            let inner = &tok[1..tok.len() - 1];
            let Some((label, num)) = inner.rsplit_once('_') else { continue };
            let known = Kind::ALL.iter().any(|k| k.label() == label);
            if known
                && !num.is_empty()
                && num.bytes().all(|c| c.is_ascii_digit())
                && !synthetic.contains(tok)
                && seen_unknown.insert(tok)
            {
                r.unknown_placeholders.push(tok.to_string());
            }
        }
        r
    }
}
