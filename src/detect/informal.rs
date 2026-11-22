//! Gayriresmî (tarifli) adresler: "Bostanlı'da, iskelenin arkasındaki sokakta,
//! 3. kat", "okulun arka tarafındaki müstakil evde, Kemeraltı tarafında".
//!
//! Sabit ifade listesi kullanılmaz; yapıya bakılır:
//!
//! 1. **Çekirdek**: `-ki` almış bir konum sözcüğü (`arkasındaki`, `karşısındaki`,
//!    `bitişiğindeki`, `köşedeki` ...) ve en fazla 3 kelime sonra bir konut sözcüğü
//!    (`ev`, `daire`, `bina`, `apartman`, `sokak`, `kat` ...).
//! 2. **Sol**: çekirdeğin önündeki işaret noktası ("eski postanenin") ve varsa
//!    semt bulunma hâli ("Bostanlı'da,").
//! 3. **Sağ**: "X tarafında", "5. kat", "2. katta sağdaki daire", "kapı numarası 24".
//!
//! Semt, sağ parça ya da yakında konut bağlamı ("adresim", "oturuyorum", "kurye")
//! yoksa güven varsayılan eşiğin altında kalır: "parkın karşısındaki binada toplantı".

use crate::entity::{Kind, Match};
use crate::text::{context_has, fold};

/// Konum sözcüğü kökleri; ardından iyelik + bulunma + `-ki` gelir.
const SPATIAL_STEMS: &[&str] = &[
    "arka", "alt", "bitişi", "çapraz", "dib", "giriş", "ileri", "karşı", "köşe", "ön", "taraf", "üst", "yakın",
    "yan",
];
/// Konut/yer sözcüğü kökleri.
const DWELLING_STEMS: &[&str] = &["apartman", "apt", "bina", "blok", "daire", "kat", "site", "sokak", "sokağ"];
/// Çekirdeğin solunda işaret noktasına dahil edilmeyen kelimeler.
const LEFT_STOP: &[&str] = &[
    "adres", "adresi", "adresim", "adresimiz", "adresine", "ama", "ben", "bizim", "burası", "da", "de", "en",
    "ev", "evim", "evimiz", "ise", "kurye", "müşteri", "oturuyorum", "oturuyoruz", "şu", "bu", "ve", "yeni",
    "yer", "yeri", "yerimiz",
];
const CONTEXT: &[&str] = &["adres", "otur", "yaşıyor", "taşın", "kurye", "teslim", "kargo", "evim", "evimiz", "ambulans"];
const MAX_LEFT: usize = 3;

#[derive(Clone, Copy)]
struct W {
    s: usize,
    e: usize,
}

fn words(text: &str) -> Vec<W> {
    let b = text.as_bytes();
    let n = b.len();
    // `i`'deki karakter harf/rakamsa bayt uzunluğu (ASCII için hızlı yol).
    let alnum = |i: usize| -> Option<usize> {
        if b[i] < 0x80 {
            return b[i].is_ascii_alphanumeric().then_some(1);
        }
        if crate::text::tr_letter(b, i).is_some() {
            return Some(2);
        }
        let c = text[i..].chars().next()?;
        c.is_alphanumeric().then(|| c.len_utf8())
    };
    let mut v = Vec::with_capacity(n / 6);
    let mut i = 0;
    while i < n {
        if !text.is_char_boundary(i) {
            i += 1;
            continue;
        }
        let Some(l) = alnum(i) else {
            i += 1;
            continue;
        };
        let s = i;
        i += l;
        while i < n {
            match alnum(i) {
                Some(l) => i += l,
                None => break,
            }
        }
        v.push(W { s, e: i });
    }
    v
}

fn is_spatial(w: &str) -> bool {
    ["daki", "deki", "taki", "teki"].iter().any(|k| w.ends_with(k))
        && SPATIAL_STEMS.iter().any(|s| w.starts_with(s))
        && w.chars().count() <= 16
}

/// Kelime (harf duyarsız) "daki/deki/taki/teki" ile bitiyor mu? Bellek tahsissiz ön eleme.
fn ki_suffix(w: &str) -> bool {
    // Son harf (katlanınca) "i" olmalı: i, I, ı (C4 B1), İ (C4 B0).
    let b = w.as_bytes();
    if !matches!(b, [.., b'i' | b'I'] | [.., 0xC4, 0xB0 | 0xB1]) {
        return false;
    }
    let mut it = w.chars().rev().map(fold_char);
    matches!(
        (it.next(), it.next(), it.next(), it.next()),
        (Some('i'), Some('k'), Some('a' | 'e'), Some('d' | 't'))
    )
}

