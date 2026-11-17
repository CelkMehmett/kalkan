# Güvenlik politikası

kalkan kişisel veri işleyen bir araçtır. Güvenlik açığı şüphesi varsa bunu
herkese açık bir issue olarak **paylaşmayın**.

## Bildirim

- Depo sahibine özel bir kanal (GitHub private vulnerability report veya e-posta)
  ile ulaşın.
- İçerikte: etkilenen sürüm, platform (Rust/Python/WASM), en küçük yeniden
  üretilebilir örnek ve olası etki yazın. Gerçek kişisel veri kullanmayın.
- İlk yanıt hedefi 7 gündür; onaylanan açıklar için düzeltilmiş sürüm
  yayınlanana kadar ayrıntıları gizli tutun.

## Kapsam

- Tespit atlamaları (gerçek TCKN/IBAN/kart vb.'nin maskelenmemesi)
- Kasa/yer tutucu karışması nedeniyle yanlış geri yükleme
- UTF-8/UTF-16 sınırı, akış (streaming) veya FFI bellek sözleşmesi kaynaklı
  panik, bellek bozulması ve veri sızıntısı
- Sentetik değerlerin gerçek değer hakkında bilgi taşıması

Kapsam dışı: kalkan'ın kapsamadığı veri türleri (adres, sağlık verisi, SGK no),
sezgisel ad tespitinin kaçırdığı isimler ve kullanıcının kasa dosyasını güvensiz
bir yerde saklaması. Bunlar README "Sınırlamalar" ve "Güvenlik modeli"
bölümlerinde açıklanır.

## Sorumlu kullanım

- `secret` değerini gizli tutun; varsayılan sentetik anahtar yalnızca
  gösterim amaçlıdır.
- Kasa dosyalarını erişim kısıtı olan dizinlerde saklayın ve işiniz bitince silin.
- `audit` bir uyarı mekanizmasıdır; otomatik karar vermeden önce sonucu değerlendirin.
