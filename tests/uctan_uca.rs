use kalkan::checksum::{is_valid_tckn, is_valid_tr_iban, luhn_valid, tckn_check_digits};
use kalkan::{Kalkan, Kind, Strategy};

fn found(text: &str) -> Vec<(Kind, String)> {
    Kalkan::new().detect(text).iter().map(|m| (m.kind, m.text(text).to_string())).collect()
}

fn kinds_of(text: &str) -> Vec<Kind> {
    found(text).into_iter().map(|(k, _)| k).collect()
}

// ---------------------------------------------------------------- tespit

#[test]
fn phone_formats() {
    for p in [
        "+90 532 123 45 67",
        "+905321234567",
        "0090 532 123 4567",
        "0532 123 45 67",
        "(0532) 123-45-67",
        "05321234567",
        "0212 345 67 89",
        "0850 222 33 44",
        "532.123.45.67",
    ] {
        let text = format!("Beni {p} numarasından arayın.");
        assert_eq!(found(&text), vec![(Kind::Phone, p.to_string())], "{p}");
    }
}

#[test]
fn adjacent_numbers_are_split() {
    let t = "Bilgiler: 10000000146 05321234567 4111 1111 1111 1111";
    assert_eq!(
        found(t),
        vec![
            (Kind::Tckn, "10000000146".into()),
            (Kind::Phone, "05321234567".into()),
            (Kind::Card, "4111 1111 1111 1111".into()),
        ]
    );
}

#[test]
fn vkn_needs_context() {
    // 1234567890 sağlamayı geçen bir VKN değil; geçerli bir tane üretelim.
    let mut d = b"123456789".to_vec();
    d.push(kalkan::checksum::vkn_check_digit(&d));
    let vkn = String::from_utf8(d).unwrap();
    assert_eq!(kinds_of(&format!("Firmanın vergi no: {vkn}")), [Kind::Vkn]);
    assert_eq!(kinds_of(&format!("VERGİ KİMLİK {vkn}")), [Kind::Vkn]);
    assert!(kinds_of(&format!("Sipariş kodu {vkn}")).is_empty());
}

#[test]
fn mixed_document() {
    let t = "Adı Soyadı: AYŞE KAYA\n\
             T.C. Kimlik: 10000000146\n\
             E-posta: ayse.kaya@ornek.com.tr\n\
             IBAN: TR33 0006 1005 1978 6457 8413 26\n\
             Araç: 34 ABC 123\n\
             Not: Mehmet Ali Yılmaz'ın kartı 5555 5555 5555 4444.";
    assert_eq!(
        kinds_of(t),
        [Kind::Name, Kind::Tckn, Kind::Email, Kind::Iban, Kind::Plate, Kind::Name, Kind::Card]
    );
}

#[test]
fn no_false_positives_on_everyday_numbers() {
    for t in [
        "Sipariş no 123456, tarih 12.03.2024, fiyat 1.250,00 TL.",
        "2024 yılında 81 ilde 3.500 kişi katıldı.",
        "Saat 14:30'da 3. kattaki 12 TL 500 kuruşluk ürün.",
        "Pi sayısı 3.14159265358979 olarak alınır.",
        "Ürün kodu ABC-12345678901-X",
        "Versiyon 10.0.19045.3803 yüklendi.",
        "Deniz Kenarı'nda Umut Etmek güzel.",
        "Toplam 99 ABC 123 adet",
    ] {
        assert!(found(t).is_empty(), "yanlış pozitif: {t:?} → {:?}", found(t));
    }
}

#[test]
fn random_tckns_always_found() {
    let mut x: u64 = 0x1234_5678;
    for _ in 0..2000 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let mut d: Vec<u8> = format!("{:09}", x % 900_000_000 + 100_000_000).into_bytes();
        let (a, b) = tckn_check_digits(&d);
        d.extend([a, b]);
        if !is_valid_tckn(&d) {
            continue;
        }
        let s = String::from_utf8(d).unwrap();
        let t = format!("Hasta ({s}) için rapor.");
        assert_eq!(found(&t), vec![(Kind::Tckn, s)]);
    }
}

#[test]
fn unicode_safety() {
    // Çok baytlı karakterler arasında panik yok, aralıklar karakter sınırında.
    let t = "Çağrı 📞 05321234567 — İĞÜŞÖÇ ışık ✓ 10000000146ğ";
    for m in Kalkan::new().detect(t) {
        assert!(t.is_char_boundary(m.span.start) && t.is_char_boundary(m.span.end));
    }
    assert_eq!(kinds_of(t), [Kind::Phone]); // sonuncusu harfe yapışık
}

