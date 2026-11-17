# Değişiklik günlüğü

Bu dosya [Keep a Changelog](https://keepachangelog.com/tr/1.1.0/) biçimini ve
[Semantic Versioning](https://semver.org/lang/tr/) kurallarını izler.

## [Unreleased]

### Eklendi

- CLI için uçtan uca testler (maskele → kasa → geri yükle, JSON, denetim çıkış kodu).

### Değiştirildi

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
