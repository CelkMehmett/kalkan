## What / Ne değişti?

## Tests / Testler

- [ ] `cargo test --workspace --exclude kalkan-py`
- [ ] `cargo clippy --workspace --all-targets --features ffi -- -D warnings`
- [ ] Python / JS tests if bindings changed / Bağlantılar değiştiyse Python / JS testleri
- [ ] New behaviour covered in `tests/uctan_uca.rs` / Yeni davranış uçtan uca testte

## Compatibility / Uyumluluk

<!-- Breaking change? Offset units (bytes / chars / UTF-16) affected? / Geriye dönük kırılma var mı? -->

- [ ] `CHANGELOG.md` updated (Unreleased) / güncellendi
- [ ] No real personal data in code, tests or this PR / Kodda, testlerde ve bu PR'da gerçek kişisel veri yok
