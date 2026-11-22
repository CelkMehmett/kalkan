//! Yazıyla söylenmiş numaralar: "sıfır beş yüz otuz beş iki yüz seksen üç on yedi
//! yetmiş altı" → 05352831776. Çağrı merkezi transkriptlerinde ve sesli asistan
//! çıktılarında telefon ve TC kimlik numaraları çoğunlukla böyle geçer.
//!
//! Sayı kelimeleri gruplara ayrılır (`beş yüz otuz beş` → 535, `on yedi` → 17,
//! `sıfır` → 0) ve gruplar birleştirilir. Bir birler kelimesinin ardından `yüz`
//! geliyorsa o kelime yeni grubu başlatır: "beş yüz yirmi | iki yüz doksan beş".
//! En az 10 haneli diziler TCKN (sağlamayla) ya da telefon olarak sınıflanır.

use crate::checksum::{is_valid_tckn, luhn_valid};
use crate::entity::{Kind, KindSet, Match};
use crate::text::context_has;

const PHONE_WORDS: &[&str] = &["tel", "cep", "gsm", "numara", "ara", "ulaş", "ulas"];
const TCKN_WORDS: &[&str] = &["tc", "t.c", "kimlik", "tckn"];
const MIN_DIGITS: usize = 10;
const MAX_DIGITS: usize = 14;

#[derive(Clone, Copy, PartialEq)]
enum Tok {
    Zero,
    Ones(u8),
    Tens(u8),
    Hundred,
    /// "bin": yalnızca dörtlü gruplarla okunan kart numaralarında ("dokuz bin yedi yüz doksan iki").
    Thousand,
}

fn token(w: &str) -> Option<Tok> {
    // `fold` sonrası biçim (ı→i) ve ASCII yazımlar.
    Some(match w {
        "sifir" => Tok::Zero,
        "bir" => Tok::Ones(1),
        "iki" => Tok::Ones(2),
        "üç" | "uc" => Tok::Ones(3),
        "dört" | "dort" => Tok::Ones(4),
        "beş" | "bes" => Tok::Ones(5),
        "alti" => Tok::Ones(6),
        "yedi" => Tok::Ones(7),
        "sekiz" => Tok::Ones(8),
        "dokuz" => Tok::Ones(9),
        "on" => Tok::Tens(1),
        "yirmi" => Tok::Tens(2),
        "otuz" => Tok::Tens(3),
        "kirk" => Tok::Tens(4),
        "elli" => Tok::Tens(5),
        "altmiş" | "altmis" => Tok::Tens(6),
        "yetmiş" | "yetmis" => Tok::Tens(7),
        "seksen" => Tok::Tens(8),
        "doksan" => Tok::Tens(9),
        "yüz" | "yuz" => Tok::Hundred,
        "bin" => Tok::Thousand,
        _ => return None,
    })
}

