//! Türkçe posta adresi tespiti. Model kullanmaz; adres yapısına bakar:
//!
//! 1. **Çapa**: `Mahallesi/Mah.`, `Caddesi/Cad.`, `Sokak/Sok./Sk.`, `Bulvarı/Blv.` ...
//!    Önünde büyük harfli bir ad ya da sayı olmalıdır (`Atatürk Cad.`, `1453 Sok.`).
//! 2. **Devam parçaları**: başka bir cadde/sokak, `No: 5/3`, `Kat 2`, `D: 4`, `B Blok`,
//!    posta kodu (`34710`), `İlçe/İl` veya il adı.
//!
//! Tek başına `Bağdat Caddesi` (mekân adı) düşük güven alır ve varsayılan eşikte
//! maskelenmez; ikinci bir parça (`No:`, ikinci çapa, il, posta kodu) ya da yakında
//! "adres" kelimesi varsa maskelenir. Türkçe ekler korunur: `Caddesi'nde` → `'nde` kalır.

use crate::entity::{Kind, Match};
use crate::text::context_has;

/// Kısaltma çapaları (ardından nokta gelebilir).
const ABBR: &[&str] = &["blv", "bulv", "cad", "cd", "mah", "mh", "sk", "sok", "sokk"];
/// Tam kelime çapaları.
const FULL: &[&str] = &[
    "bulvar", "bulvarı", "cadde", "caddesi", "mahalle", "mahallesi", "mevkii", "meydanı", "sokak", "sokağı",
];
/// Çapadan sonra gelebilen bina parçaları.
const BUILDING: &[&str] = &["apartmanı", "apt", "blok", "blk", "plaza", "rezidans", "sitesi"];
/// Numara etiketleri ve kat/daire.
const NUMBER_LABELS: &[&str] = &["d", "daire", "dış", "iç", "k", "kapı", "kat", "no", "numara"];
const ADDRESS_WORDS: &[&str] = &["adres", "address", "ikamet", "teslimat", "fatura adresi"];
const MAX_LEFT: usize = 3;
const MAX_CHAIN: usize = 3;

/// 81 il (katlanmış biçimde karşılaştırılır) ve yaygın kısa adlar.
const PROVINCES: &[&str] = &[
    "adana", "adıyaman", "afyon", "afyonkarahisar", "ağrı", "aksaray", "amasya", "ankara", "antalya",
    "ardahan", "artvin", "aydın", "balıkesir", "bartın", "batman", "bayburt", "bilecik", "bingöl", "bitlis",
    "bolu", "burdur", "bursa", "çanakkale", "çankırı", "çorum", "denizli", "diyarbakır", "düzce", "edirne",
    "elazığ", "erzincan", "erzurum", "eskişehir", "gaziantep", "giresun", "gümüşhane", "hakkari", "hatay",
    "ığdır", "ısparta", "istanbul", "izmir", "kahramanmaraş", "karabük", "karaman", "kars", "kastamonu",
    "kayseri", "kilis", "kırıkkale", "kırklareli", "kırşehir", "kocaeli", "konya", "kütahya", "malatya",
    "manisa", "maraş", "mardin", "mersin", "muğla", "muş", "nevşehir", "niğde", "ordu", "osmaniye", "rize",
    "sakarya", "samsun", "siirt", "sinop", "sivas", "şanlıurfa", "şırnak", "tekirdağ", "tokat", "trabzon",
    "tunceli", "urfa", "uşak", "van", "yalova", "yozgat", "zonguldak",
];

#[derive(Clone, Copy)]
struct Word {
    start: usize,
    end: usize,
    cap: bool,
    digit: bool,
}

