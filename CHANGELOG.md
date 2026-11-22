# Değişiklik günlüğü

Bu dosya [Keep a Changelog](https://keepachangelog.com/tr/1.1.0/) biçimini ve
[Semantic Versioning](https://semver.org/lang/tr/) kurallarını izler.

## [Unreleased]

## [0.4.0] - 2026-11-22

turkce-pii-bench test bölümünde maskeleme isabeti
%58,1'den (0.3.1) %86,2'ye, sızan varlık oranı %14,1'e geldi (makro F1 90,7).

### Değiştirildi

- Tespit 0.3.1'e göre yaklaşık 2,2 kat yavaş (aynı makinede 5,9 → 13,0 µs/mesaj):
  büyük ad sözlükleri, ad yayılım geçişi ve yeni dedektörler. Geliştirme sırasında
  27,7 µs'e çıkan süre, çıktı değişmeden yarıya indirildi: ad kuralı listeleri tek
  sözlük aramasına taşındı, kelime başına denetimler bir kez hesaplanıyor, sık Türkçe
  harfler Unicode tablosuna gitmeden işleniyor, yazıyla sayı ve tarifli adres
  taramaları kelime başına bellek ayırmıyor, pasaport ve doğum etiketi aramalarında
  ucuz ön eleme var.

### Eklendi

- TROY kartları: 9792 ile başlayan, Luhn'u geçerli 16 haneli numaralar kart sayılır
  (Türkiye'nin yerli kart ağı; önceden yalnızca 3-6 ile başlayan ağlar tanınıyordu).
- Yazıyla okunan kart numaraları: dört haneli gruplarla söylenen numara ("dokuz bin
  yedi yüz doksan iki iki bin üç yüz kırk bir ...") Luhn ve ağ önekiyle kart olarak
  bulunur. "bin"den önceki "bir" önceki grubun birler hanesidir (1xxx "bin" diye
  söylenir); okuma tektir.
- Gruplu yazılmış TCKN ("957 883 959 36"): 0 ile başlamayan, gruplar 2-4 haneli,
  sağlaması geçerli 11 hane.
- Doğum tarihi sağ bağlamı: "20.02.2021 tarihinde doğdu", "2020-12-13 doğdu".

### Değiştirildi (tespit)

- Rol e-posta hesapları (`info@`, `destek@`, `satis@`, `musteri.hizmetleri@` ...) bir
  kişiye ait olmadığı için güven 60 (varsayılan eşik 70'in altında): maskelenmez,
  `min_confidence(60)` ile istenirse maskelenir.
- Sağında "sürüm/versiyon/version" geçen noktalı dörtlü ("3.6.0.84 sürümünden") IP
  değil sürüm numarası sayılır (güven 50), solundaki gibi.
- Ad ve soyad sözlükleri (`src/detect/veri/`): 151 elle yazılmış adın yerine
  1.699 ad ve 3.558 soyadı. Kaynak Wikidata (CC0): Türk vatandaşı kişilerin
  verilen ad (P735) ve soyadı (P734) değerleri, en az iki kişide geçenler.
  Yaygın kelime ya da il adı olanlar (Deniz, Ekim, Kaya, Demir, Bursa ...)
  bilerek dışarıda.
- Yeni ad kuralları: cümle ortasında tek başına sözlükteki ad ("Taner
  kardeşim"; tarihteki ay adı "6 Kasım" ve yer eki alan "Fatih'te" hariç);
  bilinmeyen ad + sözlükteki soyad ("Nasrullah Kalender"); kendini tanıtma
  ("adım Evren Uslu", "Ben Berçem Akbulak", "ben banu"); imza ("Kolay gelsin,
  Zübeyir"); küçük harfli sözlük zinciri ("nimet özdemir").
- Şirket unvanı denetimi: zincirin içinde ya da ardında `Mobilya`, `İnşaat`,
  `Ltd`, `A.Ş.` gibi bir unvan kelimesi varsa kişi adı sayılmaz ("Çiğdem
  Mobilya", "Yılmaz İnşaat A.Ş.").
- Ad tespiti: `usta` unvanı; `teyze`, `abla`, `abi`, `amca` gibi yakınlık
  hitapları ("Funda teyze"; cümle başındaki "Komşu teyze" eşleşmez); iki
  noktasız zayıf etiketler ("Hesap sahibi Tülay Ünal", "sürücü Kadri
  Zeybekoğlu", en az iki kelimeyle); yeni zayıf etiketler: `davacı`, `davalı`,
  `gönderici`, `müvekkil`, `sigortalı`, `sürücü`.
- Küçük harfli metinde (transkript, mesajlaşma) bağlamla ad tespiti, sözlük
  gerektirmez: önde `adım`, `soyadım`, `ismim`, `av.`, `sayın`, `müvekkil`
  ("adım mizgîn özbek"); arkada `hanım`, `bey`, `isimli`, `adlı`, `adında`
  ("meltem tuna hanım"). En fazla iki kelime alınır; yaygın kelimeler ve
  çekimli fiiller ("adım yerine", "soyadım değişmiş") elenir.
- Plaka: tireli (`26-KL-469`), bitişik küçük harfli (`63rua42`) ve plaka
  bağlamında boşluklu küçük harfli (`46 fp 540`) yazımlar.
- Pasaport: iki harfli önek (`KF2129874`) ve küçük harf (`hj2212636`).
- SGK sicil no: numaradan sonra gelen bağlam ("84883037 numaralı SGK sicil").
- Doğum tarihi: tarihten sonra gelen bağlam ("12.03.1990 doğumlu"); öndeki
  bağlam penceresi 40'tan 60 bayta çıktı.
- Satıra bölünmüş IBAN ve rakam dizileri ("0543 974⏎83 76").
- Gayriresmî (tarifli) adresler: "Bostanlı'da, iskelenin arkasındaki sokakta,
  3. kat", "okulun arka tarafındaki müstakil evde, Kemeraltı tarafında". Sabit
  ifade listesi yerine yapıya bakılır: `-ki` almış konum sözcüğü + konut
  sözcüğü; soldaki semt (`X'da,`) ve sağdaki kat/kapı/`X tarafında` parçaları
  eklenir. Semt, ek parça ya da konut bağlamı yoksa varsayılan eşikte
  maskelenmez ("parkın karşısındaki binada toplantı").
- Yazıyla söylenmiş telefon ve TC kimlik numaraları ("sıfır beş yüz otuz beş
  iki yüz seksen üç on yedi yetmiş altı"); çağrı merkezi transkriptleri ve
  sesli asistan çıktıları için. Türkçe ve ASCII yazım desteklenir. Belirsiz
  gruplamalarda ("doksan beş yüz yetmiş") tüm okumalar denenir; TCKN sağlaması
  tutan okuma seçilir. En az 10 hane gerekir, sıradan metindeki sayılar
  eşleşmez.
- SGK sicil no için yeni bağlam ifadeleri: "sosyal güvenlik", "sigortalı
  sicil/no", "sigortalılık", "emekli sandığı", "tahsis no", "işyeri sicil".
  Hemen solunda SGK etiketi olan bitişik numara telefon ya da kart yerine SGK
  sayılır ("SGK sicil no: 5371451377").
- Tek başına doğum yılı ("1959 doğumlu", "doğum yılım 1998") ve yılsız doğum
  günü ("doğum günüm 7 Nisan"), yalnızca etikete bitişikse.
- Ad tespiti: ardından `adına` gelen iki-üç kelimelik ad ("faturalar Hilal
  Balta adına", "kaan budak adına"); sözlükteki addan sonra kelime olan yaygın
  soyadları ("nazım yıldız", "bircan kaya"); güçlü bağlamla bulunan bir adın
  metindeki diğer geçişleri ("Av. Tuba Telli" → "Telli Hukuk Bürosu").
  `Belediyesi`, `Cumhuriyeti`, `Derneği`, `Üniversitesi` gibi kurum kelimeleri
  zinciri kişi adı olmaktan çıkarır.
- Çağrı dökümlerinde ad soran ifadeden ("adınız soyadınız", "adınızı
  alabilir miyim") sonra müşterinin verdiği yanıt ad sayılır; yanıtta en az
  bir sözlük kelimesi gerekir.

### Düzeltildi

- "Sayın İlgili", "Sayın Müşterimiz", "Sayın Hâkimliğe", "Değerli Hocam",
  "Müşteri Hizmetleri" gibi hitap ve kurum kelimeleri artık ad sanılmıyor.
- Gelecekteki tarihler doğum tarihi olarak işaretlenmiyor ("teslimat
  19.03.2027"). wasm hedefinde saat olmadığı için bu denetim yapılmaz.
- Doğum etiketi yalnızca kendi tarihini kapsıyor: "doğum tarihim 19.02.1983,
  sipariş tarihim 02.11.2025" ya da "… 07.07.2020 tarihinde işe başladım"
  cümlelerindeki ikinci tarih ve "tahmini doğum tarihi" artık doğum tarihi
  sayılmıyor.
- Büyük "İ" ile başlayan bağlam kelimeleri ("İkamet adresi", "İşyeri sicil")
  hiç eşleşmiyordu.
- Metnin ilerisinde başka bir eşleşme (telefon, IBAN) varsa küçük harfli ad
  ("barış fidan isimli") atlanıyordu.
- "444 1234" gibi çağrı merkezi numaraları SGK sicil no sayılmıyor.
- "Sayın Meslektaşım", "Sayın Savcılığa", "Sayın Tedarikçimiz", "Degerli Hocam"
  artık ad sanılmıyor. "… Köprüsü", "… Lisesi", "… Camii" gibi yapı adları ve
  "Fatih Sultan Mehmet", "Mimar Sinan" gibi tarihî kişiler de kişi adı sayılmıyor.

## [0.3.1] - 2026-11-18

### Eklendi

- Yeni Python tekerlekleri: Linux aarch64 (ARM sunucular), Alpine/musl Linux
  (x86_64 ve aarch64) ve Intel macOS. Bu platformlarda kurulum artık Rust
  derleyicisi gerektirmez.
- Yayın otomasyonu: etiket ile sürümler uyuşmazsa yayın durur; tekerlekler
  çalıştırılabilen platformlarda kurulup denenir; PyPI'den sonra crates.io da
  token'sız (trusted publishing) yayımlanır; GitHub Release notları
  CHANGELOG'dan oluşturulur.

### Değiştirildi

- PyPI ve crates.io sayfaları Türkçe README gösterir.
- crates.io kategorileri: `text-processing`, `security`,
  `command-line-utilities`, `wasm` (`privacy` crates.io'da yok).

## [0.3.0] - 2026-11-18

### Eklendi

- `kalkan.messages`: OpenAI biçimli mesaj listelerini (çok parçalı içerik, araç
  çağrısı argümanları dahil) maskeleme/geri yükleme; bağımlılıksız.
- `kalkan.integrations.litellm`: `completion`/`acompletion` sarmalayıcıları ve
  LiteLLM Proxy için `KalkanGuardrail` (akış dahil; kasa metadata'ya yazılmaz).
- `kalkan.integrations.langchain`: her Runnable'ı saran `with_kalkan`
  (`invoke`/`ainvoke`/`stream`/`astream`, araç çağrıları, `on_leak` denetimi).
- Python ekstraları: `kalkan[litellm]`, `kalkan[langchain]`.
- `Kind::Address` (`ADRES`): mahalle/cadde/sokak/bulvar çapalı Türkçe posta adresi
  tespiti; `No:`, kat/daire, blok, posta kodu, `İlçe/İl` ve 81 il adı. Tek başına
  cadde adı (mekân) maskelenmez. Kısmi modda il/ilçe kalır, sentetik modda sahte adres.
- `Kind::SocialSecurity` (`SGK`): SGK/SSK/Bağ-Kur sicil numarası, yalnızca bağlamla.

### Değiştirildi

- **Kırıcı:** `Kind` artık `#[non_exhaustive]`; `Kind::ALL` 14 elemanlı. Bundan
  sonraki türler küçük sürümlerde kırılma olmadan eklenebilir.
- Yeni dedektörler tespit süresini ölçüm derleminde yaklaşık %14 artırır.

### Eklendi (proje altyapısı)

- İngilizce README (`README.md`); Türkçe belge `README.tr.md` olarak taşındı.
  crates.io ve PyPI sayfaları artık İngilizce.
- `CODE_OF_CONDUCT.md`, issue şablonları (tespit hatası, hata, özellik), PR şablonu
  ve Dependabot ayarı.
- CI: MSRV (1.82) işi, macOS/Windows çekirdek testleri, Python 3.9 ve 3.13 matrisi.
- CLI için uçtan uca testler (maskele → kasa → geri yükle, JSON, denetim çıkış kodu);
  sürüm testi artık `Cargo.toml` sürümünü okur.

### Düzeltildi

- FFI tutamaçları serbest bırakılan yuvaları yeniden kullanır; uzun süreli
  süreçlerde tutamaç listesi büyümez.
- `kalkan_free` ayırma boyutunu tampon başlığından okur; yanlış `len` geçirilmesi
  artık bellek bozulmasına yol açmaz.
- JS sarmalayıcısı eksik WASM dışa aktarımlarında anlaşılır hata verir.
- Demo sayfasındaki harici Google Fonts bağlantıları kaldırıldı; sayfa artık
  tamamen çevrimdışı ve tek dosyalık.

## [0.2.0]

### Eklendi

- 12 veri türü: TCKN, VKN, IBAN, telefon, kart, e-posta, plaka, ad, IP, doğum
  tarihi, pasaport ve kuruma özel terimler.
- Sağlama doğrulamaları (TCKN/VKN algoritmaları, IBAN mod-97, kart Luhn).
- Oturum kasası, akış geri yükleme, sızıntı denetimi ve sentetik değer üretimi.
- Rust, CLI, Python (PyO3) ve WASM/C ABI arayüzleri; tek dosyalık tarayıcı demosu.
- Apache-2.0 lisans metni ve yayın/katkı/güvenlik dokümanları.

### Güvenlik

- Çekirdek `unsafe` içermez; `unsafe` yalnızca `ffi` özelliğiyle açılır.
- Kart sentetiğinde olası `usize` alt akışı giderildi.