// ---------------------------------------------------------------- maskeleme

#[test]
fn same_value_same_token_across_formats() {
    let mut s = Kalkan::new().session();
    let out = s.mask("Tel: 0532 123 45 67. Tekrar: +905321234567. Diğer: 0533 000 00 00");
    assert_eq!(out, "Tel: [TELEFON_1]. Tekrar: [TELEFON_1]. Diğer: [TELEFON_2]");
    assert_eq!(s.len(), 2);
}

#[test]
fn turkish_suffix_preserved() {
    let mut s = Kalkan::new().session();
    assert_eq!(s.mask("Dosyayı Ahmet Yılmaz'a ver."), "Dosyayı [AD_SOYAD_1]'a ver.");
}

#[test]
fn synthetic_values_are_valid_and_reversible() {
    let k = Kalkan::builder().strategy(Strategy::Synthetic).secret(42).build();
    let original = "TC 10000000146, IBAN TR33 0006 1005 1978 6457 8413 26, \
                    kart 4111 1111 1111 1111, tel 0532 123 45 67, plaka 34 ABC 123, \
                    e-posta ali@ornek.com, Sayın Ayşe Kaya";
    let mut s = k.session();
    let (masked, ms) = s.mask_detailed(original);
    assert_eq!(ms.len(), 7);

    // Sahte değerler aynı biçimde ve yeniden tespit ediliyor → sağlamaları geçerli.
    let again = Kalkan::new().detect(&masked);
    let kinds: Vec<Kind> = again.iter().map(|m| m.kind).collect();
    assert_eq!(kinds, ms.iter().map(|m| m.kind).collect::<Vec<_>>(), "{masked}");
    for m in &again {
        let txt = m.text(&masked);
        let digits: Vec<u8> = txt.bytes().filter(u8::is_ascii_digit).collect();
        match m.kind {
            Kind::Tckn => assert!(is_valid_tckn(&digits)),
            Kind::Iban => assert!(is_valid_tr_iban(&digits)),
            Kind::Card => assert!(luhn_valid(&digits)),
            _ => {}
        }
        // Hiçbir gerçek değer sızmıyor.
        assert!(!original.contains(txt), "sızıntı: {txt}");
    }
    // Biçim korunuyor (boşluklar aynı yerde).
    assert!(masked.contains("IBAN TR"));
    assert_eq!(s.restore(&masked), original);

    // Aynı anahtar → aynı sahte değer (deterministik).
    assert_eq!(k.session().mask(original), masked);
}

#[test]
fn partial_and_label() {
    let p = Kalkan::builder().strategy(Strategy::Partial).build();
    assert_eq!(p.mask("TC 10000000146"), "TC *********46");
    assert_eq!(p.mask("Kart 4111 1111 1111 1111"), "Kart **** **** **** 1111");
    assert_eq!(p.mask("ali.veli@ornek.com"), "a***@ornek.com");
    assert_eq!(p.mask("Sayın Ayşe Kaya"), "Sayın A*** K***");
    assert_eq!(p.mask("34 ABC 123"), "34 *** ***");

    let l = Kalkan::builder().strategy(Strategy::Label).build();
    assert_eq!(l.mask("TC 10000000146 ve 10000000146"), "TC [TCKN] ve [TCKN]");
}

#[test]
fn kind_filter() {
    let k = Kalkan::builder().kinds([Kind::Email]).build();
    assert_eq!(k.mask("ali@ornek.com 10000000146"), "[EPOSTA_1] 10000000146");
}

// ---------------------------------------------------------------- akış

#[test]
fn stream_restore_every_split_point() {
    let mut s = Kalkan::new().session();
    let _ = s.mask("Sayın Ayşe Kaya, TC 10000000146, tel 0532 123 45 67.");
    let reply = "Merhaba [AD_SOYAD_1]! [TCKN_1] için [TELEFON_1] doğrulandı. [TCKN_1] tamam [YOK_9]";
    let expected = s.restore(reply);
    assert!(expected.contains("Ayşe Kaya") && expected.contains("[YOK_9]"));

    for cut in 0..=reply.len() {
        if !reply.is_char_boundary(cut) {
            continue;
        }
        let mut st = s.stream();
        let mut got = st.push(&reply[..cut]);
        got += &st.push(&reply[cut..]);
        got += &st.finish();
        assert_eq!(got, expected, "kesim noktası {cut}");
    }
}

