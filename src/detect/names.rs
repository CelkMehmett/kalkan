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
    "sayın", "sn", "usta", "uzm",
];
/// Sonra gelen hitaplar.
const SUFFIXES: &[&str] = &[
    "bey", "beyefendi", "hanım", "hanımefendi", "hoca", "hocam",
];
/// Sonra gelen akrabalık/yakınlık hitapları: "Funda teyze", "Ali abi". Önceki kelime
/// cümle başındaysa ("Komşu teyze") yalnızca ad sözlüğündeyse kabul edilir.
const KIN: &[&str] = &[
    "abi", "abla", "ağabey", "amca", "dayı", "efendi", "enişte", "teyze", "yenge",
];
/// Güçlü form etiketleri (iki noktadan sonra 1+ büyük harfli kelime yeter).
const STRONG_LABELS: &[&str] = &[
    "ad", "adi", "adı", "isim", "ismi", "name", "soyad", "soyadi", "soyadı",
];
/// Zayıf etiketler (en az 2 büyük harfli kelime ister).
/// İki noktasız da çalışırlar ("Hesap sahibi Tülay Ünal"), o zaman en az 2 kelime ister.
const WEAK_LABELS: &[&str] = &[
    "alıcı", "danışan", "davacı", "davalı", "gönderen", "gönderici", "hasta", "kiracı", "müvekkil",
    "müşteri", "personel", "sahibi", "sigortalı", "sürücü", "veli", "öğrenci",
];
/// Ad zincirine girmemesi gereken büyük harfli kelimeler.
const STOP: &[&str] = &[
    "adres", "adresi", "bilgileri", "birimi", "bölümü", "cad", "cadde", "caddesi", "departmanı",
    "dosyası", "eposta", "hattı", "hesabı", "hizmetleri", "iban", "kartı", "kaydı", "mah", "mahalle",
    "mahallesi", "merkezi", "müdürlüğü", "müdürü", "no", "numara", "numarası", "servisi", "sok",
    "sokak", "sorumlusu", "sözleşmesi", "tc", "tckn", "tel", "telefon", "temsilcisi", "vekili", "vkn",
    "yetkilisi", "şubesi",
];
/// Şirket unvanı, kurum ve yapı kelimeleri ("… Köprüsü", "… Lisesi"): ad zinciri bunlardan birini içerir ya da hemen ardından
/// bunlardan biri gelirse zincir kişi adı sayılmaz ("Çiğdem Mobilya", "Yılmaz İnşaat A.Ş.").
const ORG: &[&str] = &[
    "a", "ajans", "ajansi", "ajansı", "aracilik", "aracılık", "avm", "bakanligi", "bakanlığı",
    "bankasi", "bankası", "baskanligi", "başkanlığı", "belediye", "belediyesi", "bilisim",
    "bilişim", "bulvari", "bulvarı", "camii", "cumhuriyeti", "danismanlik", "danışmanlık",
    "dernegi", "derneği", "ecza", "eczanesi", "elektrik", "elektronik", "emlak", "enerji",
    "federasyonu", "gayrimenkul", "gida", "giyim", "grup", "gıda", "hastanesi", "havalimani",
    "havalimanı", "hizmetleri", "holding", "hukuk", "ihracat", "insaat", "inşaat", "ithalat",
    "kafe", "kaymakamlığı", "kimya", "kirtasiye", "kooperatifi", "koprusu", "kozmetik", "kuafor",
    "kuaför", "kurumu", "köprüsü", "külliyesi", "kırtasiye", "lisesi", "lojistik", "ltd", "makina",
    "makine", "market", "matbaa", "medikal", "meydani", "meydanı", "mimarlik", "mimarlık",
    "mobilya", "muhendislik", "mühendislik", "nakliyat", "nakliye", "odası", "okulu", "optik",
    "otomotiv", "parki", "parkı", "partisi", "pazarlama", "petrol", "restoran", "san", "sanayi",
    "sigorta", "sirketi", "stadyumu", "sti", "teknoloji", "teknolojileri", "tekstil", "tic",
    "ticaret", "turizm", "türbesi", "universitesi", "vakfi", "vakfı", "valiligi", "valiliği",
    "yapi", "yapı", "yatirim", "yatırım", "yayincilik", "yayinlari", "yayıncılık", "yayınları",
    "yazilim", "yazılım", "üniversitesi", "şirket", "şirketi", "şti",
];
/// Yer ve yapı adlarında, metinlerde sık geçen tarihî kişiler. Yaygın kişi adı da olan
/// birleşimler ("Yunus Emre") bilerek yok.
const HISTORICAL: &[&str] = &[
    "barbaros hayreddin", "fatih sultan mehmet", "ismet inönü", "kanuni sultan süleyman",
    "mehmet akif ersoy", "mimar sinan", "mustafa kemal atatürk", "nasreddin hoca", "piri reis",
    "yavuz sultan selim",
];
/// Çağrı dökümünde ad soran ifadeler ve konuşmacı etiketleri: "temsilci adınız
/// soyadınız müşteri necdet telli temsilci …".
const NAME_QUESTIONS: &[&str] = &["adınız", "adınızı", "adiniz", "adinizi", "isminiz", "isminizi", "soyadınız", "soyadiniz"];
const SPEAKERS: &[&str] = &["arayan", "müşteri", "musteri", "temsilci"];
/// Yanıtı veren taraf (temsilci soruyu sorar, yanıt vermez).
const ANSWERERS: &[&str] = &["arayan", "müşteri", "musteri"];
/// Kendini tanıtma: ardından gelen büyük harfli zincir ad ("adım Evren Uslu").
const INTRO: &[&str] = &["adim", "adım", "ben", "ismim"];
/// İmzadan önce gelen kapanış ifadeleri (iki kelimelikler boşlukla).
const CLOSINGS: &[&str] = &[
    "best regards", "hürmetlerimle", "iyi çalışmalar", "iyi günler", "kolay gelsin", "regards",
    "saygılar", "saygılarımla", "saygilarimla", "selamlar", "sevgiler", "teşekkürler",
    "tesekkurler",
];
/// Küçük harfli metin (transkript, mesajlaşma) için: ardından ad gelen kelimeler.
/// ("adı", "sahibi", "usta" küçük harfte çok belirsiz: "adı yok", "ev sahibi evi", "usta yarın".)
const LOWER_BEFORE: &[&str] = &[
    "adim", "adım", "av", "avukat", "ismim", "müvekkil", "sayın", "soyadim", "soyadım",
];
/// Küçük harfli metin için: önünde ad olan kelimeler ("meltem tuna hanım", "x isimli").
const LOWER_AFTER: &[&str] = &[
    "adinda", "adında", "adındaki", "adli", "adlı", "bey", "beye", "beyin", "hanim", "hanım",
    "hanıma", "hanımın", "isimli",
];
/// Küçük harfli bağlam kurallarında ad olamayacak yaygın kelimeler.
const LOWER_COMMON: &[&str] = &[
    "abi", "abla", "ad", "adres", "adına", "akşamlar", "ama", "annem", "arkadaş", "arkadaşım", "babam",
    "bana", "ben", "benim", "bey", "bir", "biri", "birisi", "biz", "bu", "bugün", "da", "daha", "de",
    "değerli", "diye", "doktor", "ee", "efendim", "en", "eski", "eşim", "evet", "firma", "gibi",
    "güzel", "günler", "hanım", "hayır", "hemen", "hesap", "hocam", "için", "ile", "isim", "iyi",
    "kanka", "kardeşim", "kayıtlı", "kendi", "ki", "kim", "kimse", "kişi", "kızım", "komşu", "komşum",
    "merhaba", "mi", "mı", "müşteri", "naber", "nasılsın", "ne", "nedir", "numara", "o", "olan",
    "olarak", "oğlum", "peki", "sahibi", "sayın", "selam", "sen", "siz", "soyad", "şey", "şimdi",
    "şirket", "şu", "tamam", "telefon", "temsilci", "teşekkür", "teşekkürler", "usta", "ve", "yani",
    "yeni", "aynı", "doğru", "eksik", "farklı", "hatalı", "var", "yanlış", "yarın", "yerine", "yok",
    "değil",
];
/// Küçük harfli adaylarda çekimli fiil sonları ("değişmiş", "yazıyor", "gelecek").
const VERB_ENDINGS: &[&str] = &["mış", "miş", "muş", "müş", "yor", "acak", "ecek"];
/// Hitap sonrası büyük harfle yazılsa da ad olmayan kelimelerin başları:
/// "Sayın İlgili", "Sayın Müşterimiz", "Sayın Hâkimliğe", "Değerli Hocam", "Sayın Meslektaşım".
const NOT_NAME_PREFIX: &[&str] = &[
    "abone", "bakanlı", "bayi", "başkan", "degerli", "değerli", "hakim", "heyet", "hâkim", "ilgili",
    "katılımcı", "kaymakam", "komisyon", "kullanıcı", "kurul", "kıymetli", "mahkeme", "meslektaş",
    "müdür", "müşteri", "paydaş", "savcılı", "sevgili", "tedarikçi", "valili", "yetkili", "üye",
];

