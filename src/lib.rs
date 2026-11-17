//! # kalkan 🛡️
//!
//! Türkçe kişisel verileri LLM'e gitmeden **tespit eden, maskeleyen ve cevapta geri
//! yükleyen** sıfır bağımlılıklı KVKK kalkanı.
//!
//! - **Sağlama toplamı doğrulamalı**: TCKN, VKN, IBAN (mod-97), kart (Luhn)
//! - **Türkçe'ye duyarlı**: `Yılmaz'ın` → yalnızca ad maskelenir, ek kalır; `İ/ı` doğru işlenir
//! - **Geri yüklenebilir**: oturum kasası, akış (streaming) cevaplarda bile
//! - **Sentetik mod**: LLM'e biçimi ve sağlaması geçerli sahte değerler gider
//! - **Sıfır bağımlılık**: WASM'a, gömülü sistemlere, Python'a kolayca taşınır
//!
//! ```
//! use kalkan::{Kalkan, Strategy};
//!
//! let kalkan = Kalkan::builder().strategy(Strategy::Token).build();
//! let mut oturum = kalkan.session();
//!
//! let istem = oturum.mask("Sayın Ayşe Kaya, IBAN'ınız TR33 0006 1005 1978 6457 8413 26.");
//! assert_eq!(istem, "Sayın [AD_SOYAD_1], IBAN'ınız [IBAN_1].");
//!
//! // ... istem LLM'e gider, cevap döner ...
//! let cevap = oturum.restore("[AD_SOYAD_1] adına [IBAN_1] hesabı doğrulandı.");
//! assert_eq!(cevap, "Ayşe Kaya adına TR33 0006 1005 1978 6457 8413 26 hesabı doğrulandı.");
//! ```

#![cfg_attr(not(feature = "ffi"), forbid(unsafe_code))]
#![warn(missing_docs)]

pub mod checksum;
mod detect;
mod entity;
mod mask;
mod text;

pub use detect::Terms;
pub use entity::{Kind, KindSet, Match};
pub use mask::{LeakReport, Session, Strategy, StreamRestorer};

#[cfg(feature = "ffi")]
pub mod ffi;

use std::sync::Arc;

/// Yapılandırılmış kalkan. Ucuz klonlanır (terim listesi `Arc` ile paylaşılır),
/// iş parçacıkları arasında güvenle paylaşılabilir (`Send + Sync`).
#[derive(Debug, Clone)]
pub struct Kalkan {
    kinds: KindSet,
    strategy: Strategy,
    min_confidence: u8,
    secret: u64,
    terms: Option<Arc<Terms>>,
}

impl Default for Kalkan {
    fn default() -> Self {
        Self {
            kinds: KindSet::all(),
            strategy: Strategy::Token,
            min_confidence: 70,
            secret: 0x6b61_6c6b_616e,
            terms: None,
        }
    }
}

impl Kalkan {
    /// Varsayılan ayarlarla: tüm türler, `Token` stratejisi, güven eşiği 70.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Yapılandırıcı.
    #[must_use]
    pub fn builder() -> Builder {
        Builder(Self::default())
    }

    /// Metindeki kişisel verileri bulur (konuma göre sıralı, çakışmasız).
    #[must_use]
    pub fn detect(&self, text: &str) -> Vec<Match> {
        detect::detect(text, self.kinds, self.min_confidence, self.terms.as_deref())
    }

    /// Yeni bir oturum (kasa) açar. Her sohbet/istek için bir oturum kullanın.
    #[must_use]
    pub fn session(&self) -> Session {
        Session::new(self.clone())
    }

    /// Tek seferlik maskeleme (geri yükleme gerekmiyorsa).
    #[must_use]
    pub fn mask(&self, text: &str) -> String {
        self.session().mask(text)
    }

    /// Metin kişisel veri içeriyor mu?
    #[must_use]
    pub fn contains_pii(&self, text: &str) -> bool {
        !self.detect(text).is_empty()
    }

    /// Kullanılan strateji.
    #[must_use]
    pub fn strategy(&self) -> Strategy {
        self.strategy
    }
}

/// [`Kalkan`] yapılandırıcısı.
#[derive(Debug, Clone)]
pub struct Builder(Kalkan);

impl Builder {
    /// Maskeleme stratejisi.
    #[must_use]
    pub fn strategy(mut self, s: Strategy) -> Self {
        self.0.strategy = s;
        self
    }

    /// Yalnızca bu türleri ara.
    #[must_use]
    pub fn kinds(mut self, k: impl IntoIterator<Item = Kind>) -> Self {
        self.0.kinds = k.into_iter().collect();
        self
    }

    /// Bir türü devre dışı bırak.
    #[must_use]
    pub fn without(mut self, k: Kind) -> Self {
        self.0.kinds = self.0.kinds.without(k);
        self
    }

    /// En düşük güven puanı (0-100). Yükseltmek yanlış pozitifleri azaltır.
    #[must_use]
    pub fn min_confidence(mut self, c: u8) -> Self {
        self.0.min_confidence = c;
        self
    }

    /// Sentetik değer üretiminin gizli anahtarı. Aynı anahtar → aynı sahte değerler.
    #[must_use]
    pub fn secret(mut self, s: u64) -> Self {
        self.0.secret = s;
        self
    }

    /// Kuruma özel terimler (müşteri, proje, ürün adları...). Türkçe büyük/küçük
    /// harf duyarsız ve kelime sınırına duyarlı eşleşir; `OZEL` türüyle maskelenir.
    ///
    /// ```
    /// let k = kalkan::Kalkan::builder().terms(["Proje Atlas", "Acme Holding"]).build();
    /// assert_eq!(k.mask("PROJE ATLAS bütçesi"), "[OZEL_1] bütçesi");
    /// ```
    #[must_use]
    pub fn terms<I: IntoIterator<Item = S>, S: AsRef<str>>(mut self, terms: I) -> Self {
        let t = Terms::new(terms);
        self.0.terms = (!t.is_empty()).then(|| Arc::new(t));
        self
    }

    /// Kalkanı oluştur.
    #[must_use]
    pub fn build(self) -> Kalkan {
        self.0
    }
}
