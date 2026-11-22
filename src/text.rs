//! Metin yardımcıları: sınır kontrolleri ve Türkçe'ye duyarlı harf dönüşümleri.

/// `i` bayt konumundan önceki karakter.
#[inline]
pub fn prev_char(s: &str, i: usize) -> Option<char> {
    s[..i].chars().next_back()
}

/// `i` bayt konumundaki karakter.
#[inline]
pub fn next_char(s: &str, i: usize) -> Option<char> {
    s[i..].chars().next()
}

/// Konumdan önce harf/rakam yoksa (kelime sınırı) true.
#[inline]
pub fn boundary_before(s: &str, i: usize) -> bool {
    match s.as_bytes().get(i.wrapping_sub(1)) {
        None => true,
        Some(b) if b.is_ascii() => !b.is_ascii_alphanumeric(),
        Some(_) => !prev_char(s, i).is_some_and(char::is_alphanumeric),
    }
}

/// Konumda harf/rakam yoksa (kelime sınırı) true.
#[inline]
pub fn boundary_after(s: &str, i: usize) -> bool {
    match s.as_bytes().get(i) {
        None => true,
        Some(b) if b.is_ascii() => !b.is_ascii_alphanumeric(),
        Some(_) => !next_char(s, i).is_some_and(char::is_alphanumeric),
    }
}

/// Türkçe küçük harfe çevirme: I→ı, İ→i (Rust'ın varsayılanı İ'yi "i̇" yapar).
pub fn tr_lower(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'I' => out.push('ı'),
            'İ' => out.push('i'),
            _ => out.extend(c.to_lowercase()),
        }
    }
    out
}

/// Türkçe büyük harfe çevirme: i→İ, ı→I.
pub fn tr_upper(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'i' => out.push('İ'),
            'ı' => out.push('I'),
            _ => out.extend(c.to_uppercase()),
        }
    }
    out
}

/// Karşılaştırma için "katlama": küçük harf + I/ı/İ/i ayrımını kaldırır.
/// Böylece hem Türkçe (`IŞIK` ↔ `ışık`) hem İngilizce (`HOLDING` ↔ `holding`)
/// büyük harfli yazımlar eşleşir.
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'I' | 'İ' | 'ı' | 'i' => out.push('i'),
            _ => out.extend(c.to_lowercase()),
        }
    }
    out
}

/// [`fold`] + katlanmış metnin her baytının kaynak metindeki bayt konumu.
/// Haritanın uzunluğu `folded.len() + 1`'dir (son eleman `s.len()`).
pub fn fold_with_map(s: &str) -> (String, Vec<usize>) {
    let mut out = String::with_capacity(s.len());
    let mut map = Vec::with_capacity(s.len() + 1);
    for (i, c) in s.char_indices() {
        let before = out.len();
        match c {
            'I' | 'İ' | 'ı' | 'i' => out.push('i'),
            _ => out.extend(c.to_lowercase()),
        }
        map.extend(core::iter::repeat_n(i, out.len() - before));
    }
    map.push(s.len());
    (out, map)
}

/// Tek karakterin Türkçe küçük harfi (çok karakterli dönüşümlerde ilk karakter).
#[inline]
fn tr_lower_char(c: char) -> char {
    match c {
        'I' => 'ı',
        'İ' => 'i',
        c if c.is_ascii() => c.to_ascii_lowercase(),
        c => c.to_lowercase().next().unwrap_or(c),
    }
}

/// `s`, küçük harfli `kw` ile (Türkçe büyük/küçük harf duyarsız) başlıyor mu?
/// Bellek tahsisi yapmaz.
#[inline]
pub fn starts_with_tr_ci(s: &str, kw: &str) -> bool {
    let mut it = s.chars();
    kw.chars().all(|k| it.next().is_some_and(|c| tr_lower_char(c) == k))
}

/// `a` ile küçük harfli `b` Türkçe büyük/küçük harf duyarsız eşit mi? (tahsissiz)
#[inline]
pub fn eq_tr_ci(a: &str, b: &str) -> bool {
    let mut ia = a.chars();
    b.chars().all(|k| ia.next().is_some_and(|c| tr_lower_char(c) == k)) && ia.next().is_none()
}

/// `p` konumundaki harfin anahtar kelime karşılaştırmasındaki küçük ASCII karşılığı.
/// "İ" (C4 B0) iki baytlıktır ama küçüğü ASCII "i"dir ("İkamet", "İşyeri").
#[inline]
fn keyword_initial(b: &[u8], p: usize, c: u8) -> Option<u8> {
    if c.is_ascii_alphabetic() {
        Some(c.to_ascii_lowercase())
    } else if c == 0xC4 && b.get(p + 1) == Some(&0xB0) {
        Some(b'i')
    } else {
        None
    }
}