/// Ad ve soyad sözlükleri (`veri/`): satır başına bir kelime, `#` ile başlayan satırlar
/// açıklama. "Deniz, Umut, Barış, Can, Gül" gibi aynı zamanda sıradan kelime olanlar
/// bilerek dışarıda; onlar yalnızca unvan/hitapla yakalanır.
const FIRST_NAMES: &str = include_str!("veri/adlar.txt");
/// En yaygın Türk soyadlarından aynı zamanda sıradan kelime olanlar (elle derlendi).
/// Sözlükte değiller; yalnızca sözlükteki bir addan hemen sonra soyadı sayılırlar
/// ("nazım yıldız", "bircan kaya"). Çok kısa olanlar ("can", "ay", "er") ve addan sonra
/// sıfat olarak okunabilenler ("kara", "güzel", "mutlu", "korkmaz") yok.
const COMMON_WORD_SURNAMES: &[&str] = &[
    "acar", "akar", "akın", "aksoy", "arslan", "aslan", "ateş", "avcı", "aydın", "balta", "bayram",
    "bozkurt", "budak", "bulut", "çakır", "çam", "çelik", "çiçek", "demir", "demirci", "doğan",
    "erdem", "fidan", "güler", "gül", "güneş", "ışık", "kaplan", "kartal", "kavak", "kaya", "kılıç",
    "koç", "köse", "kurt", "polat", "savaş", "şahin", "şimşek", "tekin", "toprak", "tuna", "tunç",
    "turan", "yıldırım", "yıldız", "yüksel",
];
const SURNAMES: &str = include_str!("veri/soyadlar.txt");

