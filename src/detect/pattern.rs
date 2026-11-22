//! Harf içeren kalıplar: IBAN, e-posta, plaka. Bunlar sayısal taramadan önce
//! çalışır ve bölgelerini "sahiplenir".

use crate::checksum::is_valid_tr_iban;
use crate::entity::{Kind, Match};
use crate::text::{boundary_after, boundary_before, context_has};

/// Küçük harfli ve boşluklu plaka ("46 fp 540") yalnızca bu bağlamla kabul edilir.
const PLATE_WORDS: &[&str] = &["plaka", "arac", "araç", "araba", "otomobil", "taşıt", "tasit"];
/// Küçük harfli yazımda plaka harfi sanılabilecek yaygın kısa kelimeler.
const PLATE_LOWER_STOP: &[&[u8]] = &[b"ve", b"ile", b"ya", b"da", b"de", b"ki", b"mi", b"mu", b"bu", b"en", b"az", b"ay", b"her", b"son", b"ilk", b"no"];

/// `TR33 0006 1005 1978 6457 8413 26` veya bitişik yazılmış TR IBAN'ları.
pub(crate) fn scan_iban(text: &str, out: &mut Vec<Match>) {
    let b = text.as_bytes();
    let n = b.len();
    let mut digits = [0u8; 24];
    let mut i = 0;
    while i + 26 <= n {
        let Some(off) = b[i..n - 1].iter().position(|&c| c == b'T' || c == b't') else { break };
        i += off;
        if !(b[i + 1] == b'R' || b[i + 1] == b'r') || !boundary_before(text, i) {
            i += 1;
            continue;
        }
        let mut j = i + 2;
        let mut count = 0;
        let mut last_was_sep = false;
        while j < n && count < 24 {
            match b[j] {
                c if c.is_ascii_digit() => {
                    digits[count] = c;
                    count += 1;
                    last_was_sep = false;
                }
                b' ' | b'-' | b'\n' if !last_was_sep && count > 0 => last_was_sep = true,
                _ => break,
            }
            j += 1;
        }
        if count == 24 && boundary_after(text, j) && is_valid_tr_iban(&digits) {
            out.push(Match::new(Kind::Iban, i..j, 99));
            i = j;
        } else {
            i += 2;
        }
    }
}

#[inline]
fn is_local(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'%' | b'+' | b'-')
}

#[inline]
fn is_domain(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-')
}

/// Kişiye değil göreve ait yaygın e-posta yerel adları (büyük/küçük harf duyarsız).
const ROLE_ACCOUNTS: &[&str] = &[
    "admin", "basvuru", "bilgi", "contact", "destek", "fatura", "hello", "help", "hr", "ik", "info",
    "iletisim", "insankaynaklari", "kvkk", "muhasebe", "musteri", "musterihizmetleri", "noreply",
    "no-reply", "postmaster", "sales", "satis", "siparis", "support", "webmaster",
];

/// Yerel ad bir rol hesabı mı? "musteri.hizmetleri", "satis-ekibi" gibi ayraçlı biçimlerde
/// ilk parça bakılır.
fn is_role_account(local: &str) -> bool {
    let l = local.to_ascii_lowercase();
    let head = l.split(['.', '-', '_']).next().unwrap_or("");
    ROLE_ACCOUNTS.contains(&l.as_str()) || ROLE_ACCOUNTS.contains(&head)
}

pub(crate) fn scan_email(text: &str, out: &mut Vec<Match>) {
    let b = text.as_bytes();
    let n = b.len();
    let mut from = 0;
    while let Some(off) = b[from..].iter().position(|&c| c == b'@') {
        let at = from + off;
        from = at + 1;
        let mut s = at;
        while s > 0 && is_local(b[s - 1]) {
            s -= 1;
        }
        while s < at && b[s] == b'.' {
            s += 1;
        }
        let mut e = at + 1;
        while e < n && is_domain(b[e]) {
            e += 1;
        }
        while e > at + 1 && matches!(b[e - 1], b'.' | b'-') {
            e -= 1;
        }
        if s == at || b[at - 1] == b'.' {
            continue;
        }
        let domain = &b[at + 1..e];
        let Some(dot) = domain.iter().rposition(|&c| c == b'.') else { continue };
        let tld = &domain[dot + 1..];
        if dot == 0 || tld.len() < 2 || !tld.iter().all(u8::is_ascii_alphabetic) {
            continue;
        }
        // Rol hesabı ("info@", "destek@") bir kişiye ait değildir: varsayılan eşiğin altında.
        let conf = if is_role_account(&text[s..at]) { 60 } else { 99 };
        out.push(Match::new(Kind::Email, s..e, conf));
        from = e;
    }
}

/// Türk plakası: `34 ABC 123`, `06A1234`, `35 BK 4521`.
/// Q, W, X harfleri kullanılmaz; harf sayısına göre rakam sayısı değişir.
pub(crate) fn scan_plate(text: &str, out: &mut Vec<Match>) {
    let b = text.as_bytes();
    let n = b.len();
    let mut i = 0;
    while i + 5 <= n {
        let Some(off) = b[i..].iter().position(u8::is_ascii_digit) else { break };
        i += off;
        if i + 5 > n {
            break;
        }
        if !(b[i + 1].is_ascii_digit() && boundary_before(text, i)) {
            i += 1;
            while i < n && b[i].is_ascii_digit() {
                i += 1;
            }
            continue;
        }
        let code = (b[i] - b'0') as u32 * 10 + (b[i + 1] - b'0') as u32;
        if let Some(t) = plate_tail(b, i + 2).filter(|_| (1..=81).contains(&code)) {
            // Küçük harf: bitişik yazım ("63rua42") ya da plaka bağlamı şart.
            let lower_ok = !t.lower || (!t.spaced && t.conf > 60) || context_has(text, i, 40, PLATE_WORDS);
            if boundary_after(text, t.end) && lower_ok {
                let conf = if t.lower && t.spaced { t.conf.min(80) } else { t.conf };
                out.push(Match::new(Kind::Plate, i..t.end, conf));
                let end = t.end;
                i = end;
                continue;
            }
        }
        i += 2;
    }
}

