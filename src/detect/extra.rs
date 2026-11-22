//! IP adresi, doğum tarihi, pasaport ve SGK sicil no dedektörleri.

use crate::entity::{Kind, Match};
use crate::text::{boundary_after, boundary_before, context_after_has, context_has, eq_tr_ci, starts_with_tr_ci};

const VERSION_WORDS: &[&str] = &["sürüm", "versiyon", "version", "ver.", "build", "release", "v."];
const BIRTH_WORDS: &[&str] = &["doğum", "dogum", "d.tarihi", "d. tarihi", "d.t.", "doğ.", "birth", "dob"];
const PASSPORT_WORDS: &[&str] = &["pasaport", "passport"];
const SGK_WORDS: &[&str] = &[
    "sgk", "ssk", "bağ-kur", "bağkur", "bag-kur", "bagkur", "emekli sicil", "sigorta sicil",
    "sosyal güvenlik", "sosyal guvenlik", "sigortalı sicil", "sigortali sicil", "sigortalı no",
    "sigortali no", "sigortalı numara", "sigortali numara", "sigortalılık", "sigortalilik",
    "emekli sandığı", "emekli sandigi", "tahsis no", "tahsis numara", "işyeri sicil", "isyeri sicil",
];
/// SGK etiketiyle numara arasında bunlardan biri varsa numara SGK'nın değildir.
const SGK_PHONE_WORDS: &[&str] = &["tel", "cep", "gsm", "whatsapp", "faks", "fax"];
/// Numaradan sonra gelen SGK bağlamı: "84883037 numaralı SGK sicil", "85387200 sicil numaralı".
const SGK_AFTER_WORDS: &[&str] = &["sgk", "ssk", "bağ-kur", "bagkur", "bağkur", "sicil"];
/// Tarihten sonra gelen doğum bağlamı: "12.03.1990 doğumlu".
const BIRTH_AFTER_WORDS: &[&str] = &["doğumlu", "dogumlu", "doğum", "dogum", "doğdu", "dogdu"];
const MONTHS: &[&str] = &[
    "ocak", "şubat", "mart", "nisan", "mayıs", "haziran", "temmuz", "ağustos", "eylül", "ekim", "kasım", "aralık",
];

// ---------------------------------------------------------------- IPv4 / IPv6

pub(crate) fn scan_ipv4(text: &str, out: &mut Vec<Match>) {
    let b = text.as_bytes();
    let n = b.len();
    let mut i = 0;
    while i < n {
        let Some(off) = b[i..].iter().position(u8::is_ascii_digit) else { break };
        i += off;
        let prev_dot = i > 0 && b[i - 1] == b'.';
        if !boundary_before(text, i) || prev_dot {
            while i < n && (b[i].is_ascii_digit() || b[i] == b'.') {
                i += 1;
            }
            continue;
        }
        match parse_ipv4(b, i) {
            Some(end) if boundary_after(text, end) && !(end + 1 < n && b[end] == b'.' && b[end + 1].is_ascii_digit()) => {
                // "sürüm 3.6.0.84" ya da "3.6.0.84 sürümünden": sürüm numarası.
                let version = context_has(text, i, 24, VERSION_WORDS) || context_after_has(text, end, 12, VERSION_WORDS);
                let conf = if version { 50 } else { 85 };
                out.push(Match::new(Kind::Ip, i..end, conf));
                i = end;
            }
            _ => {
                while i < n && (b[i].is_ascii_digit() || b[i] == b'.') {
                    i += 1;
                }
            }
        }
    }
}

fn parse_ipv4(b: &[u8], mut i: usize) -> Option<usize> {
    for octet in 0..4 {
        if octet > 0 {
            if b.get(i) != Some(&b'.') {
                return None;
            }
            i += 1;
        }
        let s = i;
        while i < b.len() && b[i].is_ascii_digit() && i - s < 4 {
            i += 1;
        }
        let len = i - s;
        if len == 0 || len > 3 || (len > 1 && b[s] == b'0') {
            return None;
        }
        let v: u32 = b[s..i].iter().fold(0, |a, &d| a * 10 + (d - b'0') as u32);
        if v > 255 {
            return None;
        }
    }
    Some(i)
}

