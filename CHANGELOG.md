# Değişiklik günlüğü

Bu dosya [Keep a Changelog](https://keepachangelog.com/tr/1.1.0/) biçimini ve
[Semantic Versioning](https://semver.org/lang/tr/) kurallarını izler.

## [Unreleased]

## [0.3.0] - 2026-11-18

### Eklendi

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
