import threading

import pytest

import kalkan

BELGE = (
    "Sayın Ayşe Kaya, TC 10000000146, tel 0532 123 45 67, "
    "IBAN TR33 0006 1005 1978 6457 8413 26, e-posta ayse@ornek.com, "
    "IP 192.168.1.10, doğum tarihi 05.03.1990."
)


def test_detect_char_offsets_with_turkish_text():
    metin = "Çağrı İşlemi: ğüşıöç 10000000146 ✓ 0532 123 45 67"
    ents = kalkan.Kalkan().detect(metin)
    assert [e.kind for e in ents] == ["TCKN", "TELEFON"]
    for e in ents:
        # Konumlar karakter indeksi: doğrudan dilimlenebilir.
        assert metin[e.start:e.end] == e.text
    assert ents[0].to_dict()["confidence"] == 99


def test_session_roundtrip_and_consistency():
    s = kalkan.Kalkan().session()
    maskeli = s.mask(BELGE)
    for gercek in ["Ayşe Kaya", "10000000146", "0532 123 45 67", "ayse@ornek.com", "192.168.1.10", "05.03.1990"]:
        assert gercek not in maskeli
    assert s.restore(maskeli) == BELGE
    # Aynı değer aynı token'ı alır.
    assert s.mask("Tekrar: +905321234567") == "Tekrar: [TELEFON_1]"
    assert len(s) == 7


def test_synthetic_values_pass_validation():
    k = kalkan.Kalkan(strategy="synthetic", secret=7)
    s = k.session()
    maskeli, ents = s.mask_detailed(BELGE)
    tekrar = {e.kind: e.text for e in kalkan.Kalkan().detect(maskeli)}
    assert kalkan.is_valid_tckn(tekrar["TCKN"]) and tekrar["TCKN"] != "10000000146"
    assert kalkan.is_valid_iban(tekrar["IBAN"])
    assert s.restore(maskeli) == BELGE
    # Deterministik: aynı anahtar → aynı sahte değerler.
    assert k.session().mask(BELGE) == maskeli


def test_partial_and_label():
    assert kalkan.Kalkan(strategy="partial").mask("TC 10000000146") == "TC *********46"
    assert kalkan.Kalkan(strategy="label").mask("TC 10000000146") == "TC [TCKN]"


def test_address_and_sgk():
    s = kalkan.Kalkan().session()
    t = "Adres: Moda Cad. No:12 Kadıköy/İstanbul, SGK sicil no: 3401234567"
    m = s.mask(t)
    assert m == "Adres: [ADRES_1], SGK sicil no: [SGK_1]"
    assert s.restore(m) == t
    assert {"ADRES", "SGK"} <= set(kalkan.KINDS)


def test_kinds_exclude_terms():
    assert kalkan.Kalkan(kinds=["EPOSTA"]).mask("a@b.com 10000000146") == "[EPOSTA_1] 10000000146"
    assert kalkan.Kalkan(exclude=["TCKN"]).mask("10000000146") == "10000000146"
    k = kalkan.Kalkan(terms=["Proje Atlas", "Acme Holding"])
    assert k.mask("ACME HOLDING'in PROJE ATLAS bütçesi") == "[OZEL_1]'in [OZEL_2] bütçesi"


def test_errors():
    with pytest.raises(ValueError, match="strateji"):
        kalkan.Kalkan(strategy="yok")
    with pytest.raises(ValueError, match="tür"):
        kalkan.Kalkan(kinds=["YOK"])


def test_stream_every_split_point():
    s = kalkan.Kalkan().session()
    s.mask(BELGE)
    cevap = "[AD_SOYAD_1] için [TCKN_1] ve [IBAN_1] doğrulandı; [IP_1] engellendi."
    beklenen = s.restore(cevap)
    for kesim in range(len(cevap) + 1):
        st = s.stream()
        assert st.push(cevap[:kesim]) + st.push(cevap[kesim:]) + st.finish() == beklenen


def test_audit():
    s = kalkan.Kalkan().session()
    s.mask("TC 10000000146")
    assert s.audit("[TCKN_1] tamam")
    r = s.audit("[TCKN_9] ve 0212 444 55 66")
    assert not r and not r.clean
    assert r.unknown_placeholders == ["[TCKN_9]"]
    assert r.new_pii == [("TELEFON", "0212 444 55 66")]


def test_vault_export_import_between_processes():
    k = kalkan.Kalkan()
    a = k.session()
    maskeli = a.mask(BELGE)
    kasa = dict(a.vault)  # ör. Redis'e yazılır
    b = k.session()
    b.import_vault(kasa)
    assert b.restore(maskeli) == BELGE


def test_batch_matches_sequential_and_is_threadsafe():
    k = kalkan.Kalkan()
    metinler = [f"Müşteri {i}: TC 10000000146, tel 0532 123 {i % 100:02d} 67" for i in range(2000)]
    paralel = k.mask_batch(metinler)
    assert paralel == [k.mask(t) for t in metinler]
    assert k.mask_batch(metinler, threads=1) == paralel
    assert [len(x) for x in k.detect_batch(metinler[:100])] == [2] * 100

    # Aynı Kalkan nesnesi Python iş parçacıklarından güvenle kullanılabilir.
    hatalar = []

    def isci():
        try:
            for t in metinler[:200]:
                assert k.mask(t).startswith("Müşteri")
        except Exception as e:  # pragma: no cover
            hatalar.append(e)

    ths = [threading.Thread(target=isci) for _ in range(8)]
    for t in ths:
        t.start()
    for t in ths:
        t.join()
    assert not hatalar


def test_validators():
    assert kalkan.is_valid_tckn("10000000146") and not kalkan.is_valid_tckn("10000000147")
    assert kalkan.is_valid_iban("TR33 0006 1005 1978 6457 8413 26")
    assert not kalkan.is_valid_iban("TR33 0006 1005 1978 6457 8413 27")
    assert kalkan.luhn_valid("4111 1111 1111 1111")
    # Çok baytlı karakterle başlayan girdi istisna fırlatmamalı.
    assert not kalkan.is_valid_iban("ÇR33000610051978645784132")
    assert not kalkan.is_valid_tckn("1000000014ş")


def test_docstring_example():
    import doctest

    assert doctest.testmod(kalkan).failed == 0