pub(crate) fn scan_ipv6(text: &str, out: &mut Vec<Match>) {
    let b = text.as_bytes();
    let n = b.len();
    let is_c = |c: u8| c.is_ascii_hexdigit() || c == b':';
    let mut last_end = 0;
    let mut from = 0;
    // Yalnızca ':' çevresine bakarız; düz metindeki a-f harflerine takılmayız.
    while let Some(off) = b[from..].iter().position(|&c| c == b':') {
        let colon = from + off;
        from = colon + 1;
        if colon < last_end {
            continue;
        }
        let mut s = colon;
        while s > 0 && is_c(b[s - 1]) {
            s -= 1;
        }
        let mut e = colon;
        while e < n && is_c(b[e]) {
            e += 1;
        }
        from = from.max(e);
        if valid_ipv6(&b[s..e]) && boundary_before(text, s) && boundary_after(text, e) {
            out.push(Match::new(Kind::Ip, s..e, 85));
            last_end = e;
        }
    }
}

fn valid_ipv6(s: &[u8]) -> bool {
    let txt = match core::str::from_utf8(s) {
        Ok(t) => t,
        Err(_) => return false,
    };
    let double = txt.matches("::").count();
    if double > 1 || txt.contains(":::") || txt.matches(':').count() < 2 {
        return false;
    }
    let groups: Vec<&str> = txt.split(':').filter(|g| !g.is_empty()).collect();
    if groups.iter().any(|g| g.len() > 4) {
        return false;
    }
    let hex_digits: usize = groups.iter().map(|g| g.len()).sum();
    if double == 1 {
        // "a::b" gibi kısa kelime kalıplarını ele: en az 2 grup ve 4 hane.
        groups.len() >= 2 && groups.len() <= 7 && hex_digits >= 4
    } else {
        // Tam yazım 8 grup ister; "14:30:00" saatleri ve MAC adresleri elenir.
        groups.len() == 8 && !txt.starts_with(':') && !txt.ends_with(':')
    }
}

// ---------------------------------------------------------------- Doğum tarihi

pub(crate) fn scan_birth_date(text: &str, out: &mut Vec<Match>) {
    let b = text.as_bytes();
    let n = b.len();
    let mut i = 0;
    while i < n {
        let Some(off) = b[i..].iter().position(u8::is_ascii_digit) else { break };
        i += off;
        if !boundary_before(text, i) {
            while i < n && b[i].is_ascii_digit() {
                i += 1;
            }
            continue;
        }
        if let Some(end) = numeric_date(b, i).or_else(|| textual_date(text, i)) {
            // Soldaki etiket bu tarihe ait olmalı ("doğum tarihim 19.02.1983, sipariş
            // tarihim 02.11.2026" ikinci tarihi kapsamaz) ve tarih başka bir olayın
            // tarihi olmamalı ("07.07.2026 tarihinde işe başladım").
            let left = birth_label_before(text, i, 60).is_some() && !followed_by_tarih(text, end);
            // Sağda: "1990 doğumlu", "20.02.2021 tarihinde doğdu".
            let ctx = left || context_after_has(text, end, 20, BIRTH_AFTER_WORDS);
            if boundary_after(text, end) && ctx && !in_future(b, i, end) {
                out.push(Match::new(Kind::BirthDate, i..end, 90));
                i = end;
                continue;
            }
        } else if let Some(end) = birth_year(text, i).or_else(|| birth_day_month(text, i)) {
            out.push(Match::new(Kind::BirthDate, i..end, 85));
            i = end;
            continue;
        }
        while i < n && b[i].is_ascii_digit() {
            i += 1;
        }
    }
}

