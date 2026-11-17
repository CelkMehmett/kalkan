//! Tespit edilen varlık türleri ve eşleşmeler.

use core::fmt;
use core::ops::Range;

/// Kalkan'ın tanıdığı kişisel veri türleri.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    /// T.C. Kimlik Numarası (11 hane, iki sağlama hanesi doğrulanır).
    Tckn,
    /// Vergi Kimlik Numarası (10 hane, sağlama + bağlam kelimesi gerekir).
    Vkn,
    /// Türk IBAN'ı (TR + 24 hane, mod-97 doğrulanır).
    Iban,
    /// Türk telefon numarası (cep, sabit, 0850/0800).
    Phone,
    /// E-posta adresi.
    Email,
    /// Kredi/banka kartı numarası (Luhn doğrulanır).
    Card,
    /// Türk araç plakası (il kodu 01-81).
    Plate,
    /// Kişi adı (unvan, etiket veya ad sözlüğü ile).
    Name,
    /// IP adresi (IPv4, IPv6). KVKK'ya göre kişisel veri sayılabilir.
    Ip,
    /// Doğum tarihi (yalnızca "doğum" gibi bir bağlam kelimesiyle).
    BirthDate,
    /// Pasaport numarası (yalnızca "pasaport" bağlamıyla).
    Passport,
    /// Kuruma özel terim (müşteri/proje adları vb.), kullanıcı tanımlı.
    Custom,
}

impl Kind {
    /// Tüm türler.
    pub const ALL: [Kind; 12] = [
        Kind::Tckn,
        Kind::Vkn,
        Kind::Iban,
        Kind::Phone,
        Kind::Email,
        Kind::Card,
        Kind::Plate,
        Kind::Name,
        Kind::Ip,
        Kind::BirthDate,
        Kind::Passport,
        Kind::Custom,
    ];

    /// Maskede görünen etiket (ör. `[TCKN_1]`).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Kind::Tckn => "TCKN",
            Kind::Vkn => "VKN",
            Kind::Iban => "IBAN",
            Kind::Phone => "TELEFON",
            Kind::Email => "EPOSTA",
            Kind::Card => "KART",
            Kind::Plate => "PLAKA",
            Kind::Name => "AD_SOYAD",
            Kind::Ip => "IP",
            Kind::BirthDate => "DOGUM_TARIHI",
            Kind::Passport => "PASAPORT",
            Kind::Custom => "OZEL",
        }
    }

    /// Etiketten türe çevirir (büyük/küçük harf duyarsız).
    #[must_use]
    pub fn from_label(s: &str) -> Option<Kind> {
        let up = s.trim().to_ascii_uppercase();
        Kind::ALL.into_iter().find(|k| k.label() == up).or(match up.as_str() {
            "TC" | "TC_KIMLIK" => Some(Kind::Tckn),
            "PHONE" | "TEL" => Some(Kind::Phone),
            "EMAIL" | "MAIL" => Some(Kind::Email),
            "CARD" | "KREDI_KARTI" => Some(Kind::Card),
            "PLATE" => Some(Kind::Plate),
            "NAME" | "AD" | "ISIM" => Some(Kind::Name),
            "IP_ADRESI" => Some(Kind::Ip),
            "DOGUM" | "BIRTHDATE" | "DOB" => Some(Kind::BirthDate),
            "PASSPORT" => Some(Kind::Passport),
            "CUSTOM" | "TERIM" => Some(Kind::Custom),
            _ => None,
        })
    }

    pub(crate) const fn bit(self) -> u16 {
        1 << (self as u16)
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Metinde bulunan tek bir kişisel veri.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// Veri türü.
    pub kind: Kind,
    /// Kaynak metindeki bayt aralığı (her zaman UTF-8 karakter sınırında).
    pub span: Range<usize>,
    /// 0-100 arası güven puanı. Sağlama toplamı doğrulananlar yüksek,
    /// sezgisel olanlar (ör. ad sözlüğü) daha düşüktür.
    pub confidence: u8,
}

impl Match {
    pub(crate) fn new(kind: Kind, span: Range<usize>, confidence: u8) -> Self {
        Self { kind, span, confidence }
    }

    /// Eşleşen metni döndürür.
    #[must_use]
    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        &source[self.span.clone()]
    }
}

/// Etkin türler kümesi (küçük bir bit maskesi).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KindSet(u16);

impl KindSet {
    /// Tüm türler.
    #[must_use]
    pub const fn all() -> Self {
        KindSet((1 << Kind::ALL.len()) - 1)
    }
    /// Boş küme.
    #[must_use]
    pub const fn empty() -> Self {
        KindSet(0)
    }
    /// Türü ekler.
    #[must_use]
    pub fn with(mut self, k: Kind) -> Self {
        self.0 |= k.bit();
        self
    }
    /// Türü çıkarır.
    #[must_use]
    pub fn without(mut self, k: Kind) -> Self {
        self.0 &= !k.bit();
        self
    }
    /// Tür kümede mi?
    #[must_use]
    pub const fn contains(self, k: Kind) -> bool {
        self.0 & k.bit() != 0
    }
}

impl Default for KindSet {
    fn default() -> Self {
        Self::all()
    }
}

impl FromIterator<Kind> for KindSet {
    fn from_iter<I: IntoIterator<Item = Kind>>(iter: I) -> Self {
        iter.into_iter().fold(KindSet::empty(), KindSet::with)
    }
}