fn tokenize(text: &str) -> Vec<Word> {
    let mut words = Vec::new();
    let mut cur: Option<Word> = None;
    for (i, c) in text.char_indices() {
        if c.is_alphanumeric() {
            match cur.as_mut() {
                Some(w) => w.end = i + c.len_utf8(),
                None => {
                    cur = Some(Word { start: i, end: i + c.len_utf8(), cap: c.is_uppercase(), digit: c.is_ascii_digit() })
                }
            }
        } else if let Some(w) = cur.take() {
            words.push(w);
        }
    }
    words.extend(cur);
    words
}

/// I/ı/İ/i ayrımını yok sayan, bellek ayırmayan karakter katlama.
#[inline]
fn fold_char(c: char) -> char {
    match c {
        'I' | 'İ' | 'ı' | 'i' => 'i',
        c if c.is_ascii() => c.to_ascii_lowercase(),
        c => c.to_lowercase().next().unwrap_or(c),
    }
}

fn eq_fold(a: &str, b: &str) -> bool {
    let mut ia = a.chars();
    b.chars().all(|k| ia.next().is_some_and(|c| fold_char(c) == fold_char(k))) && ia.next().is_none()
}

fn is_in(word: &str, list: &[&str]) -> bool {
    list.iter().any(|k| eq_fold(word, k))
}

fn is_province(word: &str) -> bool {
    is_in(word, PROVINCES)
}

/// Kelime çapa mı? Dönen değer, çapanın bittiği bayt (kısaltmada nokta dahil).
fn anchor_end(text: &str, w: Word) -> Option<usize> {
    let s = &text[w.start..w.end];
    // Tüm çapalar b/c/m/s ile başlar ve en fazla 9 karakterdir.
    if w.digit || s.len() > 20 || !matches!(s.as_bytes()[0] | 0x20, b'b' | b'c' | b'm' | b's') {
        return None;
    }
    if is_in(s, ABBR) {
        return Some(if text[w.end..].starts_with('.') { w.end + 1 } else { w.end });
    }
    is_in(s, FULL).then_some(w.end)
}

/// İki kelime arasındaki boşluk aynı adres içinde kalabilir mi?
fn soft_gap(g: &str) -> bool {
    g.len() <= 4 && !g.contains('\n') && g.chars().all(|c| matches!(c, ' ' | '\t' | ',' | '-' | '.' | ':'))
}

/// Ad parçası olabilecek kelime (büyük harfli veya kısa sayı, "19 Mayıs").
fn name_part(text: &str, w: Word) -> bool {
    (w.cap && !is_in(&text[w.start..w.end], NUMBER_LABELS)) || (w.digit && w.end - w.start <= 5)
}

/// Sokak adındaki unvan kısaltmaları (ardından ". " gelir).
const TITLE_ABBR: &[&str] = &["alb", "av", "doç", "dr", "gen", "op", "org", "prof", "yzb"];

/// Sola genişlemede ad parçası sayılmayan kelimeler.
const LEFT_STOP: &[&str] = &["adres", "adresi", "adresim", "adresimiz", "adresine", "ikamet", "teslimat"];

/// `i` cümle/satır başında mı (önünde yalnızca boşluk, ". ", ": ", "\n" ...)?
fn sentence_start(text: &str, i: usize) -> bool {
    let before = text[..i].trim_end_matches(' ');
    before.is_empty() || before.ends_with(['.', '!', '?', '\n', ':'])
}

/// `k`'deki çapanın önünde küçük harfli bir kelime ve çapadan sonra numara etiketi + sayı var mı?
fn lower_named_with_number(text: &str, w: &[Word], k: usize, pos: usize) -> bool {
    let p = w[k - 1];
    let lower = !p.cap && !p.digit && text[p.start..p.end].chars().all(char::is_alphabetic);
    let label = w.get(k + 1).is_some_and(|n| {
        pos <= n.start && soft_gap(&text[pos..n.start]) && is_in(&text[n.start..n.end], NUMBER_LABELS)
    });
    let num = w.get(k + 2).is_some_and(|d| d.digit);
    lower && &text[p.end..w[k].start] == " " && label && num && !is_in(&text[p.start..p.end], LEFT_STOP)
}

