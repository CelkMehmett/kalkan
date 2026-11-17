"""Benchmark derlemi üretir: müşteri destek mesajlarına benzeyen, içinde geçerli
TCKN, IBAN, kart, telefon, e-posta, plaka ve ad-soyad geçen Türkçe metinler.
Mesajlar boş satırla ayrılır; her mesajın doğru etiketleri
<çıktı>.gold.jsonl dosyasına yazılır. Deterministiktir (sabit tohum).

Kullanım: python3 gen_corpus.py <çıktı.txt> [MB]
"""
import random
import sys

R = random.Random(2026)
ADLAR = ["Ahmet", "Mehmet", "Ayşe", "Fatma", "Zeynep", "Mustafa", "Elif", "Emre", "Merve", "Hakan"]
SOYADLAR = ["Yılmaz", "Kaya", "Demir", "Şahin", "Çelik", "Yıldız", "Aydın", "Öztürk", "Arslan", "Doğan"]
DOLGU = [
    "Siparişim üç gündür kargoda bekliyor, durumu hakkında bilgi alabilir miyim?",
    "Uygulamaya giriş yaparken sürekli hata alıyorum, şifremi de sıfırladım.",
    "Geçen ay yapılan 2.450,00 TL tutarındaki ödemenin iadesini talep ediyorum.",
    "Faturamda 14.03.2025 tarihli işlem iki kez görünüyor.",
    "Ürün kodu 48213 olan siparişte beden hatası var, değişim istiyorum.",
    "Toplantı saat 14:30'da 3. kattaki salonda yapılacak.",
    "Kampanya kapsamında 12 ay taksit seçeneği hâlâ geçerli mi?",
    "İstanbul şubesine 2024 yılında açılan hesabım hakkında bilgi almak istiyorum.",
]


def tckn():
    d = [R.randint(1, 9)] + [R.randint(0, 9) for _ in range(8)]
    d10 = ((d[0] + d[2] + d[4] + d[6] + d[8]) * 7 - (d[1] + d[3] + d[5] + d[7])) % 10
    d11 = (sum(d) + d10) % 10
    return "".join(map(str, d + [d10, d11]))


def iban():
    bban = "".join(str(R.randint(0, 9)) for _ in range(5)) + "0" + "".join(str(R.randint(0, 9)) for _ in range(16))
    check = 98 - int(bban + "292700") % 97
    s = f"TR{check:02d}{bban}"
    return " ".join(s[i:i + 4] for i in range(0, len(s), 4)) if R.random() < 0.7 else s


def kart():
    body = [4] + [R.randint(0, 9) for _ in range(14)]
    total = 0
    for i, v in enumerate(reversed(body)):
        if i % 2 == 0:
            v *= 2
            v -= 9 if v > 9 else 0
        total += v
    s = "".join(map(str, body + [(10 - total % 10) % 10]))
    return " ".join(s[i:i + 4] for i in range(0, 16, 4))


def tel():
    n = f"5{R.randint(30, 59)}{R.randint(1000000, 9999999)}"
    return R.choice([f"0{n[:3]} {n[3:6]} {n[6:8]} {n[8:]}", f"+90 {n[:3]} {n[3:6]} {n[6:8]} {n[8:]}", f"0{n}"])


def ad():
    return f"{R.choice(ADLAR)} {R.choice(SOYADLAR)}"


def plaka():
    return f"{R.randint(1, 81):02d} {''.join(R.choice('ABCDEFGHJKLMNPRSTUVYZ') for _ in range(3))} {R.randint(10, 999)}"


def _satir(sablon, *parcalar):
    """Satırı ve içindeki (tür, metin) etiketlerini döndürür."""
    degerler = [p() for _, p in parcalar]
    return sablon.format(*degerler), [(t, v) for (t, _), v in zip(parcalar, degerler)]


def _eposta():
    return f"{R.choice(['ali', 'ayse.k', 'm.demir'])}{R.randint(1, 999)}@ornek.com.tr"


PII_SATIRLARI = [
    lambda: _satir("Merhaba, ben {}. T.C. kimlik numaram {}.", ("AD_SOYAD", ad), ("TCKN", tckn)),
    lambda: _satir("Bana {} numarasından ulaşabilirsiniz.", ("TELEFON", tel)),
    lambda: _satir("İade için IBAN: {} hesabını kullanın.", ("IBAN", iban)),
    lambda: _satir("Kartım {} ile yapılan işlem iptal edilsin.", ("KART", kart)),
    lambda: _satir("E-posta adresim {} olarak güncellensin.", ("EPOSTA", _eposta)),
    lambda: _satir("Sayın {}, {} plakalı aracınızın randevusu onaylandı.", ("AD_SOYAD", ad), ("PLAKA", plaka)),
]


def mesaj():
    satirlar, etiketler = [], []
    for _ in range(R.randint(4, 9)):
        if R.random() < 0.35:
            satir, et = R.choice(PII_SATIRLARI)()
            satirlar.append(satir)
            etiketler.extend(et)
        else:
            satirlar.append(R.choice(DOLGU))
    return "\n".join(satirlar), etiketler


def main():
    import json
    out = sys.argv[1]
    hedef = float(sys.argv[2] if len(sys.argv) > 2 else 5) * 1024 * 1024
    boyut = 0
    with open(out, "w", encoding="utf-8") as f, open(out + ".gold.jsonl", "w", encoding="utf-8") as g:
        while boyut < hedef:
            metin, etiketler = mesaj()
            m = metin + "\n\n"
            f.write(m)
            g.write(json.dumps(etiketler, ensure_ascii=False) + "\n")
            boyut += len(m.encode("utf-8"))


if __name__ == "__main__":
    main()