fn lines(s: &'static str) -> impl Iterator<Item = &'static str> {
    s.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#'))
}

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
const F_TITLE: u16 = 1;
const F_SUFFIX: u16 = 2;
const F_STRONG: u16 = 4;
const F_WEAK: u16 = 8;
const F_STOP: u16 = 16;
const F_FIRST: u16 = 32;
const F_KIN: u16 = 64;
const F_SUR: u16 = 128;
const F_ORG: u16 = 256;
const F_SUR2: u16 = 512;

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

type Lexicon = HashMap<&'static str, u16, BuildHasherDefault<Fnv>>;

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
            (KIN, F_KIN),
            (ORG, F_ORG),
            (COMMON_WORD_SURNAMES, F_SUR2),
        ] {
            for &w in set {
                *m.entry(w).or_insert(0) |= flag;
            }
        }
        for (set, flag) in [(FIRST_NAMES, F_FIRST), (SURNAMES, F_SUR)] {
            for w in lines(set) {
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
    // `out` önceki dedektörlerin konuma göre sıralı olmayan eşleşmelerini de taşır;
    // "az önce bulunan ad" denetimleri yalnızca bu aşamanın eklediklerine bakmalı.
    let base = out.len();
    let flags: Vec<u16> = words
        .iter()
        .map(|w| if w.le > w.ls { lex.get(&arena[w.ls..w.le]).copied().unwrap_or(0) } else { 0 })
        .collect();
    let has = |k: usize, f: u16| flags[k] & f != 0;
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
    let common = |k: usize| {
        let w = &arena[words[k].ls..words[k].le];
        !w.is_empty() && NOT_NAME_PREFIX.iter().any(|p| w.starts_with(p))
    };
    // Kelime cümle başında mı (büyük harfi ad olduğunu göstermez).
    let sentence_start = |k: usize| {
        k == 0 || {
            let g = gap(k - 1, k);
            g.contains(['.', '!', '?', '\n', ':', '-']) && !title_gap(g)
        }
    };
    // Önceki ayraçta noktalama var mı ("… adına. Hesap", "ekip, Annem"). `sentence_start`
    // ". " ayracını unvan ayracı ("Dr. Ahmet") saydığı için cümle başlarını kaçırabilir.
    let after_punct = |k: usize| k == 0 || gap(k - 1, k).contains(['.', '!', '?', '\n', ':', ',']);
    let nameable = |k: usize| {
        words[k].cap
            && !has(k, F_STOP | F_TITLE | F_SUFFIX | F_KIN)
            && !common(k)
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
        // Şirket unvanı: "Çiğdem Mobilya", "Yılmaz İnşaat A.Ş.", "Polat Gıda San. ve Tic."
        let org_next = end + 1 < n && space_gap(end, end + 1) && words[end + 1].cap && has(end + 1, F_ORG);
        if org_next || (k..=end).any(|j| has(j, F_ORG)) {
            return None;
        }
        Some((k, end))
    };
    let push = |out: &mut Vec<Match>, a: usize, b: usize, conf: u8| {
        out.push(Match::new(Kind::Name, words[a].s..words[b].e, conf));
    };

    // "6 Kasım": önce sayı geliyorsa ay adıdır.
    let date_before = |k: usize| {
        let before = text[..words[k].s].trim_end_matches([' ', '.']);
        before.ends_with(|c: char| c.is_ascii_digit())
    };
    // "Fatih'te", "Ceyhan'dan": kesmeden sonra bulunma/ayrılma eki yer adını gösterir.
    let place_suffix = |k: usize| {
        let rest = &text[words[k].e..];
        let Some(ek) = rest.strip_prefix(['\'', '’']) else { return false };
        let ek: String = ek.chars().take_while(|c| c.is_alphabetic()).collect::<String>().to_lowercase();
        ["da", "de", "ta", "te", "dan", "den", "tan", "ten"].contains(&ek.as_str())
    };

    // Küçük harfli bağlam kuralı için aday kelime.
    let lw = |k: usize| &arena[words[k].ls..words[k].le];
    let lower_name = |k: usize| {
        !words[k].cap
            && words[k].le > words[k].ls
            && flags[k] & !(F_FIRST | F_SUR | F_SUR2) == 0
            && !LOWER_COMMON.contains(&lw(k))
            && !(lw(k).chars().count() > 4 && VERB_ENDINGS.iter().any(|e| lw(k).ends_with(e)))
            && !LOWER_BEFORE.contains(&lw(k))
            && !LOWER_AFTER.contains(&lw(k))
            && !common(k)
            && !label_after(k)
            && !in_claimed(k)
    };

    // Sözlükteki adla başlayan küçük harfli zincir: "izzet yekta demiroğlu", "ben banu".
    let lower_dict_chain = |k: usize| -> Option<usize> {
        if !(has(k, F_FIRST) && lower_name(k)) {
            return None;
        }
        let mut b = k;
        while b + 1 < n && b - k < 2 && space_gap(b, b + 1) && lower_name(b + 1) && has(b + 1, F_FIRST | F_SUR) {
            b += 1;
        }
        // Kelime olan yaygın soyadı yalnızca zincirin sonunda: "nazım yıldız".
        if b + 1 < n && space_gap(b, b + 1) && lower_name(b + 1) && has(b + 1, F_SUR2) {
            b += 1;
        }
        Some(b)
    };
    // İmza satırı: "Saygılarımla, Ayşegül Hakimi", "Kolay gelsin,\nZübeyir".
    let after_closing = |k: usize| {
        if k == 0 || !gap(k - 1, k).contains([',', '\n']) || gap(k - 1, k).len() > 4 {
            return false;
        }
        let last = lw(k - 1);
        let prev = if k >= 2 { lw(k - 2) } else { "" };
        CLOSINGS.iter().any(|c| match c.split_once(' ') {
            Some((a, b)) => prev == a && last == b,
            None => last == *c,
        })
    };
    let line_ends_after = |b: usize| {
        let rest = text[words[b].e..].trim_start_matches([' ', '.', ',']);
        rest.is_empty() || rest.starts_with(['\n', '\r'])
    };

    // Çağrı dökümü: ad soran ifadeden sonra (aynı satırda, en çok altı kelime içinde)
    // konuşmacı etiketi ve ardından bir-üç küçük harfli kelime, sonraki etikete kadar.
    let answer_after_question = |k: usize| -> Option<usize> {
        if k == 0 || words[k].cap || !ANSWERERS.contains(&lw(k - 1)) || !space_gap(k - 1, k) {
            return None;
        }
        let asked = (k.saturating_sub(7)..k - 1)
            .any(|q| NAME_QUESTIONS.contains(&lw(q)) && !text[words[q].e..words[k].s].contains('\n'));
        if !asked || !lower_name(k) {
            return None;
        }
        let mut b = k;
        while b + 1 < n && b - k < MAX_CHAIN - 1 && space_gap(b, b + 1) && lower_name(b + 1) && !SPEAKERS.contains(&lw(b + 1)) {
            b += 1;
        }
        // Yanıtın sonu: konuşmacı etiketi, noktalama ya da metin sonu.
        let ends = b + 1 == n || !space_gap(b, b + 1) || SPEAKERS.contains(&lw(b + 1));
        // "müşteri söylemek istemiyorum": yanıtta en az bir sözlük kelimesi olmalı.
        let dict = (k..=b).any(|j| has(j, F_FIRST | F_SUR | F_SUR2));
        (ends && dict).then_some(b)
    };
    // "Hilal Balta adına", "kaan budak adına kayıtlı": ardından "adına / namına" gelen
    // iki-üç kelime. Büyük harfte şirket unvanı zinciri zaten elenir; küçük harfte
    // en az bir kelime ad ya da soyad sözlüğünde olmalı.
    let behalf = |k: usize| k < n && matches!(lw(k), "adına" | "adina" | "namına" | "namina");
    let mut k = 0;
    while k < n {
        if let Some(b) = answer_after_question(k) {
            push(out, k, b, 80);
            k = b + 1;
            continue;
        }
        if words[k].cap {
            if let Some((mut a, b)) = chain(k) {
                if b > a && b + 1 < n && space_gap(b, b + 1) && behalf(b + 1) {
                    // Cümle başındaki büyük harf ad olduğunu göstermez: "Annem Sevilay
                    // Akkaya adına", "Hesap Furkan Esen adına".
                    if b - a == 2 && after_punct(a) && !has(a, F_FIRST) && has(a + 1, F_FIRST) {
                        a += 1;
                    }
                    push(out, a, b, 78);
                    k = b + 1;
                    continue;
                }
            }
        } else if k + 2 < n
            && lower_name(k)
            && lower_name(k + 1)
            && space_gap(k, k + 1)
            && space_gap(k + 1, k + 2)
            && behalf(k + 2)
            && (has(k, F_FIRST | F_SUR) || has(k + 1, F_FIRST | F_SUR | F_SUR2))
        {
            push(out, k, k + 1, 72);
            k += 2;
            continue;
        }
        // 6) Tanıtma: "adım Evren Uslu", "Ben Berçem Akbulak", "hoş geldiniz ben banu".
        if INTRO.contains(&lw(k)) && k + 1 < n && space_gap(k, k + 1) {
            if words[k + 1].cap {
                if let Some((a, b)) = chain(k + 1) {
                    if b > a || has(a, F_FIRST) || (lw(k) != "ben" && line_ends_after(a)) {
                        push(out, a, b, 80);
                        k = b + 1;
                        continue;
                    }
                }
            } else if lw(k) == "ben" {
                if let Some(b) = lower_dict_chain(k + 1) {
                    push(out, k + 1, b, 72);
                    k = b + 1;
                    continue;
                }
            }
        }
        // 7) İmza: kapanış ifadesinden sonra gelen ad.
        if words[k].cap && after_closing(k) {
            if let Some((a, b)) = chain(k) {
                // "Selamlar, Kargomu Bursa aktarmadan" selamlamadır: satır sonu, sözlükteki ad
                // ya da ardından gelen alan etiketi ("… Hakimi Tel:") gerekir.
                let field_next = b + 1 < n && space_gap(b, b + 1) && has(b + 1, F_STOP);
                if has(a, F_FIRST | F_SUR) || has(b, F_SUR) || line_ends_after(b) || field_next {
                    push(out, a, b, 75);
                    k = b + 1;
                    continue;
                }
            }
        }
        // 8) Küçük harfli iki+ sözlük kelimesi: "nimet özdemir", "izzet yekta".
        if !words[k].cap {
            if let Some(b) = lower_dict_chain(k) {
                if b > k {
                    push(out, k, b, 72);
                    k = b + 1;
                    continue;
                }
            }
        }

        // 5) Küçük harf: "adım mizgîn özbek", "av. aysun", "meltem tuna hanım".
        if !words[k].cap && LOWER_BEFORE.contains(&lw(k)) && k + 1 < n && title_gap(gap(k, k + 1)) && lower_name(k + 1) {
            let mut b = k + 1;
            while b + 1 < n && b - k < 2 && space_gap(b, b + 1) && lower_name(b + 1) {
                b += 1;
            }
            push(out, k + 1, b, 72);
            k = b + 1;
            continue;
        }
        if !words[k].cap && LOWER_AFTER.contains(&lw(k)) && k > 0 && space_gap(k - 1, k) && lower_name(k - 1) {
            let already = out[base..].last().is_some_and(|m| m.span.end > words[k - 1].s);
            let mut a = k - 1;
            while a > 0 && k - a < 2 && space_gap(a - 1, a) && lower_name(a - 1) {
                a -= 1;
            }
            if !already {
                push(out, a, k - 1, 72);
            }
        }

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
            } else if !strong && space_gap(k, k + 1) {
                // "Hesap sahibi Tülay Ünal", "sürücü Kadri Zeybekoğlu": iki noktasız
                // zayıf etiket yalnızca 2+ kelimelik büyük harfli zincirle.
                if let Some((a, b)) = chain(k + 1) {
                    if b > a {
                        push(out, a, b, 72);
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
                // Tek ad, cümle ortasında: "Taner kardeşim", "alıcısı: Ilgın".
                // Ardından hitap geliyorsa ("Ahmet Bey") kural 4'e bırakılır.
                let hitap_next = k + 1 < n && has(k + 1, F_SUFFIX | F_KIN);
                if !sentence_start(k) && !hitap_next && !date_before(k) && !place_suffix(k) {
                    push(out, a, a, 72);
                    k += 1;
                    continue;
                }
            }
        }

        // 3b) Bilinmeyen ad + sözlükteki soyad. "Nasrullah Kalender", "Berçem Akbulak"
        if words[k].cap && !has(k, F_FIRST) {
            if let Some((a, b)) = chain(k) {
                // "Bugün Kemal Sunal": ikinci kelime sözlükte bir adsa zinciri kural 3'e bırak.
                if b > a && !has(a + 1, F_FIRST) && (a + 1..=b).any(|j| has(j, F_SUR)) {
                    push(out, a, b, 72);
                    k = b + 1;
                    continue;
                }
            }
        }

        // 4) Hitap ← ad. "Ahmet Bey", "Ayşe Kaya Hanım", "Funda teyze"
        let kin_ok = |k: usize| !has(k, F_KIN) || !sentence_start(k - 1) || has(k - 1, F_FIRST);
        if has(k, F_SUFFIX | F_KIN) && k > 0 && space_gap(k - 1, k) && words[k - 1].cap && kin_ok(k) {
            let mut a = k - 1;
            while a > 0 && k - a < 2 && space_gap(a - 1, a) && nameable(a - 1) {
                a -= 1;
            }
            let already = out[base..].last().is_some_and(|m| m.span.end == words[k - 1].e);
            if nameable(k - 1) && !already {
                push(out, a, k - 1, 80);
            }
        }
        k += 1;
    }

    // 9) Metinde ad olarak bulunan kelimenin diğer geçişleri: "müşteri çağatay zengin
    // … teşekkürler çağatay bey", "Telli Hukuk Bürosu … Av. Tuba Telli".
    // Yalnızca güçlü bağlamla (unvan, hitap, tanıtma, form etiketi) bulunan adlar
    // yayılır; zayıf kuralların hataları çoğalmasın.
    let found: Vec<core::ops::Range<usize>> = out[base..].iter().map(|m| m.span.clone()).collect();
    let strong: Vec<core::ops::Range<usize>> =
        out[base..].iter().filter(|m| m.confidence >= 80).map(|m| m.span.clone()).collect();
    let covered = |k: usize| found.iter().any(|r| words[k].s < r.end && r.start < words[k].e);
    let mut known: Vec<&str> = (0..n)
        .filter(|&k| strong.iter().any(|r| r.start <= words[k].s && words[k].e <= r.end))
        .map(lw)
        .filter(|w| w.chars().count() >= 3)
        .collect();
    known.sort_unstable();
    known.dedup();
    let again = |k: usize| {
        !covered(k)
            && known.binary_search(&lw(k)).is_ok()
            && !has(k, F_STOP | F_TITLE | F_SUFFIX | F_KIN | F_ORG | F_STRONG | F_WEAK)
            && !LOWER_COMMON.contains(&lw(k))
            && !common(k)
            && !label_after(k)
            && !in_claimed(k)
            && !date_before(k)
            && !place_suffix(k)
            // "Demir kapı açıktı": cümle başında kelime olan soyadı yayılmaz.
            && !(has(k, F_SUR2) && after_punct(k))
    };
    let mut k = 0;
    while k < n {
        if again(k) {
            let mut b = k;
            while b + 1 < n && b - k < MAX_CHAIN - 1 && space_gap(b, b + 1) && again(b + 1) {
                b += 1;
            }
            push(out, k, b, 72);
            k = b;
        }
        k += 1;
    }
    drop_historical(text, &words, base, out);
}

/// "Fatih Sultan Mehmet Köprüsü", "Mimar Sinan'ın eserleri": eşleşme, çevresiyle birlikte
/// bilinen bir tarihî kişi adının parçasıysa kişisel veri değildir.
fn drop_historical(text: &str, words: &[Word], base: usize, out: &mut Vec<Match>) {
    let mut i = base;
    while i < out.len() {
        let span = out[i].span.clone();
        let first = words.partition_point(|w| w.e <= span.start);
        let last = words.partition_point(|w| w.s < span.end);
        let lo = words[first.saturating_sub(2)].s;
        let hi = words[(last + 1).min(words.len()) - 1].e;
        let window = crate::text::tr_lower(&text[lo..hi]);
        let own = crate::text::tr_lower(&text[span]);
        if HISTORICAL.iter().any(|h| h.contains(own.as_str()) && window.contains(h)) {
            out.remove(i);
        } else {
            i += 1;
        }
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
        for set in [
            TITLES, SUFFIXES, KIN, STRONG_LABELS, WEAK_LABELS, STOP, NOT_NAME_PREFIX, ORG, COMMON_WORD_SURNAMES,
            NAME_QUESTIONS, SPEAKERS, ANSWERERS,
        ] {
            for w in set {
                assert_eq!(crate::text::tr_lower(w), *w);
                assert!(w.len() <= MAX_LOWER, "{w}");
            }
        }
        for h in HISTORICAL {
            assert_eq!(crate::text::tr_lower(h), *h);
        }
        for set in [FIRST_NAMES, SURNAMES] {
            for w in lines(set) {
                assert_eq!(crate::text::tr_lower(w), w);
                assert!(w.len() <= MAX_LOWER, "{w}");
                assert!(w.chars().all(char::is_alphabetic), "{w}");
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
        // Cümle ortasında sözlükteki tek ad kişidir.
        assert_eq!(names("İSTANBUL'da Ahmet ile buluştuk."), ["Ahmet"]);
        // Cümle başında, tarihte (ay adı) ya da yer ekiyle tek ad maskelenmez.
        assert!(names("Ahmet geldi mi?").is_empty());
        assert!(names("Toplantı 6 Kasım tarihinde.").is_empty());
        assert!(names("Yol çalışması Fatih'te de var.").is_empty());
        assert!(names("Ceyhan'dan geçen kargo.").is_empty());
        assert_eq!(names("Bunu Kaan'a ilet."), ["Kaan"]);
        // Tanıtma, imza ve küçük harfli sözlük zinciri.
        assert_eq!(names("Merhaba, adım Evren Uslu. Siparişim"), ["Evren Uslu"]);
        assert_eq!(names("Ben Berçem Akbulak, geçen hafta"), ["Berçem Akbulak"]);
        assert!(names("Ben Ankara'dan yazıyorum").is_empty());
        assert_eq!(names("hoş geldiniz ben banu müşteri"), ["banu"]);
        assert_eq!(names("Bilgilerinize.\nKolay gelsin, Zübeyir"), ["Zübeyir"]);
        assert!(names("Selamlar, Kargomu Bursa aktarmadan bekliyor").is_empty());
        assert_eq!(names("abi naber izzet yekta bizim kat"), ["izzet yekta"]);
        // Sözlükte olmayan ad + sözlükteki soyad.
        assert_eq!(names("Yeni işe alınan Nasrullah Kalender için"), ["Nasrullah Kalender"]);
        assert_eq!(names("Bugün Kemal Sunal filmi var"), ["Kemal Sunal"]);
        // Şirket unvanı kişi adı değildir.
        assert!(names("Kiralama şirketi Çiğdem Mobilya istiyor.").is_empty());
        assert!(names("Şirketimiz Yılmaz İnşaat A.Ş. adına").is_empty());
    }

    #[test]
    fn behalf_common_surnames_and_propagation() {
        assert_eq!(names("faturalar Hilal Balta adına kesiliyor"), ["Hilal Balta"]);
        assert_eq!(names("Değerli ekip, Annem Banu Nasser adına başvuruyorum."), ["Banu Nasser"]);
        assert_eq!(names("kira kaan budak adına yatırıldı"), ["kaan budak"]);
        assert_eq!(names("müşteri temsilciniz şerife yıldız sizi arayacak"), ["şerife yıldız"]);
        assert_eq!(names("Telli Hukuk Bürosu adına Av. Tuba Telli katıldı."), ["Tuba Telli", "Telli"]);
        assert!(names("Ankara Büyükşehir Belediyesi adına konuştu. Türkiye Cumhuriyeti adına imzaladı.").is_empty());
        assert!(names("kendi şirketim adına başvurdum, annem adına da.").is_empty());
        assert!(names("ali kara bir kedi gördü").is_empty());
        assert_eq!(names("Sayın Ayşe Demir, gelemedi. Demir kapı açıktı."), ["Ayşe Demir"]);
    }

    #[test]
    fn transcript_answer_and_landmarks() {
        assert_eq!(
            names("temsilci adınız soyadınız müşteri oğuzhan kovalenko temsilci teşekkürler oğuzhan bey"),
            ["oğuzhan kovalenko", "oğuzhan"]
        );
        assert_eq!(names("temsilci adınızı tam olarak alabilir miyim müşteri su tuna temsilci"), ["su tuna"]);
        // Soru yoksa ya da yanıt cümle gibi uzarsa ad sayılmaz.
        assert!(names("temsilci nasıl yardımcı olabilirim müşteri kartım kayboldu temsilci").is_empty());
        assert!(names("temsilci adınız soyadınız müşteri bir saniye bakıyorum hemen söylüyorum").is_empty());
        assert!(names("temsilci adınızı alayım müşteri söylemek istemiyorum temsilci").is_empty());
        assert!(names("temsilci adınızı alayım müşteri temsilci soyadınızın yazılışı müşteri").is_empty());
        assert!(names("Fatih Sultan Mehmet Köprüsü trafiği yüzünden geciktim.").is_empty());
        assert_eq!(names("Etkinlik bitince Mimar Sinan'ın eserleri sergisi var."), Vec::<&str>::new());
        assert!(names("Sayın Meslektaşım, Sayın Savcılığa, Degerli Hocam,").is_empty());
    }

    #[test]
    fn no_false_positive_on_plain_text() {
        assert!(names("Deniz Kenarı çok güzeldi. Umut Etmek lazım.").is_empty());
    }

    #[test]
    fn addressee_words_are_not_names() {
        for s in ["Sayın İlgili, bilgilerinize.", "Sayın Müşterimiz, merhaba.", "Sayın Hâkimliğe, davacı",
                  "Değerli Hocam, merhaba", "Sayın Müdürüm, VPN", "Müşteri Hizmetleri Temsilcisi aradı"] {
            assert!(names(s).is_empty(), "{s}: {:?}", names(s));
        }
    }

    #[test]
    fn kinship_and_unlabeled_weak_labels() {
        assert_eq!(names("anahtarı Funda teyzeye bıraktım"), ["Funda"]); // sözlükteki ad
        assert_eq!(names("anahtarı Funda teyze aldı"), ["Funda"]);
        assert!(names("Komşu teyze aradı.").is_empty());
        assert_eq!(names("Hesap sahibi Tülay Ünal. Teşekkürler"), ["Tülay Ünal"]);
        assert_eq!(names("Tadilat ustası değil, usta Tahsin Sezgin geldi"), ["Tahsin Sezgin"]);
        assert!(names("Müşteri Hizmetleri aradı").is_empty());
    }

    #[test]
    fn lowercase_context() {
        assert_eq!(names("müşteri ee adım mizgîn soyadım özbek temsilci"), ["mizgîn", "özbek"]);
        assert_eq!(names("teşekkürler meltem tuna hanım doğum tarihiniz"), ["meltem tuna"]);
        assert_eq!(names("saygılarımla,\nav. aysun yıldız"), ["aysun yıldız"]);
        assert_eq!(names("kawa baysal isimli biri kapora istedi"), ["kawa baysal"]);
        assert_eq!(names("sayın hagop bedrosyan"), ["hagop bedrosyan"]);
        assert!(names("çok teşekkürler hanım, ürünün adı nedir").is_empty());
        assert!(names("etikette benim adım yerine başka ad, soyadım değişmiş").is_empty());
        assert!(names("iyi günler bey, sayın müşterimiz").is_empty());
    }
}