/// Tarihin solundaki son doğum etiketi bu tarihe mi ait? Aitse etiket öbeğinden
/// ("doğum", "doğum tarihim", "doğum yılınız") sonra tarihe kadar kalan metni döndürür.
/// Arada başka bir tarih etiketi varsa ("…, sipariş tarihim") etiket bu tarihin değildir.
fn birth_label_before(text: &str, i: usize, window: usize) -> Option<&str> {
    let mut start = i.saturating_sub(window);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    let ctx = &text[start..i];
    // Ucuz ret: tüm etiketler "do", "d.", "bi" ile başlar (büyük/küçük harf).
    let cb = ctx.as_bytes();
    if !cb.windows(2).any(|w| matches!((w[0] | 0x20, w[1] | 0x20), (b'd', b'o') | (b'b', b'i')) || (w[0] | 0x20 == b'd' && w[1] == b'.')) {
        return None;
    }
    let (p, kw) = ctx
        .char_indices()
        .filter(|&(p, _)| boundary_before(ctx, p))
        .filter_map(|(p, _)| BIRTH_WORDS.iter().find(|k| starts_with_tr_ci(&ctx[p..], k)).map(|k| (p, *k)))
        .next_back()?;
    // "tahmini / beklenen doğum tarihi" bir doğumun tarihidir, kişinin doğum tarihi değil.
    let prev_word = ctx[..p].trim_end().rsplit(|c: char| !c.is_alphabetic()).next().unwrap_or("");
    if ["tahmini", "beklenen"].iter().any(|k| eq_tr_ci(prev_word, k)) {
        return None;
    }
    // Etiket sözcüğünün geri kalanını ("doğum|lu", "doğum|unuz") atla.
    let mut q = p + ctx[p..].chars().take(kw.chars().count()).map(char::len_utf8).sum::<usize>();
    q += word_len(&ctx[q..]);
    // "doğum tarihi / günü / yılı" öbeğini tek etiket say.
    let after_ws = q + (ctx[q..].len() - ctx[q..].trim_start().len());
    let next = &ctx[after_ws..];
    if ["tarih", "gün", "gun", "yıl", "yil"].iter().any(|k| starts_with_tr_ci(next, k)) {
        q = after_ws + word_len(next);
    }
    let rest = &ctx[q..];
    let other_date_label = rest
        .char_indices()
        .any(|(r, _)| boundary_before(rest, r) && starts_with_tr_ci(&rest[r..], "tarih"));
    (!other_date_label).then_some(rest)
}

/// Baştaki harf dizisinin bayt uzunluğu.
fn word_len(s: &str) -> usize {
    s.char_indices().find(|(_, c)| !c.is_alphabetic()).map_or(s.len(), |(k, _)| k)
}

/// Etiket ile değer arasında yalnızca boşluk ve ":" / "(" var mı ("doğum yılım: 1998")?
fn label_adjacent(rest: &str) -> bool {
    rest.chars().all(|c| c.is_whitespace() || matches!(c, ':' | '(' | '-'))
}

/// Tarihten hemen sonra "tarihinde / tarihli" geliyor mu?
fn followed_by_tarih(text: &str, end: usize) -> bool {
    starts_with_tr_ci(text[end..].trim_start(), "tarih")
}

/// Tek başına doğum yılı: "1959 doğumlu", "doğum yılım 1998".
fn birth_year(text: &str, i: usize) -> Option<usize> {
    let b = text.as_bytes();
    let (y, end) = read_num(b, i, 4, 4)?;
    if !(1900..=current_year().unwrap_or(2030)).contains(&y) || !boundary_after(text, end) {
        return None;
    }
    let after = text[end..].trim_start();
    let dogumlu = ["doğumlu", "dogumlu"].iter().any(|k| starts_with_tr_ci(after, k));
    let labelled = birth_label_before(text, i, 32).is_some_and(label_adjacent);
    (dogumlu || labelled).then_some(end)
}

/// Yılsız doğum günü, yalnızca etiketin hemen ardından: "doğum günüm 7 Nisan".
fn birth_day_month(text: &str, i: usize) -> Option<usize> {
    let b = text.as_bytes();
    let (d, j) = read_num(b, i, 1, 2)?;
    if b.get(j) != Some(&b' ') {
        return None;
    }
    let we = j + 1 + word_len(&text[j + 1..]);
    let month = MONTHS.iter().position(|&m| eq_tr_ci(&text[j + 1..we], m))? as u32 + 1;
    // Yılı izleyen biçimler textual_date'e kalır; 2000 artık yıl olduğu için 29 Şubat geçer.
    let label = birth_label_before(text, i, 40).is_some_and(label_adjacent);
    (valid_dmy(d, month, 2000) && label).then_some(we)
}

/// Doğum tarihi gelecekte olamaz: "teslimat 19.03.2027" gibi tarihleri eler.
/// Saat erişimi olmayan hedeflerde (wasm) denetim yapılmaz.
fn in_future(b: &[u8], s: usize, e: usize) -> bool {
    let year = b[s..e]
        .windows(4)
        .enumerate()
        .filter(|(k, w)| {
            w.iter().all(u8::is_ascii_digit)
                && (*k == 0 || !b[s + k - 1].is_ascii_digit())
                && b.get(s + k + 4).is_none_or(|c| !c.is_ascii_digit())
        })
        .map(|(_, w)| w.iter().fold(0u32, |a, &d| a * 10 + (d - b'0') as u32))
        .next();
    matches!((year, current_year()), (Some(y), Some(c)) if y > c)
}