#[test]
fn stream_restore_token_by_token() {
    let mut s = Kalkan::builder().strategy(Strategy::Synthetic).build().session();
    let masked = s.mask("Müşteri Ayşe Kaya Hanım, TC 10000000146.");
    // LLM'in token token akıttığını taklit et: 1-4 karakterlik parçalar.
    let chars: Vec<char> = masked.chars().collect();
    let mut st = s.stream();
    let mut got = String::new();
    let mut i = 0;
    let mut step = 1;
    while i < chars.len() {
        let j = (i + step).min(chars.len());
        got += &st.push(&chars[i..j].iter().collect::<String>());
        i = j;
        step = step % 4 + 1;
    }
    got += &st.finish();
    assert_eq!(got, "Müşteri Ayşe Kaya Hanım, TC 10000000146.");
}

#[test]
fn stream_does_not_hold_plain_text() {
    let mut s = Kalkan::new().session();
    let _ = s.mask("TC 10000000146");
    let mut st = s.stream();
    // '[' bir token başlangıcı olabilir → bekletilir; düz metin hemen akar.
    assert_eq!(st.push("Merhaba dünya "), "Merhaba dünya ");
    assert_eq!(st.push("[TC"), "");
    assert_eq!(st.push("KN_1] ok"), "10000000146 ok");
}

#[test]
fn stream_synthetic_every_split_and_chunk_size() {
    // Gerileme testi: sahte TCKN "...0" ile biterken sahte telefon "0..." ile başlıyor;
    // akış, biten anahtarın kuyruğunu yeni bir anahtarın başı sanıp kesmemeli.
    let k = Kalkan::builder().strategy(Strategy::Synthetic).build();
    let original = "Ben Ayşe Kaya, TC 10000000146. İadeyi TR33 0006 1005 1978 6457 8413 26 \
                    hesabıma yapın, telefonum 0532 123 45 67.";
    let mut s = k.session();
    let masked = s.mask(original);
    let reply = format!("(Yankı: {masked}) {masked}");
    let expected = s.restore(&reply);
    assert_eq!(expected, format!("(Yankı: {original}) {original}"));

    let chars: Vec<char> = reply.chars().collect();
    for size in 1..=12 {
        let mut st = s.stream();
        let mut got = String::new();
        for c in chars.chunks(size) {
            got += &st.push(&c.iter().collect::<String>());
        }
        got += &st.finish();
        assert_eq!(got, expected, "parça boyu {size}");
    }
    for cut in (0..=reply.len()).filter(|&c| reply.is_char_boundary(c)) {
        let mut st = s.stream();
        let got = st.push(&reply[..cut]) + &st.push(&reply[cut..]) + &st.finish();
        assert_eq!(got, expected, "kesim {cut}");
    }
}

#[test]
fn fuzz_roundtrip_and_invariants() {
    // Ucuz, bağımlılıksız "fuzz": rakam, ayraç, Türkçe harf ve özel karakter karışımı.
    const PARCALAR: &[&str] = &[
        "0", "1", "4", "5", "9", "05", "90", "+", "(", ")", "-", ".", " ", "  ", "/", "@", ":", "'", "\n",
        "TR", "tr", "Ayşe", "Kaya", "Sayın", "Bey", "Hanım", "Ad Soyad:", "İ", "ı", "ğ", "Ş", "📞", "é",
        "10000000146", "0532 123 45 67", "4111 1111 1111 1111", "ali@ornek.com", "34 ABC 123", "vergi",
        "TR33 0006 1005 1978 6457 8413 26", "Mehmet", "Yılmaz", "ABC", "x", "Dr.",
        "192.168.1.10", "2001:db8::1", "::", "doğum", "05.03.1990", "5 Mart 1990", "Pasaport", "U12345678",
        "Acme", "ACME", "acmeli", "sürüm 1.2.3.4",
    ];
    let k = Kalkan::builder().terms(["Acme"]).build();
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut rnd = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    for _ in 0..20_000 {
        let n = (rnd() % 24) as usize;
        let text: String = (0..n).map(|_| PARCALAR[(rnd() % PARCALAR.len() as u64) as usize]).collect();
        let ms = k.detect(&text);
        for w in ms.windows(2) {
            assert!(w[0].span.end <= w[1].span.start, "çakışma: {text:?}");
        }
        for m in &ms {
            assert!(text.is_char_boundary(m.span.start) && text.is_char_boundary(m.span.end));
            assert!(!m.span.is_empty());
        }
        let mut s = k.session();
        let masked = s.mask(&text);
        let back = s.restore(&masked);
        if back != text {
            // Tek izin verilen fark: aynı değerin farklı yazımları (telefon biçimi, büyük/küçük
            // harf) bilerek aynı token'ı alır ve ilk görülen yazımla geri yüklenir.
            let key = |m: &kalkan::Match| {
                let t = m.text(&text);
                if m.kind == Kind::Phone {
                    let d: String = t.chars().filter(char::is_ascii_digit).collect();
                    d[d.len().saturating_sub(10)..].to_string()
                } else {
                    t.chars().filter(|c| !c.is_whitespace()).flat_map(char::to_lowercase).map(|c| if c == 'ı' { 'i' } else { c }).filter(|&c| c != '\u{307}').collect()
                }
            };
            let mut seen = std::collections::HashMap::new();
            let mut variant = false;
            for m in &ms {
                let t = m.text(&text);
                if let Some(prev) = seen.insert((m.kind, key(m)), t) {
                    variant |= prev != t;
                }
            }
            assert!(variant, "gidiş-dönüş bozuldu: {text:?} → {masked:?} → {back:?}");
        }
    }
}

