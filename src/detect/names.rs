//! Türkçe'ye duyarlı ad-soyad tespiti. Model kullanmaz; üç sinyale bakar:
//!
//! 1. **Unvan/hitap** önce gelir: `Sayın`, `Dr.`, `Av.`, `Prof. Dr.` ...
//! 2. **Hitap** sonra gelir: `Ahmet Bey`, `Ayşe Hanım`
//! 3. **Form etiketi**: `Ad Soyad: ...`, `Hasta Adı: ...`
//! 4. **Ad sözlüğü**: yaygın bir ad + büyük harfli soyad (`Mehmet Ali Yılmaz`)
//!
//! Türkçe ekler korunur: `Yılmaz'ın` → yalnızca `Yılmaz` maskelenir, `'ın` kalır.

use std::collections::HashMap;
use std::hash::BuildHasherDefault;
use std::sync::OnceLock;

use crate::entity::{Kind, Match};

const MAX_CHAIN: usize = 3;

/// Önce gelen unvanlar (sıralı: ikili arama için).
const TITLES: &[&str] = &[
    "av", "avukat", "bay", "bayan", "doktor", "doç", "doçent", "dr", "hemşire", "müh", "op", "prof",
    "sayın", "sn", "uzm",
];
/// Sonra gelen hitaplar.
const SUFFIXES: &[&str] = &[
    "bey", "beyefendi", "hanım", "hanımefendi", "hoca", "hocam",
];
/// Güçlü form etiketleri (iki noktadan sonra 1+ büyük harfli kelime yeter).
const STRONG_LABELS: &[&str] = &[
    "ad", "adi", "adı", "isim", "ismi", "name", "soyad", "soyadi", "soyadı",
];
/// Zayıf etiketler (en az 2 büyük harfli kelime ister).
const WEAK_LABELS: &[&str] = &[
    "alıcı", "danışan", "gönderen", "hasta", "kiracı", "müşteri", "personel", "sahibi", "veli",
    "öğrenci",
];
/// Ad zincirine girmemesi gereken büyük harfli kelimeler.
const STOP: &[&str] = &[
    "adres", "cad", "cadde", "caddesi", "eposta", "iban", "mah", "mahalle", "mahallesi", "no", "numara",
    "sok", "sokak", "tc", "tckn", "tel", "telefon", "vkn",
];

/// Yaygın Türkçe adlar (sıralı). "Deniz, Umut, Barış, Can, Gül" gibi aynı zamanda
/// sıradan kelime olanlar bilerek dışarıda; onlar yalnızca unvan/hitapla yakalanır.
const FIRST_NAMES: &[&str] = &[
    "abdullah", "abdurrahman", "adem", "ahmet", "ali", "alper", "arda", "aslı", "asya", "ayla", "aylin",
    "ayten", "ayşe", "azra", "batuhan", "bekir", "berk", "berna", "beyza", "burak", "burcu", "buse",
    "bülent", "büşra", "canan", "cansu", "cem", "cemal", "cengiz", "ceren", "cihan", "defne", "derya",
    "dilara", "doruk", "ebru", "ecrin", "efe", "elif", "emine", "emir", "emre", "enes", "engin",
    "ercan", "erdem", "eren", "erkan", "esra", "fadime", "fatih", "fatma", "filiz", "furkan", "gamze",
    "gökhan", "gözde", "gülay", "gülsüm", "hakan", "halil", "hamza", "hanife", "harun", "hasan",
    "hatice", "havva", "hülya", "hüseyin", "ibrahim", "ilhan", "ilker", "ilknur", "irem", "ismail",
    "kaan", "kadir", "kemal", "kenan", "kerem", "kübra", "levent", "leyla", "mahmut", "mehmet", "mert",
    "merve", "meryem", "metin", "murat", "musa", "mustafa", "mücahit", "nazlı", "nermin", "nesrin",
    "nilay", "nisa", "nuray", "nurcan", "okan", "oktay", "onur", "orhan", "osman", "oğuz", "pınar",
    "rabia", "ramazan", "recep", "saadet", "salih", "seda", "sedat", "selim", "selin", "sena", "serdar",
    "serkan", "serpil", "sevda", "sevim", "sibel", "sinan", "songül", "sultan", "süleyman", "taner",
    "tarık", "tayfun", "tolga", "tuba", "tuncay", "tuncer", "tuğba", "tülin", "uğur", "volkan",
    "yasemin", "yaşar", "yeliz", "yeşim", "yiğit", "yunus", "yusuf", "zafer", "zehra", "zeliha",
    "zerrin", "zeynep", "ömer", "özlem", "şerife", "şeyma", "şule",
];