#[cfg(not(target_arch = "wasm32"))]
fn current_year() -> Option<u32> {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    // Gregoryen takvimde gün sayısından yıl (H. Hinnant, civil_from_days).
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    Some((yoe + era * 400 + i64::from(m <= 2)) as u32)
}

#[cfg(target_arch = "wasm32")]
fn current_year() -> Option<u32> {
    None
}

fn read_num(b: &[u8], i: usize, min: usize, max: usize) -> Option<(u32, usize)> {
    let mut j = i;
    while j < b.len() && b[j].is_ascii_digit() && j - i < max {
        j += 1;
    }
    if j - i < min || (j < b.len() && b[j].is_ascii_digit()) {
        return None;
    }
    Some((b[i..j].iter().fold(0, |a, &d| a * 10 + (d - b'0') as u32), j))
}

fn valid_dmy(d: u32, m: u32, y: u32) -> bool {
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&d) && (1900..=2030).contains(&y)
}

/// 05.03.1990, 5/3/1990, 05-03-1990, 1990-03-05
fn numeric_date(b: &[u8], i: usize) -> Option<usize> {
    let sep_ok = |c: Option<&u8>| matches!(c, Some(b'.' | b'/' | b'-'));
    // yyyy-mm-dd
    if let Some((y, j)) = read_num(b, i, 4, 4) {
        if b.get(j) == Some(&b'-') {
            let (m, k) = read_num(b, j + 1, 1, 2)?;
            if b.get(k) != Some(&b'-') {
                return None;
            }
            let (d, e) = read_num(b, k + 1, 1, 2)?;
            return valid_dmy(d, m, y).then_some(e);
        }
        return None;
    }
    let (d, j) = read_num(b, i, 1, 2)?;
    if !sep_ok(b.get(j)) {
        return None;
    }
    let sep = b[j];
    let (m, k) = read_num(b, j + 1, 1, 2)?;
    if b.get(k) != Some(&sep) {
        return None;
    }
    let (y, e) = read_num(b, k + 1, 4, 4)?;
    valid_dmy(d, m, y).then_some(e)
}

/// "5 Mart 1990", "05 AĞUSTOS 1987"
fn textual_date(text: &str, i: usize) -> Option<usize> {
    let b = text.as_bytes();
    let (d, j) = read_num(b, i, 1, 2)?;
    if b.get(j) != Some(&b' ') {
        return None;
    }
    let ws = j + 1;
    let we = text[ws..].char_indices().find(|(_, c)| !c.is_alphabetic()).map_or(text.len(), |(k, _)| ws + k);
    if we == ws || we - ws > 16 {
        return None;
    }
    let word = &text[ws..we];
    let month = MONTHS.iter().position(|&m| eq_tr_ci(word, m))? as u32 + 1;
    if b.get(we) != Some(&b' ') {
        return None;
    }
    let (y, e) = read_num(b, we + 1, 4, 4)?;
    valid_dmy(d, month, y).then_some(e)
}

// ---------------------------------------------------------------- Pasaport

/// "Pasaport no: U12345678", "pasaport hj2212636" — 1-2 harf + 7-8 hane, bağlam kelimesiyle.
/// Harfler aynı büyüklükte olmalı; Türk pasaportu tek harfli, yabancılar çoğunlukla iki harfli.
pub(crate) fn scan_passport(text: &str, out: &mut Vec<Match>) {
    let b = text.as_bytes();
    let n = b.len();
    for i in 0..n.saturating_sub(7) {
        if !b[i].is_ascii_alphabetic() {
            continue;
        }
        let upper = b[i].is_ascii_uppercase();
        let mut d = i + 1;
        if d < n && b[d].is_ascii_alphabetic() && b[d].is_ascii_uppercase() == upper {
            d += 1;
        }
        // Ucuz ret önce: harf(ler)den sonra rakam yoksa sınır denetimine gerek yok.
        if d >= n || !b[d].is_ascii_digit() || !boundary_before(text, i) {
            continue;
        }
        let mut j = d;
        while j < n && b[j].is_ascii_digit() {
            j += 1;
        }
        if (7..=8).contains(&(j - d)) && boundary_after(text, j) && context_has(text, i, 48, PASSPORT_WORDS) {
            out.push(Match::new(Kind::Passport, i..j, if upper && d == i + 1 { 90 } else { 85 }));
        }
    }
}