// ---------------------------------------------------------------- yeni türler

#[test]
fn new_kinds_in_document() {
    let t = "Kullanıcı 192.168.1.10 ve 2001:db8:85a3::8a2e:370:7334 adreslerinden girdi.\n\
             Doğum tarihi: 05.03.1990, pasaport no U12345678.\n\
             Sürüm 1.2.3.4 yüklendi; fatura tarihi 05.03.2024.";
    assert_eq!(
        found(t),
        vec![
            (Kind::Ip, "192.168.1.10".into()),
            (Kind::Ip, "2001:db8:85a3::8a2e:370:7334".into()),
            (Kind::BirthDate, "05.03.1990".into()),
            (Kind::Passport, "U12345678".into()),
        ]
    );
}

#[test]
fn custom_terms() {
    let k = Kalkan::builder().terms(["Proje Atlas", "Acme Holding"]).build();
    let mut s = k.session();
    let m = s.mask("ACME HOLDING'in Proje Atlas bütçesi; acme holdingimiz değil.");
    assert_eq!(m, "[OZEL_1]'in [OZEL_2] bütçesi; acme holdingimiz değil.");
    assert_eq!(s.restore("[OZEL_2] onaylandı"), "Proje Atlas onaylandı");

    // Sentetik modda özel terimler de token alır (gerçekçi sahtesi olmaz).
    let k = Kalkan::builder().strategy(Strategy::Synthetic).terms(["Proje Atlas"]).build();
    assert_eq!(k.mask("Proje Atlas"), "[OZEL_1]");
}

#[test]
fn synthetic_new_kinds_are_safe_and_reversible() {
    let k = Kalkan::builder().strategy(Strategy::Synthetic).build();
    let original = "IP 192.168.1.10, v6 2001:db8:85a3::8a2e:370:7334, doğum tarihi 5 Mart 1990, \
                    doğum: 07/11/1985, pasaport U12345678";
    let mut s = k.session();
    let (masked, ms) = s.mask_detailed(original);
    assert_eq!(ms.len(), 5, "{masked}");
    for v in ["192.168.1.10", "8a2e", "5 Mart 1990", "07/11/1985", "U12345678"] {
        assert!(!masked.contains(v), "sızıntı {v}: {masked}");
    }
    // Sahte IP'ler belgeleme bloklarından: gerçek bir cihaza ait olamaz.
    assert!(["192.0.2.", "198.51.100.", "203.0.113."].iter().any(|p| masked.contains(p)), "{masked}");
    assert!(masked.contains("2001:db8:"));
    // Biçim korunur ve yeniden tespit edilir (geçerli tarih).
    let again: Vec<Kind> = Kalkan::new().detect(&masked).iter().map(|m| m.kind).collect();
    assert_eq!(again, [Kind::Ip, Kind::Ip, Kind::BirthDate, Kind::BirthDate, Kind::Passport], "{masked}");
    assert_eq!(s.restore(&masked), original);
}

#[test]
fn partial_new_kinds() {
    let p = Kalkan::builder().strategy(Strategy::Partial).build();
    assert_eq!(p.mask("IP 192.168.1.10"), "IP 192.168.*.*");
    assert_eq!(p.mask("doğum: 05.03.1990"), "doğum: **.**.1990");
    assert_eq!(p.mask("doğum 5 Mart 1990"), "doğum * **** 1990");
    assert_eq!(p.mask("pasaport U12345678"), "pasaport U******78");
}