struct PlateTail {
    end: usize,
    conf: u8,
    /// Harfler küçük mü ("34 abc 123").
    lower: bool,
    /// Ayraç (boşluk ya da tire) içeriyor mu.
    spaced: bool,
}

/// Plakanın harf+rakam kuyruğu. Ayraç boşluk ya da tire olabilir ("26-KL-469");
/// harfler ya hepsi büyük ya hepsi küçük olmalı.
fn plate_tail(b: &[u8], mut j: usize) -> Option<PlateTail> {
    let n = b.len();
    let mut sep: Option<u8> = None;
    if j < n && matches!(b[j], b' ' | b'-') {
        sep = Some(b[j]);
        j += 1;
    }
    let ls = j;
    let lower = j < n && b[j].is_ascii_lowercase();
    let same_case = |c: u8| if lower { c.is_ascii_lowercase() } else { c.is_ascii_uppercase() };
    while j < n && same_case(b[j]) && j - ls < 4 {
        if matches!(b[j].to_ascii_uppercase(), b'Q' | b'W' | b'X') {
            return None;
        }
        j += 1;
    }
    let letters = j - ls;
    if lower && PLATE_LOWER_STOP.contains(&&b[ls..j]) {
        return None;
    }
    // "12 TL 500", "5 KG 250" gibi birim kısaltmaları da plaka biçimine uyar.
    // Gerçek plakalarda da geçebildikleri için reddetmek yerine güveni düşürürüz
    // (varsayılan eşik 70'in altı; `min_confidence(60)` ile dahil edilebilir).
    const UNITS: &[&[u8]] = &[
        b"TL", b"KG", b"KM", b"CM", b"MM", b"MB", b"GB", b"KB", b"TB", b"MG", b"ML", b"HP", b"KW", b"DK",
        b"SN", b"LT", b"ADT", b"KWH",
    ];
    let upper: Vec<u8> = b[ls..j].to_ascii_uppercase();
    let conf = if UNITS.contains(&upper.as_slice()) { 60 } else { 90 };
    // İkinci ayraç ilkiyle aynı olmalı ("34 ABC-123" kabul edilmez).
    if j < n && matches!(b[j], b' ' | b'-') && sep.is_none_or(|s| s == b[j]) {
        sep = Some(b[j]);
        j += 1;
    }
    let ds = j;
    while j < n && b[j].is_ascii_digit() && j - ds < 6 {
        j += 1;
    }
    let nd = j - ds;
    let ok = match letters {
        1 => (4..=5).contains(&nd),
        2 => (3..=4).contains(&nd),
        3 => (2..=3).contains(&nd),
        _ => false,
    };
    ok.then_some(PlateTail { end: j, conf, lower, spaced: sep.is_some() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(f: fn(&str, &mut Vec<Match>), s: &str) -> Vec<String> {
        let mut v = Vec::new();
        f(s, &mut v);
        v.iter().map(|m| m.text(s).to_string()).collect()
    }

    #[test]
    fn iban() {
        assert_eq!(
            kinds(scan_iban, "IBAN: TR33 0006 1005 1978 6457 8413 26 adresine"),
            ["TR33 0006 1005 1978 6457 8413 26"]
        );
        assert_eq!(kinds(scan_iban, "tr330006100519786457841326."), ["tr330006100519786457841326"]);
        assert!(kinds(scan_iban, "TR330006100519786457841327").is_empty());
    }

    #[test]
    fn email() {
        assert_eq!(
            kinds(scan_email, "Bana ali.veli+test@ornek.com.tr. adresinden yazın"),
            ["ali.veli+test@ornek.com.tr"]
        );
        assert!(kinds(scan_email, "@kullanici ve a@b").is_empty());
    }

    #[test]
    fn plate() {
        assert_eq!(
            kinds(scan_plate, "Araç 34 ABC 123 ve 06A1234, ayrıca 35 BK 4521."),
            ["34 ABC 123", "06A1234", "35 BK 4521"]
        );
        assert!(kinds(scan_plate, "99 ABC 123 ve 34 km 120 ve 34 QW 123").is_empty());
        assert_eq!(kinds(scan_plate, "51 EUR 557"), ["51 EUR 557"]);
    }

    #[test]
    fn plate_variants() {
        assert_eq!(kinds(scan_plate, "Plakalar 26-KL-469 ve 63rua42."), ["26-KL-469", "63rua42"]);
        assert_eq!(kinds(scan_plate, "aracın plakası 46 fp 540 idi"), ["46 fp 540"]);
        // bağlamsız küçük harf, karışık ayraç ve yaygın kelimeler plaka değildir
        assert!(kinds(scan_plate, "toplam 46 fp 540 adet, 34 ABC-123").is_empty());
        assert!(kinds(scan_plate, "araç sayısı 10 ve 200 arası").is_empty());
    }

    #[test]
    fn iban_line_break() {
        assert_eq!(
            kinds(scan_iban, "IBAN: TR33 0006 1005\n1978 6457 8413 26 adresine"),
            ["TR33 0006 1005\n1978 6457 8413 26"]
        );
    }
}
