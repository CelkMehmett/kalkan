# Contributing / Katkı rehberi

> **English summary.** Contributions are welcome in English or Turkish. Before opening a PR run:
>
> ```bash
> cargo test --workspace --exclude kalkan-py
> cargo test --features ffi --lib
> cargo clippy --workspace --all-targets --features ffi -- -D warnings
> ```
>
> plus the Python/JS tests if you touched the bindings (commands below). Rules: the core in
> `src/` stays **dependency-free** and `#![forbid(unsafe_code)]` (`unsafe` only in `src/ffi.rs`
> behind the `ffi` feature); new detectors go in `src/detect/` and must keep spans on UTF-8
> boundaries; behaviour changes come with tests in `tests/uctan_uca.rs`; offsets are bytes in
> Rust, characters in Python and UTF-16 units in JS. **Never put real personal data** in code,
> tests, issues or logs. Add an entry under `Unreleased` in `CHANGELOG.md`. See also
> [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## Geliştirme ortamı

- Rust ≥ 1.82 (önerilen: güncel stable), `cargo`
- Python ≥ 3.9 + `maturin` + `pytest` (Python bağlantısı için)
- `bun` (JS sarmalayıcı ve demo testleri için)
- İsteğe bağlı: `wasm-opt` (WASM çıktısını küçültmek için)
- İsteğe bağlı: `wasm32-unknown-unknown` hedefi (`wasm/build.sh` ekler)

## Test ve kontroller

```bash
cargo test --workspace --exclude kalkan-py
cargo test --features ffi --lib
cargo clippy --workspace --all-targets --features ffi -- -D warnings
cargo doc --no-deps --all-features

cd python && maturin develop && pytest
cargo build --release --features ffi && cd wasm && bun install && bun test
```

`rustdoc` kütüphaneleri eksikse `cargo test --lib --tests` doctest'leri atlar;
CI ortamında tüm testler çalışır. Mevcut kaynak özel satır genişliğiyle
biçimlendirilmiştir; `cargo fmt` kullanıyorsanız yalnızca dokunduğunuz dosyaları
dönüştürün ve stil farkını commit'e eklemeyin.

## Kod kuralları

- Çekirdek (`src/`) **sıfır bağımlılık** ve `#![forbid(unsafe_code)]` altında kalmalı.
  `unsafe` yalnızca `src/ffi.rs` içinde ve `ffi` özelliği açıkken yazılabilir.
- Yeni tespit kuralları `src/detect/` altına gider; `Match.span` her zaman UTF-8
  karakter sınırında olmalı. Sağlama doğrulaması olan türler yüksek güven alır.
- Konum birimleri katmanlar arasında farklıdır: Rust bayt, Python karakter,
  WASM/JS UTF-16. Yeni bir API eklerken bunu belgeleyin.
- Ortak davranış değişiklikleri `tests/uctan_uca.rs` ve ilgili dil testleriyle
  birlikte gelmeli; fuzz/akış/kasa değişmezlerini koruyun.
- Kasa (`Session`) gerçek kişisel veri tutar: hata mesajlarına ve loglara ham
  değer yazmayın.

## Commit ve PR

- Commit mesajı kısa ve imperatif olsun: `detect: RF-tekil eşleşmeleri birleştir`.
- PR açıklamasında ne değiştiği, hangi testlerin eklendiği ve geriye dönük
  uyumluluk etkisi yazın.
- Yeni davranış ekliyorsanız README ve doctest'leri de güncelleyin.
- Onay öncesi yukarıdaki tüm kontroller yeşil olmalı.

## Yayın öncesi kontrol listesi

1. Sürümü üç yerde güncelleyin: kök `Cargo.toml`, `python/Cargo.toml`,
   `python/pyproject.toml`.
2. `CHANGELOG.md` içine sürüm maddesini ekleyin; `Unreleased` bölümünü boşaltın.
3. `cargo package --list` ile paket içeriğini gözden geçirin; lisans dosyalarının
   dahil olduğundan emin olun.
4. `cargo publish --dry-run` çalıştırın.
5. `maturin build --release -m python/Cargo.toml -o dist` ile tekerleği doğrulayın.
6. Etiket atın (`vX.Y.Z`); `wheels.yml` PyPI'ye, `demo.yml` GitHub Pages'e yayınlar.
7. `Cargo.toml` içindeki `documentation` alanı ve depo bağlantısı güncel olsun.

## Lisans

Katkılar MIT OR Apache-2.0 altında kabul edilir. Ayrıntılar için
[LICENSE-MIT](LICENSE-MIT) ve [LICENSE-APACHE](LICENSE-APACHE) dosyalarına bakın.