fn verb_like(w: &str) -> bool {
    const ENDS: &[&str] = &[
        "dim", "dum", "düm", "tim", "tum", "tüm", "dik", "duk", "dük", "tik", "tuk", "tük", "yor", "yorum",
        "miş", "muş", "müş", "acak", "ecek",
    ];
    w.chars().count() > 3 && ENDS.iter().any(|e| w.ends_with(e))
}

fn is_dwelling(w: &str) -> bool {
    matches!(w, "ev" | "evde" | "evi" | "evin" | "evimiz" | "evimizde") || DWELLING_STEMS.iter().any(|s| w.starts_with(s))
}

/// `s`'in başındaki ayracı (", " / " ") atlar; atlanan bayt sayısı.
fn skip_sep(s: &str) -> usize {
    let t = s.strip_prefix(',').unwrap_or(s);
    let sp = t.len() - t.trim_start_matches(' ').len();
    if sp == 0 || sp > 2 {
        return 0;
    }
    s.len() - t.len() + sp
}

fn fold_char(c: char) -> char {
    match c {
        'I' | 'İ' | 'ı' | 'i' => 'i',
        c => c.to_lowercase().next().unwrap_or(c),
    }
}

/// `s`, `p` ile (Türkçe büyük/küçük harf ve ı/i duyarsız) başlıyorsa `s`'te
/// eşleşen kısmın bayt uzunluğu. Bayt konumları her zaman `s`'ten alınır.
fn prefix_ci(s: &str, p: &str) -> Option<usize> {
    let mut it = s.char_indices();
    for pc in p.chars() {
        let (_, c) = it.next()?;
        if fold_char(c) != fold_char(pc) {
            return None;
        }
    }
    Some(it.next().map_or(s.len(), |(i, _)| i))
}

fn letters(s: &str) -> usize {
    s.char_indices().find(|(_, c)| !c.is_alphabetic()).map_or(s.len(), |(i, _)| i)
}

fn digits(s: &str) -> usize {
    s.bytes().take_while(u8::is_ascii_digit).count()
}

/// Sağdaki ek parça: "Göztepe tarafında", "5. kat(ta)( sağdaki daire)", "en üst kat",
/// "zemin kat", "kapı numarası 24". Parçanın bayt uzunluğu.
fn tail_part(s: &str) -> Option<usize> {
    for p in ["en üst kat", "zemin kat", "bodrum kat", "çatı kat"] {
        if let Some(n) = prefix_ci(s, p) {
            return Some(n + letters(&s[n..]));
        }
    }
    for p in ["kapı numarası ", "kapı no: ", "kapı no "] {
        if let Some(n) = prefix_ci(s, p) {
            let d = digits(&s[n..]);
            return (d > 0).then_some(n + d);
        }
    }
    let d = digits(s);
    if (1..=2).contains(&d) {
        let mut e = d + prefix_ci(&s[d..], ". kat")?;
        e += letters(&s[e..]);
        for side in [" sağdaki daire", " soldaki daire", " ortadaki daire", " sağdaki", " soldaki"] {
            if let Some(n) = prefix_ci(&s[e..], side) {
                e += n;
                break;
            }
        }
        return Some(e);
    }
    let l = letters(s);
    if l > 0 {
        return prefix_ci(&s[l..], " tarafında").map(|n| l + n);
    }
    None
}

/// Başlangıçtan hemen önce "<semt>'da, " var mı? Semtin başlangıcı.
fn locative_before(text: &str, start: usize) -> Option<usize> {
    let before = text[..start].trim_end_matches([' ', ',']);
    let suf = ["'da", "'de", "'ta", "'te", "’da", "’de", "’ta", "’te"]
        .iter()
        .find(|s| before.len() >= s.len() && before.is_char_boundary(before.len() - s.len()) && eq_tail(before, s))?;
    let root_end = before.len() - suf.len();
    let root = &before[..root_end];
    let l = root.chars().rev().take_while(|c| c.is_alphabetic()).map(char::len_utf8).sum::<usize>();
    (l >= 3).then_some(root_end - l)
}

/// `s` (büyük/küçük harf duyarsız) `suf` ile bitiyor mu? `suf` ASCII harf + kesme.
fn eq_tail(s: &str, suf: &str) -> bool {
    s[s.len() - suf.len()..].eq_ignore_ascii_case(suf)
}

