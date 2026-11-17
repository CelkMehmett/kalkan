"""kalkan ile kvkk-pii'yi (yalnızca regex katmanı, model yok) aynı derlemde
hız ve doğruluk açısından karşılaştırır.

    python3 bench/gen_corpus.py corpus.txt 5
    cargo run --release --example bench -- corpus.txt kalkan.jsonl
    pip install kvkk-pii
    python3 bench/karsilastir.py corpus.txt kalkan.jsonl

Puanlama yalnızca kalkan'ın 8 türü üzerinden yapılır (tarih, IP gibi diğer
türler iki taraf için de yok sayılır). Eşleşme: aynı tür + metinlerden biri
diğerini içeriyor (unvan dahil/hariç farkları cezalandırılmasın diye).
"""
import json
import statistics
import sys
import time
from collections import Counter

ESLEME = {
    "TC_KIMLIK": "TCKN", "VKN": "VKN", "IBAN_TR": "IBAN", "TELEFON_TR": "TELEFON",
    "EMAIL": "EPOSTA", "KREDI_KARTI": "KART", "PLAKA_TR": "PLAKA", "KISI_ADI": "AD_SOYAD",
    "PERSON": "AD_SOYAD",
}
TURLER = {"TCKN", "VKN", "IBAN", "TELEFON", "EPOSTA", "KART", "PLAKA", "AD_SOYAD"}


def norm(t):
    return " ".join(t.split())


def puanla(ad, tahminler, gold):
    """Esnek eşleşme: aynı tür ve metinlerden biri diğerini içeriyorsa doğru sayılır
    (ör. rakip "Sayın Ayşe Kaya" der, etiket "Ayşe Kaya" ise eşleşir)."""
    tp = Counter(); fp = Counter(); fn = Counter()
    for tah, dog in zip(tahminler, gold):
        kalan = [(k, norm(v)) for k, v in dog]
        for k, v in ((k, norm(v)) for k, v in tah if k in TURLER):
            j = next((i for i, (gk, gv) in enumerate(kalan) if gk == k and (gv in v or v in gv)), None)
            if j is None:
                fp[k] += 1
            else:
                tp[k] += 1
                kalan.pop(j)
        for k, _ in kalan:
            fn[k] += 1
    print(f"\n{ad}")
    print(f"  {'tür':<9} {'isabet':>8} {'kesinlik':>9}   (bulunan/beklenen)")
    for t in sorted(TURLER):
        if tp[t] + fn[t] + fp[t] == 0:
            continue
        r = tp[t] / max(1, tp[t] + fn[t]); p = tp[t] / max(1, tp[t] + fp[t])
        print(f"  {t:<9} {r:>8.1%} {p:>9.1%}   ({tp[t]}/{tp[t] + fn[t]}, {fp[t]} yanlış)")
    T, F, N = sum(tp.values()), sum(fp.values()), sum(fn.values())
    print(f"  {'TOPLAM':<9} {T / (T + N):>8.1%} {T / max(1, T + F):>9.1%}   ({T}/{T + N}, {F} yanlış)")


def main():
    derlem, kalkan_jsonl = sys.argv[1], sys.argv[2]
    mesajlar = [m for m in open(derlem, encoding="utf-8").read().split("\n\n") if m]
    gold = [json.loads(l) for l in open(derlem + ".gold.jsonl", encoding="utf-8")]
    kalkan = [json.loads(l) for l in open(kalkan_jsonl, encoding="utf-8")]
    assert len(mesajlar) == len(gold) == len(kalkan), (len(mesajlar), len(gold), len(kalkan))
    mb = sum(len(m.encode()) for m in mesajlar) / 2**20

    from kvkk_pii import PiiDetector
    from kvkk_pii.layers.regex_layer import DEFAULT_RECOGNIZERS
    from kvkk_pii.recognizers.kisi_adi import KisiAdiRecognizer

    det = PiiDetector(layers=["regex"], recognizers=DEFAULT_RECOGNIZERS + [KisiAdiRecognizer()],
                      download_policy="never")
    det.analyze(mesajlar[0])  # ısınma

    sureler = []
    for _ in range(3):
        t = time.perf_counter()
        sonuc = [det.analyze(m) for m in mesajlar]
        sureler.append(time.perf_counter() - t)
    d = statistics.median(sureler)
    tah = [[(ESLEME.get(e.entity_type, e.entity_type), e.text) for e in r.entities] for r in sonuc]

    t = time.perf_counter()
    for m in mesajlar:
        s = det.create_session(m)
        s.restore(s.mask())
    r = time.perf_counter() - t

    print(f"kvkk-pii (regex katmanı): {mb:.2f} MB, {len(mesajlar)} mesaj")
    print(f"  tespit          : {mb / d:8.2f} MB/s  {len(mesajlar) / d:9.0f} mesaj/s  {d * 1e6 / len(mesajlar):8.1f} µs/mesaj")
    print(f"  maskele+geri yük: {mb / r:8.2f} MB/s  {len(mesajlar) / r:9.0f} mesaj/s  {r * 1e6 / len(mesajlar):8.1f} µs/mesaj")

    puanla("kalkan", kalkan, gold)
    puanla("kvkk-pii (regex katmanı)", tah, gold)


if __name__ == "__main__":
    main()
