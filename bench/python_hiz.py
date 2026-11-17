"""Python içinden hız: kalkan (Rust uzantısı) ile kvkk-pii (regex katmanı).

    python3 bench/gen_corpus.py corpus.txt 5
    pip install kalkan kvkk-pii
    python3 bench/python_hiz.py corpus.txt
"""
import os
import sys
import time

import kalkan


def olc(ad, fn, mesajlar, mb):
    fn(mesajlar[:200])  # ısınma
    en_iyi = min(_sure(fn, mesajlar) for _ in range(3))
    print(f"  {ad:<34} {mb / en_iyi:8.1f} MB/s  {en_iyi * 1e6 / len(mesajlar):8.1f} µs/mesaj")
    return en_iyi


def _sure(fn, mesajlar):
    t = time.perf_counter()
    fn(mesajlar)
    return time.perf_counter() - t


def main():
    mesajlar = [m for m in open(sys.argv[1], encoding="utf-8").read().split("\n\n") if m]
    mb = sum(len(m.encode()) for m in mesajlar) / 2**20
    print(f"{mb:.2f} MB, {len(mesajlar)} mesaj, {os.cpu_count()} çekirdek\n")

    k = kalkan.Kalkan()
    tek = olc("kalkan: döngüde mask()", lambda ms: [k.mask(m) for m in ms], mesajlar, mb)
    olc("kalkan: mask_batch() (tüm çekirdek)", k.mask_batch, mesajlar, mb)

    try:
        from kvkk_pii import PiiDetector
        from kvkk_pii.layers.regex_layer import DEFAULT_RECOGNIZERS
        from kvkk_pii.recognizers.kisi_adi import KisiAdiRecognizer
    except ImportError:
        print("  (kvkk-pii kurulu değil, karşılaştırma atlandı)")
        return
    det = PiiDetector(layers=["regex"], recognizers=DEFAULT_RECOGNIZERS + [KisiAdiRecognizer()],
                      download_policy="never")

    def kvkk(ms):
        out = []
        for m in ms:
            s = det.create_session(m)
            out.append(s.mask())
        return out

    rakip = olc("kvkk-pii (regex): döngüde mask()", kvkk, mesajlar, mb)
    print(f"\n  tek çekirdekte fark: {rakip / tek:.1f}x")


if __name__ == "__main__":
    main()
