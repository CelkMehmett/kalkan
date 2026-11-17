//! Kuruma özel terimler: müşteri, proje, ürün kod adları gibi kullanıcının
//! verdiği ifadeler. Türkçe büyük/küçük harf duyarsız, kelime sınırına duyarlı.

use std::collections::HashMap;

use crate::entity::{Kind, Match};
use crate::text::{boundary_after, boundary_before, fold, fold_with_map};

/// Önceden katlanmış (bkz. `text::fold`), uzundan kısaya sıralı terim listesi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Terms(Vec<String>);

impl Terms {
    pub(crate) fn new<I: IntoIterator<Item = S>, S: AsRef<str>>(it: I) -> Self {
        let mut v: Vec<String> = it
            .into_iter()
            .map(|s| fold(s.as_ref().trim()))
            .filter(|s| !s.is_empty())
            .collect();
        v.sort_by_key(|s| core::cmp::Reverse(s.len()));
        v.dedup();
        Terms(v)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn scan(&self, text: &str, out: &mut Vec<Match>) {
        if self.0.is_empty() {
            return;
        }
        let (lower, map) = fold_with_map(text);
        let mut positions: HashMap<u8, Vec<usize>> = HashMap::new();
        for (p, _) in lower.char_indices() {
            positions.entry(lower.as_bytes()[p]).or_default().push(p);
        }
        for term in &self.0 {
            let Some(first) = term.as_bytes().first() else { continue };
            let Some(candidates) = positions.get(first) else { continue };
            for &p in candidates {
                if !lower[p..].starts_with(term.as_str()) {
                    continue;
                }
                let (s, e) = (map[p], map[p + term.len()]);
                if s < e && boundary_before(text, s) && boundary_after(text, e) {
                    out.push(Match::new(Kind::Custom, s..e, 99));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turkish_case_insensitive() {
        let t = Terms::new(["Proje Işık", "acme", "Acme Holding"]);
        let s = "PROJE IŞIK ve proje ışık, ACME'nin acmeli değil, ACME HOLDING";
        let mut v = Vec::new();
        t.scan(s, &mut v);
        let got: Vec<&str> = v.iter().map(|m| m.text(s)).collect();
        assert_eq!(got, ["ACME HOLDING", "PROJE IŞIK", "proje ışık", "ACME", "ACME"]);
    }
}