// ---------------------------------------------------------------- SGK sicil no

/// "SGK sicil no: 1234567890" — 6-13 haneli bitişik sayı, yalnızca SGK/SSK/Bağ-Kur
/// bağlamıyla. Sağlaması olmadığından bağlam şarttır. Daha önce başka bir türle
/// (ör. TCKN) eşleşen bölgeler atlanır.
pub(crate) fn scan_social_security(text: &str, out: &mut Vec<Match>) {
    let b = text.as_bytes();
    let n = b.len();
    let taken = out.len();
    let mut displaced = Vec::new();
    let mut i = 0;
    while i < n {
        let Some(off) = b[i..].iter().position(u8::is_ascii_digit) else { break };
        i += off;
        let mut j = i;
        while j < n && b[j].is_ascii_digit() {
            j += 1;
        }
        // Ucuz kontroller önce; bağlam araması yalnızca aday sayılar için.
        // 444'lü yedi haneli numaralar çağrı merkezidir ("SGK hattı 4441234").
        let call_center = j - i == 7 && b[i..].starts_with(b"444");
        if (6..=13).contains(&(j - i)) && !call_center && boundary_before(text, i) && boundary_after(text, j) {
            let clash = out[..taken].iter().position(|m| m.span.start < j && i < m.span.end);
            match clash {
                None if context_has(text, i, 40, SGK_WORDS) || context_after_has(text, j, 24, SGK_AFTER_WORDS) => {
                    out.push(Match::new(Kind::SocialSecurity, i..j, 85));
                }
                // Bitişik yazılmış numara telefon ya da kart sanıldıysa ama hemen
                // solunda SGK etiketi varsa ("SGK sicil no: 5371451377") SGK kazanır.
                // Geçerli TCKN olduğu gibi kalır.
                Some(k)
                    if matches!(out[k].kind, Kind::Phone | Kind::Card)
                        && out[k].span == (i..j)
                        && sgk_label_right_before(text, i) =>
                {
                    displaced.push(k);
                    out.push(Match::new(Kind::SocialSecurity, i..j, 85));
                }
                _ => {}
            }
        }
        i = j;
    }
    displaced.sort_unstable();
    for k in displaced.into_iter().rev() {
        out.remove(k);
    }
}