/// Listelerdeki en uzun kelimeden uzun kelimelerin küçük harfi tutulmaz.
const MAX_LOWER: usize = 16;

#[derive(Clone, Copy)]
struct Word {
    /// Kaynak metindeki bayt aralığı.
    s: usize,
    e: usize,
    /// Küçük harfli halinin arena içindeki aralığı.
    ls: usize,
    le: usize,
    cap: bool,
}

// Sözlük bayrakları: bir kelime birden çok listede olabilir.
const F_TITLE: u8 = 1;
const F_SUFFIX: u8 = 2;
const F_STRONG: u8 = 4;
const F_WEAK: u8 = 8;
const F_STOP: u8 = 16;
const F_FIRST: u8 = 32;

/// Kısa anahtarlar için hızlı FNV-1a (SipHash'e göre birkaç kat hızlı).
#[derive(Default)]
struct Fnv(u64);

impl core::hash::Hasher for Fnv {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        let mut h = if self.0 == 0 { 0xcbf2_9ce4_8422_2325 } else { self.0 };
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        self.0 = h;
    }
}

type Lexicon = HashMap<&'static str, u8, BuildHasherDefault<Fnv>>;

/// Tüm listeleri tek bir tabloda birleştirir: kelime başına tek arama.
fn lexicon() -> &'static Lexicon {
    static LEX: OnceLock<Lexicon> = OnceLock::new();
    LEX.get_or_init(|| {
        let mut m = Lexicon::default();
        for (set, flag) in [
            (TITLES, F_TITLE),
            (SUFFIXES, F_SUFFIX),
            (STRONG_LABELS, F_STRONG),
            (WEAK_LABELS, F_WEAK),
            (STOP, F_STOP),
            (FIRST_NAMES, F_FIRST),
        ] {
            for &w in set {
                *m.entry(w).or_insert(0) |= flag;
            }
        }
        m
    })
}

/// Kelimelere böler ve kısa kelimelerin Türkçe küçük harfli halini tek bir
/// arenaya yazar (kelime başına bellek tahsisi yok; ASCII için hızlı yol).
fn tokenize(text: &str, words: &mut Vec<Word>, arena: &mut String) {
    let b = text.as_bytes();
    let n = b.len();
    let mut i = 0;
    while i < n {
        // Kelime başlangıcını bul.
        let c0 = b[i];
        if c0 < 0x80 {
            if !c0.is_ascii_alphabetic() {
                i += 1;
                continue;
            }
        } else {
            let ch = text[i..].chars().next().unwrap_or(' ');
            if !ch.is_alphabetic() {
                i += ch.len_utf8();
                continue;
            }
        }
        let s = i;
        let ls = arena.len();
        let mut cap = false;
        while i < n {
            let c = b[i];
            if c < 0x80 {
                if !c.is_ascii_alphabetic() {
                    break;
                }
                if i == s {
                    cap = c.is_ascii_uppercase();
                }
                arena.push(if c == b'I' { 'ı' } else { c.to_ascii_lowercase() as char });
                i += 1;
            } else {
                let ch = text[i..].chars().next().unwrap_or(' ');
                if !ch.is_alphabetic() {
                    break;
                }
                if i == s {
                    cap = ch.is_uppercase();
                }
                match ch {
                    'İ' => arena.push('i'),
                    _ => arena.extend(ch.to_lowercase()),
                }
                i += ch.len_utf8();
            }
        }
        if i - s > MAX_LOWER {
            arena.truncate(ls);
        }
        words.push(Word { s, e: i, ls, le: arena.len(), cap });
    }
}