/// `i` konumundan geriye `window` baytlık bölgede verilen (küçük harfli) anahtar
/// kelimelerden biri geçiyor mu? Türkçe büyük/küçük harf duyarsız, tahsissiz.
pub fn context_has(s: &str, i: usize, window: usize, keywords: &[&str]) -> bool {
    debug_assert!(keywords.iter().all(|k| k.as_bytes().first().is_some_and(u8::is_ascii_lowercase)));
    let mut start = i.saturating_sub(window);
    while !s.is_char_boundary(start) {
        start += 1;
    }
    let ctx = &s[start..i];
    // Tüm anahtar kelimeler ASCII harfle başlar: önce baytla ilk harfi eşle,
    // yalnızca tutarsa (Unicode'a duyarlı) tam karşılaştırma yap.
    ctx.bytes().enumerate().any(|(p, c)| {
        keyword_initial(ctx.as_bytes(), p, c).is_some_and(|lc| {
            keywords.iter().any(|k| k.as_bytes().first() == Some(&lc) && starts_with_tr_ci(&ctx[p..], k))
        })
    })
}

/// `context_has`'ın sağ tarafa bakan karşılığı: `i`'den sonraki `window` bayt içinde
/// anahtar kelimelerden biri başlıyor mu ("84883037 numaralı SGK sicil").
pub fn context_after_has(s: &str, i: usize, window: usize, keywords: &[&str]) -> bool {
    debug_assert!(keywords.iter().all(|k| k.as_bytes().first().is_some_and(u8::is_ascii_lowercase)));
    let mut end = (i + window).min(s.len());
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    // Başka bir değere ait bağlamı almamak için ilk noktalamada dur ("123, sicil 45").
    let ctx = &s[i..end];
    let cut = ctx.bytes().position(|c| matches!(c, b',' | b';' | b':' | b'\n' | b'.')).unwrap_or(ctx.len());
    let ctx = &ctx[..cut];
    ctx.bytes().enumerate().any(|(p, c)| {
        (p == 0 || !ctx.as_bytes()[p - 1].is_ascii_alphabetic())
            && keyword_initial(ctx.as_bytes(), p, c).is_some_and(|lc| {
                keywords.iter().any(|k| k.as_bytes().first() == Some(&lc) && starts_with_tr_ci(&ctx[p..], k))
            })
    })
}

/// `i`'deki iki baytlık Türkçe harf: (Türkçe küçük harfi, büyük harf mi).
pub(crate) fn tr_letter(b: &[u8], i: usize) -> Option<(&'static str, bool)> {
    Some(match (b[i], *b.get(i + 1)?) {
        (0xC3, 0xA7) => ("ç", false),
        (0xC3, 0x87) => ("ç", true),
        (0xC3, 0xB6) => ("ö", false),
        (0xC3, 0x96) => ("ö", true),
        (0xC3, 0xBC) => ("ü", false),
        (0xC3, 0x9C) => ("ü", true),
        (0xC4, 0x9F) => ("ğ", false),
        (0xC4, 0x9E) => ("ğ", true),
        (0xC4, 0xB1) => ("ı", false),
        (0xC4, 0xB0) => ("i", true),
        (0xC5, 0x9F) => ("ş", false),
        (0xC5, 0x9E) => ("ş", true),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_dotted_capital_i() {
        assert!(context_has("İkamet adresi: ", 15, 15, &["ikamet"]));
        assert!(context_after_has(" İşyeri sicil", 0, 20, &["işyeri"]));
    }

    #[test]
    fn turkish_case() {
        assert_eq!(tr_lower("VERGİ KIMLIK"), "vergi kımlık");
        assert_eq!(tr_upper("ışık iğne"), "IŞIK İĞNE");
        assert_eq!(fold("PROJE IŞIK"), fold("proje ışık"));
        assert_eq!(fold("ACME HOLDING"), fold("Acme Holding"));
        assert!(eq_tr_ci("AĞUSTOS", "ağustos") && eq_tr_ci("Mart", "mart") && !eq_tr_ci("Marta", "mart"));
        let (a, b, c) = ("DOĞUM TARİHİ: ", "VERGİ NO ", "fatura ");
        assert!(context_has(a, a.len(), 40, &["doğum"]));
        assert!(context_has(b, b.len(), 40, &["vergi"]) && !context_has(c, c.len(), 40, &["vergi"]));
    }

    #[test]
    fn boundaries() {
        let s = "aÇ12 x";
        assert!(!boundary_before(s, 3)); // 'Ç' harf
        assert!(boundary_after(s, 5));
    }
}