/// Aynı satır ve cümlede, adresin hemen önünde "adres/teslimat" gibi bir kelime var mı?
fn label_before(text: &str, start: usize) -> bool {
    let mut from = start.saturating_sub(40);
    while !text.is_char_boundary(from) {
        from += 1;
    }
    let ctx = &text[from..start];
    let cut = [ctx.rfind('\n').map(|i| i + 1), ctx.rfind(". ").map(|i| i + 2)].into_iter().flatten().max().unwrap_or(0);
    let ctx = &ctx[cut..];
    context_has(ctx, ctx.len(), 40, ADDRESS_WORDS)
}

/// Hızlı ret: metinde hiçbir çapa kökü yoksa tokenize etmeye gerek yok.
fn has_root(text: &str) -> bool {
    // Yalnızca kelime başlarına bakılır; "bul" gibi sıradan fiil kökleri bilerek yok.
    const ROOTS: &[&[u8]] = &[b"mah", b"mh", b"cad", b"cd", b"sok", b"sk", b"bulv", b"blv", b"mevk", b"meyd"];
    let b = text.as_bytes();
    b.iter().enumerate().any(|(i, &c)| {
        // Önceki bayt ASCII harf/rakam ya da çok baytlı bir harfin parçasıysa kelime ortasıdır.
        matches!(c | 0x20, b'b' | b'c' | b'm' | b's')
            && (i == 0 || (b[i - 1].is_ascii() && !b[i - 1].is_ascii_alphanumeric()))
            && ROOTS.iter().any(|r| b.get(i..i + r.len()).is_some_and(|w| w.eq_ignore_ascii_case(r)))
    })
}

