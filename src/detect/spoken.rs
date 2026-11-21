//! Yazıyla söylenmiş numaralar: "sıfır beş yüz otuz beş iki yüz seksen üç on yedi
//! yetmiş altı" → 05352831776. Çağrı merkezi transkriptlerinde ve sesli asistan
//! çıktılarında telefon ve TC kimlik numaraları çoğunlukla böyle geçer.
//!
//! Sayı kelimeleri gruplara ayrılır (`beş yüz otuz beş` → 535, `on yedi` → 17,
//! `sıfır` → 0) ve gruplar birleştirilir. Bir birler kelimesinin ardından `yüz`
//! geliyorsa o kelime yeni grubu başlatır: "beş yüz yirmi | iki yüz doksan beş".
//! En az 10 haneli diziler TCKN (sağlamayla) ya da telefon olarak sınıflanır.

use crate::checksum::is_valid_tckn;
use crate::entity::{Kind, KindSet, Match};
use crate::text::{context_has, fold};

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
    let flush = |run: &mut Vec<(usize, usize, Tok)>, out: &mut Vec<Match>| {
        if run.len() >= 4 {
            let toks: Vec<Tok> = run.iter().map(|x| x.2).collect();
            let (s, e) = (run[0].0, run[run.len() - 1].1);
            if !claimed.iter().any(|r| s < r.end && e > r.start) {
                if let Some((kind, conf)) = classify(text, s, &readings(&toks), kinds) {
                    out.push(Match::new(kind, s..e, conf));
                }
            }
        }
        run.clear();
    };
    let mut last_end = 0;
    let mut it = text.char_indices().peekable();
    while let Some((s, c)) = it.next() {
        if !c.is_alphabetic() {
            continue;
        }
        let mut e = s + c.len_utf8();
        while let Some(&(k, ch)) = it.peek() {
            if !ch.is_alphabetic() {
                break;
            }
            e = k + ch.len_utf8();
            it.next();
        }
        let tok = if e - s <= 8 { token(&fold(&text[s..e])) } else { None };
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
    fn ordinary_text_is_ignored() {
        assert!(run("bir iki üç dört diye saydı, beş yüz kişi geldi").is_empty());
        assert!(run("on iki ay boyunca yüz yirmi bin lira ödedik").is_empty());
    }
}