/// Sayı kelimesi dizisinin olası rakam okumaları.
///
/// "doksan beş yüz yetmiş" hem 95|170 hem 90|570 okunabilir: birler kelimesinin
/// ardından `yüz` geliyorsa önceki gruba da, yeni gruba da ait olabilir. İlk okuma
/// yeni grubu seçer; diğerleri dallanarak üretilir. Okumaların uzunluğu aynıdır.
fn readings(toks: &[Tok]) -> Vec<Vec<u8>> {
    const MAX_READINGS: usize = 64;
    fn go(toks: &[Tok], mut i: usize, acc: &mut Vec<u8>, outs: &mut Vec<Vec<u8>>) {
        if outs.len() >= MAX_READINGS {
            return;
        }
        let Some(&tok) = toks.get(i) else {
            outs.push(acc.clone());
            return;
        };
        let hundred_at = |k: usize| toks.get(k) == Some(&Tok::Hundred);
        let mark = acc.len();
        // Grubun birler hanesi: `j`'deki birler kelimesi (varsa). Ardından `yüz`
        // geliyorsa iki seçenek: yeni grubu başlatır (önce) ya da bu gruba katılır.
        let finish = |acc: &mut Vec<u8>, outs: &mut Vec<Vec<u8>>, j: usize, prefix: &[u8]| {
            let ones = match toks.get(j) {
                Some(&Tok::Ones(o)) => Some(o),
                _ => None,
            };
            match ones {
                Some(o) if hundred_at(j + 1) => {
                    acc.extend_from_slice(prefix);
                    acc.push(b'0');
                    go(toks, j, acc, outs);
                    acc.truncate(mark);
                    acc.extend_from_slice(prefix);
                    acc.push(b'0' + o);
                    go(toks, j + 1, acc, outs);
                }
                Some(o) => {
                    acc.extend_from_slice(prefix);
                    acc.push(b'0' + o);
                    go(toks, j + 1, acc, outs);
                }
                None => {
                    acc.extend_from_slice(prefix);
                    acc.push(b'0');
                    go(toks, j, acc, outs);
                }
            }
            acc.truncate(mark);
        };
        match tok {
            Tok::Zero => {
                acc.push(b'0');
                go(toks, i + 1, acc, outs);
                acc.truncate(mark);
            }
            Tok::Ones(o) if !hundred_at(i + 1) => {
                acc.push(b'0' + o);
                go(toks, i + 1, acc, outs);
                acc.truncate(mark);
            }
            Tok::Tens(t) => finish(acc, outs, i + 1, &[b'0' + t]),
            // Binli diziler `thousands_reading` ile okunur; buraya gelmez.
            Tok::Thousand => {}
            Tok::Ones(_) | Tok::Hundred => {
                let h = match tok {
                    Tok::Ones(h) => {
                        i += 2;
                        h
                    }
                    _ => {
                        i += 1;
                        1
                    }
                };
                let (t, j) = match toks.get(i) {
                    Some(&Tok::Tens(t)) => (t, i + 1),
                    _ => (0, i),
                };
                finish(acc, outs, j, &[b'0' + h, b'0' + t]);
            }
        }
    }
    let mut outs = Vec::new();
    go(toks, 0, &mut Vec::with_capacity(toks.len() * 2), &mut outs);
    outs
}

/// Her grubu "[<birler>] bin [[<birler>] yüz] [<onlar>] [<birler>]" olan dizinin dört
/// haneli gruplarla okunuşu: "dokuz bin yedi yüz doksan iki dört bin ..." → 9792 4...
/// 1xxx grubu "bin" diye söylenir, "bir bin" denmez: "bin"den önceki "bir" önceki grubun
/// birler hanesidir ("kırk bir bin dokuz yüz" → …41|1900), "iki".."dokuz" ise sonraki
/// grubun çarpanı. Böylece okuma tektir. Bir grup biçime uymuyorsa `None`.
fn thousands_reading(toks: &[Tok]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(toks.len());
    let mut i = 0;
    while i < toks.len() {
        let m = match (toks.get(i), toks.get(i + 1)) {
            (Some(&Tok::Ones(o)), Some(&Tok::Thousand)) if o != 1 => {
                i += 2;
                o
            }
            (Some(&Tok::Thousand), _) => {
                i += 1;
                1
            }
            _ => return None,
        };
        let h = match (toks.get(i), toks.get(i + 1)) {
            (Some(&Tok::Ones(o)), Some(&Tok::Hundred)) => {
                i += 2;
                o
            }
            (Some(&Tok::Hundred), _) => {
                i += 1;
                1
            }
            _ => 0,
        };
        let t = match toks.get(i) {
            Some(&Tok::Tens(t)) => {
                i += 1;
                t
            }
            _ => 0,
        };
        let o = match (toks.get(i), toks.get(i + 1)) {
            (Some(&Tok::Ones(o)), next) if o == 1 || next != Some(&Tok::Thousand) => {
                i += 1;
                o
            }
            _ => 0,
        };
        out.extend([b'0' + m, b'0' + h, b'0' + t, b'0' + o]);
    }
    Some(out)
}

fn classify(text: &str, start: usize, all: &[Vec<u8>], kinds: KindSet) -> Option<(Kind, u8)> {
    let d = all.first()?.as_slice();
    if !(MIN_DIGITS..=MAX_DIGITS).contains(&d.len()) {
        return None;
    }
    let tckn_ctx = || context_has(text, start, 60, TCKN_WORDS);
    if kinds.contains(Kind::Tckn) && d.len() == 11 && d[0] != b'0' {
        if all.iter().any(|r| is_valid_tckn(r)) {
            return Some((Kind::Tckn, 90));
        }
        if tckn_ctx() {
            return Some((Kind::Tckn, 75));
        }
    }
    if kinds.contains(Kind::Phone) {
        let (core, prefixed) = match d.len() {
            12 if d.starts_with(b"90") => (&d[2..], true),
            11 if d[0] == b'0' => (&d[1..], true),
            10 => (d, false),
            _ => return None,
        };
        let ctx = context_has(text, start, 60, PHONE_WORDS);
        let ok = match core[0] {
            b'5' => true,
            b'2' | b'3' | b'4' => prefixed || ctx,
            b'8' => prefixed && (core.starts_with(b"850") || core.starts_with(b"800")),
            _ => false,
        };
        if ok {
            return Some((Kind::Phone, if prefixed || ctx { 85 } else { 72 }));
        }
    }
    None
}