pub(crate) fn scan_informal(text: &str, out: &mut Vec<Match>) {
    // Hızlı ret: `-ki` ile biten bir konum sözcüğü yoksa tarama yok.
    if !["daki", "deki", "taki", "teki"].iter().any(|k| text.contains(k)) {
        return;
    }
    let ws = words(text);
    // Küçük harfli biçim yalnızca aday kelimelerin çevresinde gerekir.
    let lw = |k: usize| if ws[k].e - ws[k].s <= 40 { fold(&text[ws[k].s..ws[k].e]) } else { String::new() };
    let spaces = |a: usize, b: usize| {
        let g = &text[ws[a].e..ws[b].s];
        !g.is_empty() && g.len() <= 2 && g.bytes().all(|c| c == b' ')
    };
    let mut i = 0;
    while i < ws.len() {
        if !ki_suffix(&text[ws[i].s..ws[i].e]) || !is_spatial(&lw(i)) {
            i += 1;
            continue;
        }
        // Konut sözcüğü: aynı öbekte, en fazla 3 kelime sonra.
        let Some(j) = (i + 1..ws.len().min(i + 4)).take_while(|&j| spaces(j - 1, j)).find(|&j| is_dwelling(&lw(j))) else {
            i += 1;
            continue;
        };
        // Sol: işaret noktası (noktalama ya da durak kelimesine kadar).
        // Büyük harfli bir kelimede durulur ("Ambulans Parkın ..." → "Parkın"), çekimli
        // fiil ("bıraktım") alınmaz.
        let mut a = i;
        while a > 0
            && i - a < MAX_LEFT
            && spaces(a - 1, a)
            && !LEFT_STOP.contains(&lw(a - 1).as_str())
            && !verb_like(&lw(a - 1))
            && !text[ws[a].s..].starts_with(char::is_uppercase)
        {
            a -= 1;
        }
        let mut start = ws[a].s;
        let semt = locative_before(text, start);
        if let Some(s) = semt {
            start = s;
        }
        // Sağ: ek parçalar.
        let mut end = ws[j].e;
        let mut tails = 0;
        while tails < 3 {
            let sep = skip_sep(&text[end..]);
            match (sep > 0).then(|| tail_part(&text[end + sep..])).flatten() {
                Some(len) => {
                    end += sep + len;
                    tails += 1;
                }
                None => break,
            }
        }
        let strong = semt.is_some() || tails > 0 || context_has(text, start, 80, CONTEXT);
        out.push(Match::new(Kind::Address, start..end, if strong { 80 } else { 60 }));
        while i < ws.len() && ws[i].s < end {
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<&str> {
        let mut v = Vec::new();
        scan_informal(s, &mut v);
        v.iter().filter(|m| m.confidence >= 70).map(|m| m.text(s)).collect()
    }

    #[test]
    fn with_district_and_details() {
        assert_eq!(
            run("Ambulans Bostanlı'da, sahildeki çay bahçesinin arkasındaki sarı bina, 5. kat gelsin."),
            ["Bostanlı'da, sahildeki çay bahçesinin arkasındaki sarı bina, 5. kat"]
        );
        assert_eq!(
            run("Yeni yer: Okulun arka tarafındaki müstakil evde, Kemeraltı tarafında, 7. kat"),
            ["Okulun arka tarafındaki müstakil evde, Kemeraltı tarafında, 7. kat"]
        );
        assert_eq!(
            run("müşteri adresim fırının üstündeki dairede, erenköy tarafında, 5. katta sağdaki daire oradayım"),
            ["fırının üstündeki dairede, erenköy tarafında, 5. katta sağdaki daire"]
        );
        assert_eq!(
            run("anahtarı teyzeye bıraktım Caminin yanındaki dar sokakta, Göztepe tarafında"),
            ["Caminin yanındaki dar sokakta, Göztepe tarafında"]
        );
        assert_eq!(
            run("Ambulans Parkın köşesindeki sarı binada, zemin kat"),
            ["Parkın köşesindeki sarı binada, zemin kat"]
        );
        assert_eq!(
            run("Galata'da, eczanenin bitişiğindeki binada, kapı numarası 24."),
            ["Galata'da, eczanenin bitişiğindeki binada, kapı numarası 24"]
        );
    }

    #[test]
    fn needs_context() {
        // Konut/semt/ek parça/bağlam yoksa varsayılan eşikte maskelenmez.
        assert!(run("Toplantı parkın karşısındaki binada yapılacak.").is_empty());
        assert!(run("Kutunun içindeki ürün kırık, yanındaki not da yok.").is_empty());
        assert!(run("Fiyatı üstteki tabloda.").is_empty());
    }
}
