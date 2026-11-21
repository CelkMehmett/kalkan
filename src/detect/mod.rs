//! Dedektörlerin orkestrasyonu ve çakışma çözümü.

mod address;
mod extra;
mod informal;
mod names;
mod numeric;
mod pattern;
mod spoken;
mod terms;

pub use terms::Terms;

use crate::entity::{Kind, KindSet, Match};

/// Metindeki tüm kişisel verileri bulur; sonuçlar konuma göre sıralı ve çakışmasızdır.
pub(crate) fn detect(text: &str, kinds: KindSet, min_confidence: u8, terms: Option<&Terms>) -> Vec<Match> {
    let mut out = Vec::new();

    // 1) Harf içeren kalıplar bölgelerini sahiplenir.
    if kinds.contains(Kind::Iban) {
        pattern::scan_iban(text, &mut out);
    }
    if kinds.contains(Kind::Email) {
        pattern::scan_email(text, &mut out);
    }
    if kinds.contains(Kind::Plate) {
        pattern::scan_plate(text, &mut out);
    }
    if kinds.contains(Kind::Ip) {
        extra::scan_ipv4(text, &mut out);
        extra::scan_ipv6(text, &mut out);
    }
    if kinds.contains(Kind::BirthDate) {
        extra::scan_birth_date(text, &mut out);
    }
    if kinds.contains(Kind::Passport) {
        extra::scan_passport(text, &mut out);
    }
    if kinds.contains(Kind::Address) {
        address::scan_address(text, &mut out);
        informal::scan_informal(text, &mut out);
    }
    if let Some(t) = terms.filter(|_| kinds.contains(Kind::Custom)) {
        t.scan(text, &mut out);
    }
    out.sort_unstable_by_key(|m| m.span.start);
    let claimed: Vec<_> = merge_ranges(out.iter().map(|m| m.span.clone()));

    // 2) Rakam dizileri (TCKN, VKN, telefon, kart).
    let numeric_kinds = [Kind::Tckn, Kind::Vkn, Kind::Phone, Kind::Card];
    if numeric_kinds.iter().any(|&k| kinds.contains(k)) {
        numeric::scan(text, kinds, &claimed, &mut out);
    }
    // Yazıyla söylenmiş telefon ve TCKN ("sıfır beş yüz otuz beş ...").
    if kinds.contains(Kind::Tckn) || kinds.contains(Kind::Phone) {
        spoken::scan(text, kinds, &claimed, &mut out);
    }
    // SGK sicil no rakam aşamasından sonra: "SGK no: <geçerli TCKN>" TCKN olarak kalır.
    if kinds.contains(Kind::SocialSecurity) {
        extra::scan_social_security(text, &mut out);
    }

    // 3) Adlar (daha önce sahiplenilen bölgelerle çakışmasın).
    if kinds.contains(Kind::Name) {
        names::scan(text, &claimed, &mut out);
    }

    out.retain(|m| m.confidence >= min_confidence);
    resolve(out)
}

fn merge_ranges(it: impl Iterator<Item = core::ops::Range<usize>>) -> Vec<core::ops::Range<usize>> {
    let mut v: Vec<core::ops::Range<usize>> = Vec::new();
    for r in it {
        match v.last_mut() {
            Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
            _ => v.push(r),
        }
    }
    v
}

/// Çakışmaları çözer: aynı yerden başlayanlarda uzun ve güvenilir olan kazanır.
fn resolve(mut v: Vec<Match>) -> Vec<Match> {
    v.sort_unstable_by(|a, b| {
        a.span
            .start
            .cmp(&b.span.start)
            .then(b.span.len().cmp(&a.span.len()))
            .then(b.confidence.cmp(&a.confidence))
    });
    let mut out: Vec<Match> = Vec::with_capacity(v.len());
    for m in v {
        match out.last() {
            Some(last) if m.span.start < last.span.end => {
                // Çakışma: daha güvenilir olan kazanır (eşitse önceki kalır).
                if m.confidence > last.confidence && m.span.end >= last.span.end {
                    out.pop();
                    out.push(m);
                }
            }
            _ => out.push(m),
        }
    }
    out
}