pub(crate) fn scan(text: &str, kinds: KindSet, claimed: &[core::ops::Range<usize>], out: &mut Vec<Match>) {
    // Kelimeler: (başlangıç, bitiş, tür). Sayı kelimesi olmayanlar zinciri böler.
    let mut run: Vec<(usize, usize, Tok)> = Vec::new();
    let free = |s: usize, e: usize| !claimed.iter().any(|r| s < r.end && e > r.start);
    let numbers = |run: &[(usize, usize, Tok)], out: &mut Vec<Match>| {
        if run.len() >= 4 {
            let toks: Vec<Tok> = run.iter().map(|x| x.2).collect();
            let (s, e) = (run[0].0, run[run.len() - 1].1);
            if free(s, e) {
                if let Some((kind, conf)) = classify(text, s, &readings(&toks), kinds) {
                    out.push(Match::new(kind, s..e, conf));
                }
            }
        }
    };
    let flush = |run: &mut Vec<(usize, usize, Tok)>, out: &mut Vec<Match>| {
        if run.iter().any(|x| x.2 == Tok::Thousand) {
            // Dörtlü gruplarla okunan kart numarası; değilse "bin"de bölünür (eski davranış).
            let toks: Vec<Tok> = run.iter().map(|x| x.2).collect();
            let card = kinds.contains(Kind::Card)
                && thousands_reading(&toks).is_some_and(|d| {
                    (13..=19).contains(&d.len())
                        && (matches!(d[0], b'3'..=b'6') || d.starts_with(b"9792"))
                        && luhn_valid(&d)
                });
            let (s, e) = (run[0].0, run[run.len() - 1].1);
            if card && free(s, e) {
                out.push(Match::new(Kind::Card, s..e, 88));
            } else {
                for part in run.split(|x| x.2 == Tok::Thousand) {
                    numbers(part, out);
                }
            }
        } else {
            numbers(run, out);
        }
        run.clear();
    };
    let mut last_end = 0;
    let mut buf = String::with_capacity(16);
    let b = text.as_bytes();
    let n = b.len();
    let alpha_at = |i: usize| -> Option<usize> {
        // Harfse karakterin bayt uzunluğu.
        let c = b[i];
        if c < 0x80 {
            return c.is_ascii_alphabetic().then_some(1);
        }
        if crate::text::tr_letter(b, i).is_some() {
            return Some(2);
        }
        let ch = text[i..].chars().next()?;
        ch.is_alphabetic().then(|| ch.len_utf8())
    };
    let mut i = 0;
    while i < n {
        if !text.is_char_boundary(i) {
            i += 1;
            continue;
        }
        let Some(l) = alpha_at(i) else {
            i += 1;
            continue;
        };
        let s = i;
        let mut e = i + l;
        while e < n {
            match alpha_at(e) {
                Some(l) => e += l,
                None => break,
            }
        }
        i = e;
        // Sayı kelimeleri bu harflerle başlar (büyük/küçük; Ü/ü UTF-8'de 0xC3 ile).
        let first_ok = matches!(b[s] | 0x20, b's' | b'b' | b'i' | b'u' | b'd' | b'a' | b'y' | b'o' | b'k' | b'e')
            || b[s] == 0xC3
            || b[s] == 0xC4;
        // `fold` ile aynı dönüşüm, kelime başına bellek tahsisi olmadan.
        let tok = if e - s <= 8 && first_ok {
            buf.clear();
            for ch in text[s..e].chars() {
                match ch {
                    'I' | 'İ' | 'ı' | 'i' => buf.push('i'),
                    c if c.is_ascii() => buf.push(c.to_ascii_lowercase()),
                    _ => buf.extend(ch.to_lowercase()),
                }
            }
            token(&buf)
        } else {
            None
        };
        // Kelimeler arasında yalnızca boşluk olmalı.
        let joined = !run.is_empty() && text[last_end..s].bytes().all(|b| b == b' ');
        if !joined || tok.is_none() {
            flush(&mut run, out);
        }
        if let Some(t) = tok {
            run.push((s, e, t));
        }
        last_end = e;
    }
    flush(&mut run, out);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::fold;

    fn run(s: &str) -> Vec<(Kind, String)> {
        let mut v = Vec::new();
        scan(s, KindSet::all(), &[], &mut v);
        v.iter().map(|m| (m.kind, m.text(s).to_string())).collect()
    }

    fn reads(s: &str) -> Vec<String> {
        let t: Vec<Tok> = s.split(' ').map(|w| token(&fold(w)).unwrap()).collect();
        readings(&t).into_iter().map(|d| String::from_utf8(d).unwrap()).collect()
    }

    #[test]
    fn grouping() {
        assert_eq!(reads("sıfır beş yüz otuz beş iki yüz seksen üç on yedi yetmiş altı"), ["05352831776"]);
        assert_eq!(reads("beş yüz yedi yedi yüz beş otuz iki kırk sekiz"), ["5077053248"]);
        assert_eq!(reads("SIFIR BES YUZ ELLI DORT"), ["0554"]);
        assert_eq!(reads("yüz on"), ["110"]);
        // belirsiz: 520|290|571|14, 520|295|171|14 (ve "yirmi iki yüz" dalları)
        let r = reads("beş yüz yirmi iki yüz doksan beş yüz yetmiş bir on dört");
        assert_eq!(r[0], "52029057114");
        assert!(r.contains(&"52029517114".to_string()));
        assert!(r.iter().all(|x| x.len() == 11));
    }

    #[test]
    fn phone_and_tckn() {
        let s = "müşteri telefonum sıfır beş yüz otuz beş iki yüz seksen üç on yedi yetmiş altı bu numaradan";
        assert_eq!(run(s), [(Kind::Phone, "sıfır beş yüz otuz beş iki yüz seksen üç on yedi yetmiş altı".into())]);
        // 10000000146 geçerli bir TCKN'dir.
        let s = "tc kimlik on milyon değil: on sıfır sıfır sıfır sıfır sıfır sıfır bir kırk altı";
        assert_eq!(run(s).len(), 1);
        assert_eq!(run(s)[0].0, Kind::Tckn);
    }

    #[test]
    fn card_in_thousand_groups() {
        // 4532 2234 5678 9012 (Luhn geçerli)
        let k = "dört bin beş yüz otuz iki iki bin iki yüz otuz dört beş bin altı yüz yetmiş sekiz dokuz bin on iki";
        assert_eq!(run(&format!("kart numaram {k} tamam")), [(Kind::Card, k.into())]);
        // "bin"den önceki "bir" önceki gruba aittir: 9792 2341 1971 5975.
        let k = "dokuz bin yedi yüz doksan iki iki bin üç yüz kırk bir bin dokuz yüz yetmiş bir beş bin dokuz yüz yetmiş beş";
        assert_eq!(thousands_reading(&k.split(' ').map(|w| token(&fold(w)).unwrap()).collect::<Vec<_>>()).unwrap(), b"9792234119715975");
        // Son hane farklı: Luhn tutmaz, kart değil.
        let k = "dört bin beş yüz otuz iki iki bin iki yüz otuz dört beş bin altı yüz yetmiş sekiz dokuz bin on beş";
        assert!(run(k).iter().all(|(kind, _)| *kind != Kind::Card));
        // Sıfırla başlayan grup binli okunmaz.
        let k = "dokuz bin yedi yüz doksan iki sıfır sıfır sıfır sıfır sıfır sıfır sıfır sıfır sıfır sıfır sıfır üç";
        assert!(run(k).iter().all(|(kind, _)| *kind != Kind::Card));
        // "bin" sıradan metinde diziyi eskisi gibi böler.
        assert!(run("beş bin kişi geldi, iki bin üç yüz otuz ikisi kaldı").is_empty());
    }

    #[test]
    fn ordinary_text_is_ignored() {
        assert!(run("bir iki üç dört diye saydı, beş yüz kişi geldi").is_empty());
        assert!(run("on iki ay boyunca yüz yirmi bin lira ödedik").is_empty());
    }
}