/// Numaranın hemen solunda (aynı satırda, arada başka rakam ya da telefon
/// sözcüğü olmadan) SGK etiketi var mı?
fn sgk_label_right_before(text: &str, i: usize) -> bool {
    let b = text.as_bytes();
    let mut start = i;
    while start > 0 && i - start < 40 && !b[start - 1].is_ascii_digit() && b[start - 1] != b'\n' {
        start -= 1;
    }
    while !text.is_char_boundary(start) {
        start += 1;
    }
    let seg = &text[start..i];
    context_has(seg, seg.len(), seg.len(), SGK_WORDS) && !context_has(seg, seg.len(), seg.len(), SGK_PHONE_WORDS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(f: fn(&str, &mut Vec<Match>), s: &str) -> Vec<String> {
        let mut v = Vec::new();
        f(s, &mut v);
        v.iter().filter(|m| m.confidence >= 70).map(|m| m.text(s).to_string()).collect()
    }

    #[test]
    fn ipv4() {
        assert_eq!(run(scan_ipv4, "Giriş 192.168.1.10 ve 8.8.8.8 adresinden."), ["192.168.1.10", "8.8.8.8"]);
        assert!(run(scan_ipv4, "sürüm 1.2.3.4, tarih 12.03.2024, 256.1.1.1, 1.2.3.4.5, 01.2.3.4").is_empty());
    }

    #[test]
    fn ipv6() {
        assert_eq!(
            run(scan_ipv6, "IP: 2001:db8:85a3::8a2e:370:7334, saat 14:30:00, mac aa:bb:cc:dd:ee:ff"),
            ["2001:db8:85a3::8a2e:370:7334"]
        );
        assert_eq!(run(scan_ipv6, "fe80:0:0:0:202:b3ff:fe1e:8329 bağlı"), ["fe80:0:0:0:202:b3ff:fe1e:8329"]);
        assert!(run(scan_ipv6, "Not: a::b ve Saat:12:30").is_empty());
    }

    #[test]
    fn birth_date() {
        assert_eq!(run(scan_birth_date, "Doğum tarihi: 05.03.1990"), ["05.03.1990"]);
        assert_eq!(run(scan_birth_date, "DOĞUM TARİHİ 5 Mart 1990."), ["5 Mart 1990"]);
        assert_eq!(run(scan_birth_date, "d.t. 1987-08-21"), ["1987-08-21"]);
        assert!(run(scan_birth_date, "Fatura tarihi 05.03.1990 ve 11.11.2011").is_empty());
        assert!(run(scan_birth_date, "doğum 31.02.1990, doğum 32.01.1990").is_empty());
        assert_eq!(run(scan_birth_date, "doğum: 29.02.2000"), ["29.02.2000"]);
        assert_eq!(run(scan_birth_date, "Hasta 12.03.1990 doğumlu"), ["12.03.1990"]);
        assert!(run(scan_birth_date, "Doğum iznim 1 Mayıs 2999 tarihinde bitiyor").is_empty());
        // Etiket yalnızca kendi tarihini kapsar.
        assert_eq!(run(scan_birth_date, "Doğum tarihim 19.02.1983, sipariş tarihim 02.11.2025."), ["19.02.1983"]);
        assert_eq!(run(scan_birth_date, "1959 doğumluyum, 07.07.2020 tarihinde işe başladım"), ["1959"]);
        assert_eq!(run(scan_birth_date, "Eşimin doğum tarihi 11.05.1994, oğlumun 26.01.2014."), ["11.05.1994", "26.01.2014"]);
        // Tek başına yıl ve yılsız gün-ay yalnızca etikete bitişikse.
        assert_eq!(run(scan_birth_date, "doğum yılım 1998, hasta (1977 doğumlu)"), ["1998", "1977"]);
        assert_eq!(run(scan_birth_date, "Doğum günüm 7 Nisan. Doğum tarihiniz: 29 Şubat"), ["7 Nisan", "29 Şubat"]);
        assert!(run(scan_birth_date, "doğum yılınızı teyit edelim, sipariş 1954 adet").is_empty());
        assert!(run(scan_birth_date, "Doğum günü kampanyası 2024 yılında 7 Nisan'a kadar").is_empty());
        assert!(run(scan_birth_date, "Bebeğin tahmini doğum tarihi 20 Kasım, beklenen doğum 03.12.2025").is_empty());
    }

    #[test]
    fn passport() {
        assert_eq!(run(scan_passport, "Pasaport No: U12345678"), ["U12345678"]);
        assert!(run(scan_passport, "Sipariş kodu U12345678").is_empty());
        assert_eq!(run(scan_passport, "pasaport hj2212636, Pasaportumla (KF2129874)"), ["hj2212636", "KF2129874"]);
        assert!(run(scan_passport, "pasaport Ab1234567").is_empty());
    }

    #[test]
    fn social_security() {
        assert_eq!(run(scan_social_security, "SGK sicil no: 3401234567"), ["3401234567"]);
        assert_eq!(run(scan_social_security, "Bağ-Kur No 1234567, SSK 99887766"), ["1234567", "99887766"]);
        assert!(run(scan_social_security, "Sipariş 3401234567, sicil 12345").is_empty());
        assert_eq!(run(scan_social_security, "gelir belgemi ve 84883037 numaralı SGK sicil dökümü"), ["84883037"]);
        assert_eq!(run(scan_social_security, "kayıtlı 85387200 sicil numaralı sigortalı"), ["85387200"]);
        assert!(run(scan_social_security, "Sipariş 3401234567 kargoya verildi").is_empty());
        assert_eq!(run(scan_social_security, "Sosyal Güvenlik Kurumu sicil numaram 34012345"), ["34012345"]);
        assert_eq!(run(scan_social_security, "emekli sandığı tahsis no 1234567"), ["1234567"]);
        assert!(run(scan_social_security, "sigorta poliçe no 3401234567").is_empty());
        assert_eq!(run(scan_social_security, "İşyeri sicil numaramız 3412345"), ["3412345"]);
        assert!(run(scan_social_security, "Sosyal güvenlik hattı 4441234").is_empty());
    }
}