// ---------------------------------------------------------------- denetim

#[test]
fn audit_finds_new_pii_echoes_and_fake_tokens() {
    let mut s = Kalkan::new().session();
    let _ = s.mask("Ayşe Kaya Hanım, TC 10000000146, tel 0532 123 45 67");

    assert!(s.audit("[AD_SOYAD_1] için [TCKN_1] doğrulandı, [TELEFON_1] aranacak.").is_clean());

    let r = s.audit(
        "[AD_SOYAD_1] için [TCKN_3] kaydı açıldı. Destek: 0212 444 55 66. \
         Ayrıca 10000000146 numarası da görüldü. [BILINMEYEN_1]",
    );
    assert_eq!(r.unknown_placeholders, ["[TCKN_3]"]); // etiketi bilinmeyen [BILINMEYEN_1] sayılmaz
    assert_eq!(r.new_pii, vec![(Kind::Phone, "0212 444 55 66".to_string())]);
    assert_eq!(r.echoed_originals, ["10000000146"]);
    assert!(!r.is_clean());
}

#[test]
fn audit_ignores_our_synthetic_values() {
    let mut s = Kalkan::builder().strategy(Strategy::Synthetic).build().session();
    let masked = s.mask("TC 10000000146, IBAN TR33 0006 1005 1978 6457 8413 26");
    assert!(s.audit(&masked).is_clean(), "{:?}", s.audit(&masked));
}

#[test]
fn stream_restorer_is_independent_of_session() {
    // Akış geri yükleyici oturumdan bağımsız: başka bir iş parçacığına taşınabilir.
    let mut s = Kalkan::new().session();
    let _ = s.mask("TC 10000000146");
    let mut st = s.stream();
    drop(s);
    let h = std::thread::spawn(move || {
        let mut out = st.push("Kayıt [TC");
        out += &st.push("KN_1] tamam");
        out + &st.finish()
    });
    assert_eq!(h.join().unwrap(), "Kayıt 10000000146 tamam");
}

// ---------------------------------------------------------------- adres ve SGK

#[test]
fn address_and_social_security() {
    let t = "Teslimat adresi: Caferağa Mah. Moda Cad. No:12/3 Kadıköy/İstanbul. \
             SGK sicil no: 3401234567, SGK no: 10000000146. Bağdat Caddesi'nde trafik var.";
    assert_eq!(
        found(t),
        vec![
            (Kind::Address, "Caferağa Mah. Moda Cad. No:12/3 Kadıköy/İstanbul".into()),
            (Kind::SocialSecurity, "3401234567".into()),
            // SGK numarası TCKN ile aynıysa TCKN olarak etiketlenir (sağlaması tutar).
            (Kind::Tckn, "10000000146".into()),
        ]
    );

    let mut s = Kalkan::new().session();
    let m = s.mask(t);
    assert!(m.starts_with("Teslimat adresi: [ADRES_1]. SGK sicil no: [SGK_1], SGK no: [TCKN_1]."), "{m}");
    assert!(m.ends_with("Bağdat Caddesi'nde trafik var."), "{m}");
    assert_eq!(s.restore(&m), t);

    // Akışta yer tutucu bölünse bile geri yüklenir.
    let mut st = s.stream();
    let out = st.push("Kargo [ADR") + &st.push("ES_1] adresine çıktı.") + &st.finish();
    assert_eq!(out, "Kargo Caferağa Mah. Moda Cad. No:12/3 Kadıköy/İstanbul adresine çıktı.");
}

#[test]
fn address_strategies() {
    let t = "Adres: Kızılay Mah. Atatürk Blv. No:101/7 Çankaya/Ankara";
    let partial = Kalkan::builder().strategy(Strategy::Partial).build().mask(t);
    assert_eq!(partial, "Adres: *** Çankaya/Ankara");

    let mut s = Kalkan::builder().strategy(Strategy::Synthetic).build().session();
    let masked = s.mask(t);
    assert!(!masked.contains("Kızılay") && !masked.contains("101/7"), "{masked}");
    assert!(s.audit(&masked).is_clean(), "{:?}", s.audit(&masked));
    assert_eq!(s.restore(&masked), t);

    assert!(Kalkan::builder().kinds([Kind::Tckn]).build().detect(t).is_empty());
}