pub(crate) fn scan_address(text: &str, out: &mut Vec<Match>) {
    if !has_root(text) {
        return;
    }
    let w = tokenize(text);
    let mut k = 0;
    while k < w.len() {
        let Some(mut pos) = anchor_end(text, w[k]) else {
            k += 1;
            continue;
        };
        // Sola doğru: büyük harfli kelimeler veya sayılar ("Atatürk", "19 Mayıs", "1453").
        let mut left = k;
        // Sayı adın başındadır ("19 Mayıs", "1453."): sayıdan sonra sola gidilmez.
        while left > 0 && k - left < MAX_LEFT && !(left < k && w[left].digit) {
            let p = w[left - 1];
            let gap = &text[p.end..w[left].start];
            let dotted_num = gap == ". " && p.digit; // "1453. Sokak"
            // Kişi adlı sokak: "Dr. Sadık Ahmet Cd.", "Av. Âdem Yavuz Sk."
            let dotted_title = gap == ". " && is_in(&text[p.start..p.end], TITLE_ABBR);
            if !(gap == " " || dotted_num || dotted_title) || !name_part(text, p) || is_in(&text[p.start..p.end], LEFT_STOP) {
                break;
            }
            left -= 1;
        }
        // Cümle başındaki büyük harf ad olduğunu göstermez ("Araç Portakal Cd.",
        // "Tebligatın Kemalpaşa Mahallesi"): ad en az iki kelimeyse ilki bırakılır.
        if k - left >= 2
            && !w[left].digit
            && !is_in(&text[w[left].start..w[left].end], TITLE_ABBR)
            && sentence_start(text, w[left].start)
        {
            left += 1;
        }
        // Küçük harfli tek kelimelik ad, ardından numara geliyorsa: "aydınlık sok. no:163/9".
        if left == k && k > 0 && lower_named_with_number(text, &w, k, pos) {
            left = k - 1;
        }
        if left == k {
            k += 1;
            continue;
        }
        let start = w[left].start;
        let mut anchors = 1;
        let mut strong = false;
        let mut next = k + 1;

        // Sağa doğru devam parçaları.
        while next < w.len() {
            let gap_from = pos.max(w[next - 1].end);
            if gap_from > w[next].start || !soft_gap(&text[gap_from..w[next].start]) {
                break;
            }
            let cur = w[next];
            let s = &text[cur.start..cur.end];

            // a) Numara/kat/daire: "No: 5/3", "Kat 2", "D:4", "İç Kapı No 7"
            if is_in(s, NUMBER_LABELS) {
                let mut m = next + 1;
                // "İç Kapı No", "Dış Kapı No" gibi zincirler.
                while m < w.len() && is_in(&text[w[m].start..w[m].end], NUMBER_LABELS) && m - next < 3 {
                    m += 1;
                }
                if m < w.len() && w[m].digit && soft_gap(&text[w[m - 1].end..w[m].start]) {
                    let mut end = w[m].end;
                    // "5/3", "12-B", "7A"
                    if m + 1 < w.len() && matches!(&text[end..w[m + 1].start], "/" | "-") {
                        m += 1;
                        end = w[m].end;
                    }
                    pos = end;
                    strong = true;
                    next = m + 1;
                    continue;
                }
                break;
            }

            // b) Posta kodu: 5 hane, il kodu 01-81.
            if cur.digit && s.len() == 5 && s.bytes().all(|b| b.is_ascii_digit()) {
                let il = (s.as_bytes()[0] - b'0') * 10 + (s.as_bytes()[1] - b'0');
                if (1..=81).contains(&il) {
                    pos = cur.end;
                    strong = true;
                    next += 1;
                    // Posta kodundan sonra gelen büyük harfli kelime ilçedir: "34710 Kadıköy".
                    if next < w.len() && w[next].cap && &text[pos..w[next].start] == " " {
                        pos = w[next].end;
                        next += 1;
                        // "34710 Kadıköy/İstanbul"
                        if next < w.len() && w[next].cap && text[pos..w[next].start].trim() == "/" {
                            pos = w[next].end;
                            next += 1;
                        }
                    }
                    continue;
                }
            }

            // c) İlçe/İl: "Kadıköy/İstanbul", "Kadıköy / İSTANBUL", tek başına il adı.
            if cur.cap && next + 1 < w.len() && text[cur.end..w[next + 1].start].trim() == "/" && w[next + 1].cap {
                pos = w[next + 1].end;
                strong = true;
                next += 2;
                continue;
            }
            // "Bornova İzmir", "Kadıköy, İstanbul"
            if cur.cap
                && next + 1 < w.len()
                && matches!(&text[cur.end..w[next + 1].start], " " | ", ")
                && is_province(&text[w[next + 1].start..w[next + 1].end])
            {
                pos = w[next + 1].end;
                strong = true;
                next += 2;
                continue;
            }
            if cur.cap && is_province(s) {
                pos = cur.end;
                strong = true;
                next += 1;
                continue;
            }

            // d) Bina: "Gül Apt.", "B Blok", "Deniz Sitesi"
            // e) Başka bir cadde/sokak: "Moda Cad.", "1453. Sok."
            let mut m = next;
            while m < w.len()
                && m - next < MAX_CHAIN
                && name_part(text, w[m])
                && anchor_end(text, w[m]).is_none()
                && !is_in(&text[w[m].start..w[m].end], BUILDING)
            {
                let gap = w.get(m + 1).map_or("", |nx| &text[w[m].end..nx.start]);
                let title = gap == ". " && is_in(&text[w[m].start..w[m].end], TITLE_ABBR);
                if !(gap == " " || (w[m].digit && gap == ". ") || title) {
                    break;
                }
                m += 1;
            }
            if m > next && m < w.len() {
                let t = &text[w[m].start..w[m].end];
                if let Some(e) = anchor_end(text, w[m]) {
                    pos = e;
                    anchors += 1;
                    next = m + 1;
                    continue;
                }
                if is_in(t, BUILDING) {
                    pos = if text[w[m].end..].starts_with('.') { w[m].end + 1 } else { w[m].end };
                    strong = true;
                    next = m + 1;
                    continue;
                }
            }
            break;
        }

        // `pos` yalnızca kısaltma noktasını (Mah., Apt.) içerir; cümle sonu noktası dışarıda kalır.
        let end = pos;
        let mut conf: u8 = if strong || anchors >= 2 { 85 } else { 55 };
        if label_before(text, start) {
            conf = conf.max(80);
        }
        out.push(Match::new(Kind::Address, start..end, conf));
        k = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<&str> {
        let mut v = Vec::new();
        scan_address(s, &mut v);
        v.iter().filter(|m| m.confidence >= 70).map(|m| m.text(s)).collect()
    }

    #[test]
    fn boundaries() {
        // Posta kodu + ilçe/il
        assert_eq!(run("Gökdere Mah., Bereket Sk. No:12 D:9, 22960 Keşan/Edirne."), ["Gökdere Mah., Bereket Sk. No:12 D:9, 22960 Keşan/Edirne"]);
        // Cümle başındaki ve etiket kelimesi sola alınmaz.
        assert_eq!(run("Araç Portakal Cd. No: 26 Kat:2 D:22 adresindeki otoparkta."), ["Portakal Cd. No: 26 Kat:2 D:22"]);
        assert_eq!(run("Adres Cevizli Mahallesi Serçe Cad. No:12 Kars"), ["Cevizli Mahallesi Serçe Cad. No:12 Kars"]);
        // Kişi adlı sokak unvanıyla birlikte.
        assert_eq!(run("Adres: Yeşilyurt Mah., Dr. Yağmur Kaya Cd. No:17 Artuklu/Mardin"), ["Yeşilyurt Mah., Dr. Yağmur Kaya Cd. No:17 Artuklu/Mardin"]);
        // Küçük harfli tek kelime + numara.
        assert_eq!(run("adres aydınlık sok. no:163/9 zil bozuk"), ["aydınlık sok. no:163/9"]);
        // Cümle başındaki tek kelimelik ad korunur.
        assert_eq!(run("Moda Cad. No:5 Kadıköy/İstanbul"), ["Moda Cad. No:5 Kadıköy/İstanbul"]);
    }

    #[test]
    fn full_addresses() {
        assert_eq!(
            run("Adres: Caferağa Mah. Moda Cad. No:12/3 Kadıköy/İstanbul, teşekkürler."),
            ["Caferağa Mah. Moda Cad. No:12/3 Kadıköy/İstanbul"]
        );
        assert_eq!(run("Atatürk Caddesi No 5 Kat 2 Daire 4'e gelin"), ["Atatürk Caddesi No 5 Kat 2 Daire 4"]);
        assert_eq!(run("Kargo 1453. Sokak No: 7 Bornova İzmir adresine"), ["1453. Sokak No: 7 Bornova İzmir"]);
        assert_eq!(run("19 Mayıs Mah. Gül Apt. D:3 34736 Kadıköy"), ["19 Mayıs Mah. Gül Apt. D:3 34736 Kadıköy"]);
        assert_eq!(run("Cumhuriyet Mahallesi Lale Sokak'ta oturuyor"), ["Cumhuriyet Mahallesi Lale Sokak"]);
    }

    #[test]
    fn context_label() {
        assert_eq!(run("Teslimat adresi: Moda Mah."), ["Moda Mah."]);
    }

    #[test]
    fn landmarks_and_plain_text_are_not_addresses() {
        assert!(run("Bağdat Caddesi'nde trafik var.").is_empty());
        // Önceki satırdaki "adres" kelimesi sonraki satırdaki mekân adını etkilemez.
        assert!(run("Kargo adresine gelmedi.\nBağdat Caddesi'nde trafik var.").is_empty());
        assert!(run("mahalle maçı, sokak hayvanları, ana cadde").is_empty());
        assert!(run("No: 5 numaralı sipariş, Kat planı").is_empty());
    }
}
