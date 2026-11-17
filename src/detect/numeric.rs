//! Rakam dizilerini tek geçişte tarayan dedektör: TCKN, VKN, telefon, kart.
//!
//! Strateji: Ayırıcılarla (boşluk, `-`, `.`, `/`, parantez) bölünmüş rakam
//! gruplarını bir "koşu" olarak toplar, sonra ardışık grup pencerelerini en
//! uzundan kısaya doğru sınıflandırır. Böylece `"TC 12345678950 tel 0532 123 45 67"`
//! veya yan yana yazılmış iki numara doğru ayrılır.

use crate::checksum::{is_valid_tckn, is_valid_vkn, luhn_valid};
use crate::entity::{Kind, KindSet, Match};
use crate::text::{boundary_after, boundary_before, context_has};

const VKN_KEYWORDS: &[&str] = &["vergi", "vkn", "v.k.n", "vno", "v.no", "tax"];
const MAX_GROUPS: usize = 12;
const MAX_DIGITS: usize = 19;

#[inline]
fn is_sep(b: u8) -> bool {
    matches!(b, b' ' | b'-' | b'.' | b'/' | b'(' | b')')
}

pub(crate) fn scan(text: &str, kinds: KindSet, claimed: &[core::ops::Range<usize>], out: &mut Vec<Match>) {
    let bytes = text.as_bytes();
    let n = bytes.len();
    let mut groups: Vec<(usize, usize)> = Vec::with_capacity(MAX_GROUPS);
    let mut digits: Vec<u8> = Vec::with_capacity(32);
    let mut claim_idx = 0;
    let mut i = 0;

    while i < n {
        // Hızlı yol: bir sonraki rakama atla.
        match bytes[i..].iter().position(u8::is_ascii_digit) {
            Some(off) => i += off,
            None => break,
        }
        // Daha önce IBAN/e-posta/plaka olarak alınmış bölgeyi atla.
        while claim_idx < claimed.len() && claimed[claim_idx].end <= i {
            claim_idx += 1;
        }
        if claim_idx < claimed.len() && claimed[claim_idx].start <= i {
            i = claimed[claim_idx].end;
            continue;
        }
        if !boundary_before(text, i) {
            while i < n && bytes[i].is_ascii_digit() {
                i += 1;
            }
            continue;
        }

        // Koşuyu topla.
        groups.clear();
        let mut j = i;
        loop {
            let gs = j;
            while j < n && bytes[j].is_ascii_digit() {
                j += 1;
            }
            groups.push((gs, j));
            let mut k = j;
            let mut seps = 0;
            while k < n && seps < 2 && is_sep(bytes[k]) {
                k += 1;
                seps += 1;
            }
            let next_claimed = claim_idx < claimed.len() && claimed[claim_idx].start <= k;
            if seps > 0 && k < n && bytes[k].is_ascii_digit() && groups.len() < MAX_GROUPS && !next_claimed {
                j = k;
            } else {
                break;
            }
        }
        // Son grup harfe yapışıksa (ör. "123abc") koşudan çıkar.
        if !boundary_after(text, groups[groups.len() - 1].1) {
            groups.pop();
        }
        let run_end = groups.last().map_or(i, |g| g.1);
        let plus = i > 0 && bytes[i - 1] == b'+';

        classify_run(text, &groups, plus, kinds, &mut digits, out);
        i = run_end.max(i + 1);
        while i < n && bytes[i].is_ascii_digit() {
            i += 1;
        }
    }
}

fn classify_run(
    text: &str,
    groups: &[(usize, usize)],
    plus_prefix: bool,
    kinds: KindSet,
    digits: &mut Vec<u8>,
    out: &mut Vec<Match>,
) {
    let bytes = text.as_bytes();
    let g = groups.len();
    let mut s = 0;
    while s < g {
        let mut found = None;
        for e in (s..g).rev() {
            digits.clear();
            for &(a, b) in &groups[s..=e] {
                digits.extend_from_slice(&bytes[a..b]);
            }
            if digits.len() > MAX_DIGITS + 2 {
                continue;
            }
            let plus = plus_prefix && s == 0;
            if let Some((kind, conf)) = classify(text, digits, &groups[s..=e], plus, kinds) {
                found = Some((e, kind, conf, plus));
                break;
            }
        }
        match found {
            Some((e, kind, conf, plus)) => {
                let mut start = groups[s].0;
                let end = groups[e].1;
                // "+" ve açılış parantezi yalnızca telefon numarasının parçasıdır.
                let paren = start > 0 && bytes[start - 1] == b'(' && bytes[start..end].contains(&b')');
                if kind == Kind::Phone && (plus || paren) {
                    start -= 1;
                }
                out.push(Match::new(kind, start..end, conf));
                s = e + 1;
            }
            None => s += 1,
        }
    }
}

fn classify(
    text: &str,
    d: &[u8],
    groups: &[(usize, usize)],
    plus: bool,
    kinds: KindSet,
) -> Option<(Kind, u8)> {
    let single = groups.len() == 1;
    let start = groups[0].0;

    if kinds.contains(Kind::Tckn) && single && d.len() == 11 && is_valid_tckn(d) {
        return Some((Kind::Tckn, 99));
    }
    if kinds.contains(Kind::Vkn)
        && single
        && d.len() == 10
        && is_valid_vkn(d)
        && context_has(text, start, 48, VKN_KEYWORDS)
    {
        return Some((Kind::Vkn, 95));
    }
    if kinds.contains(Kind::Phone) {
        if let Some(conf) = phone(d, plus, single) {
            return Some((Kind::Phone, conf));
        }
    }
    if kinds.contains(Kind::Card) && (13..=19).contains(&d.len()) && matches!(d[0], b'3'..=b'6') {
        let shape_ok = single || (groups.len() >= 3 && groups.iter().all(|&(a, b)| (3..=6).contains(&(b - a))));
        if shape_ok && luhn_valid(d) && !d.iter().all(|&b| b == d[0]) {
            return Some((Kind::Card, 97));
        }
    }
    None
}

/// Türk telefon numarası mı? Ulusal 10 haneli çekirdeği çıkarıp alan koduna bakar.
fn phone(d: &[u8], plus: bool, single: bool) -> Option<u8> {
    let (core, prefixed) = match d.len() {
        14 if d.starts_with(b"0090") => (&d[4..], true),
        12 if d.starts_with(b"90") => (&d[2..], true),
        11 if d[0] == b'0' => (&d[1..], true),
        10 if !plus => (d, false),
        _ => return None,
    };
    let ok = match core[0] {
        b'5' => true,                                     // cep: 5xx
        b'2' | b'3' | b'4' => prefixed,                   // sabit hat: 2xx/3xx/4xx
        b'8' => prefixed && (core.starts_with(b"850") || core.starts_with(b"800")),
        _ => false,
    };
    if !ok {
        return None;
    }
    // Öneksiz, tek parça 10 hane (ör. 5321234567) daha belirsiz.
    Some(if prefixed || !single { 92 } else { 75 })
}