pub(crate) fn scan(text: &str, claimed: &[core::ops::Range<usize>], out: &mut Vec<Match>) {
    let mut words = Vec::with_capacity(text.len() / 6);
    let mut arena = String::with_capacity(text.len());
    tokenize(text, &mut words, &mut arena);
    let n = words.len();
    if n < 2 {
        return;
    }
    let lex = lexicon();
    let flags: Vec<u8> = words
        .iter()
        .map(|w| if w.le > w.ls { lex.get(&arena[w.ls..w.le]).copied().unwrap_or(0) } else { 0 })
        .collect();
    let has = |k: usize, f: u8| flags[k] & f != 0;
    let gap = |a: usize, b: usize| &text[words[a].e..words[b].s];
    let space_gap = |a: usize, b: usize| {
        let g = gap(a, b);
        !g.is_empty() && g.len() <= 2 && g.bytes().all(|c| c == b' ')
    };
    let label_after = |k: usize| k + 1 < n && gap(k, k + 1).trim_start().starts_with(':');
    let in_claimed = |k: usize| {
        let (s, e) = (words[k].s, words[k].e);
        claimed.iter().any(|r| s < r.end && e > r.start)
    };
    let nameable = |k: usize| {
        words[k].cap
            && !has(k, F_STOP | F_TITLE | F_SUFFIX)
            && !label_after(k)
            && !in_claimed(k)
    };

    // `k`'dan başlayan büyük harfli kelime zinciri: [k, son] (dahil).
    let chain = |k: usize| -> Option<(usize, usize)> {
        if !nameable(k) {
            return None;
        }
        let mut end = k;
        while end + 1 < n && end + 1 - k < MAX_CHAIN && space_gap(end, end + 1) && nameable(end + 1) {
            end += 1;
        }
        Some((k, end))
    };
    let push = |out: &mut Vec<Match>, a: usize, b: usize, conf: u8| {
        out.push(Match::new(Kind::Name, words[a].s..words[b].e, conf));
    };

    let mut k = 0;
    while k < n {
        // 1) Unvan → ad. "Sayın Prof. Dr. Ayşe Kaya"
        if has(k, F_TITLE) {
            let mut j = k;
            while j + 1 < n && has(j + 1, F_TITLE) && title_gap(gap(j, j + 1)) {
                j += 1;
            }
            if j + 1 < n && title_gap(gap(j, j + 1)) {
                if let Some((a, b)) = chain(j + 1) {
                    push(out, a, b, 85);
                    k = b + 1;
                    continue;
                }
            }
        }

        // 2) Form etiketi → ad. "Adı Soyadı: AHMET YILMAZ"
        let strong = has(k, F_STRONG);
        if (strong || has(k, F_WEAK)) && k + 1 < n {
            let g = gap(k, k + 1);
            let colon = g.trim().starts_with(':') && g.trim().len() == 1;
            if colon {
                if let Some((a, b)) = chain(k + 1) {
                    if strong || b > a {
                        push(out, a, b, if strong { 88 } else { 72 });
                        k = b + 1;
                        continue;
                    }
                }
            }
        }

        // 3) Ad sözlüğü + soyad. "Mehmet Ali Yılmaz'a"
        if words[k].cap && has(k, F_FIRST) {
            if let Some((a, b)) = chain(k) {
                if b > a {
                    push(out, a, b, 75);
                    k = b + 1;
                    continue;
                }
            }
        }

        // 4) Hitap ← ad. "Ahmet Bey", "Ayşe Kaya Hanım"
        if has(k, F_SUFFIX) && k > 0 && space_gap(k - 1, k) && words[k - 1].cap {
            let mut a = k - 1;
            while a > 0 && k - a < 2 && space_gap(a - 1, a) && nameable(a - 1) {
                a -= 1;
            }
            let already = out.last().is_some_and(|m| m.kind == Kind::Name && m.span.end == words[k - 1].e);
            if nameable(k - 1) && !already {
                push(out, a, k - 1, 80);
            }
        }
        k += 1;
    }
}

/// Unvandan sonra izin verilen ayraç: " ", ". ", "." (ör. "Dr.Ahmet").
fn title_gap(g: &str) -> bool {
    let rest = g.strip_prefix('.').unwrap_or(g);
    g.len() <= 3 && rest.bytes().all(|c| c == b' ') && !g.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(s: &str) -> Vec<&str> {
        let mut v = Vec::new();
        scan(s, &[], &mut v);
        v.iter().map(|m| m.text(s)).collect()
    }

    #[test]
    fn lists_are_lowercase_and_short() {
        for set in [TITLES, SUFFIXES, STRONG_LABELS, WEAK_LABELS, STOP, FIRST_NAMES] {
            for w in set {
                assert_eq!(crate::text::tr_lower(w), *w);
                assert!(w.len() <= MAX_LOWER, "{w}");
            }
        }
    }

    #[test]
    fn title_and_suffix() {
        assert_eq!(names("Sayın Prof. Dr. Ayşe Kaya toplantıya katıldı."), ["Ayşe Kaya"]);
        assert_eq!(names("Bugün Kemal Sunal Bey aradı."), ["Kemal Sunal"]);
        assert_eq!(names("dün Tunç Hanım geldi"), ["Tunç"]);
    }

    #[test]
    fn labels_and_dictionary() {
        assert_eq!(names("Adı Soyadı: AHMET YILMAZ TC: 1"), ["AHMET YILMAZ"]);
        assert_eq!(names("Paketi Mehmet Ali Yılmaz'a teslim et."), ["Mehmet Ali Yılmaz"]);
        assert_eq!(names("İSTANBUL'da Ahmet ile buluştuk."), Vec::<&str>::new());
    }

    #[test]
    fn no_false_positive_on_plain_text() {
        assert!(names("Deniz Kenarı çok güzeldi. Umut Etmek lazım.").is_empty());
    }
}
