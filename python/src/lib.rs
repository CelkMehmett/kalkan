//! kalkan Python bağlantısı (PyO3).
//!
//! Konumlar (start/end) Python'a **karakter** indeksi olarak verilir; böylece
//! `text[e.start:e.end]` doğrudan çalışır.

use std::collections::HashMap;

use ::kalkan as core;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

fn parse_strategy(s: &str) -> PyResult<core::Strategy> {
    Ok(match s.to_lowercase().as_str() {
        "token" => core::Strategy::Token,
        "synthetic" | "sentetik" => core::Strategy::Synthetic,
        "partial" | "kismi" | "kısmi" => core::Strategy::Partial,
        "label" | "etiket" => core::Strategy::Label,
        other => {
            return Err(PyValueError::new_err(format!(
                "bilinmeyen strateji: {other:?} (token | synthetic | partial | label)"
            )))
        }
    })
}

/// Bayt aralıklarını karakter aralıklarına çevirir (eşleşmeler sıralı; tek geçiş).
fn char_spans(text: &str, ms: &[core::Match]) -> Vec<(usize, usize)> {
    let mut out = Vec::with_capacity(ms.len());
    let (mut byte, mut chars) = (0usize, 0usize);
    let mut advance = |to: usize| {
        chars += text[byte..to].chars().count();
        byte = to;
        chars
    };
    for m in ms {
        let s = advance(m.span.start);
        let e = advance(m.span.end);
        out.push((s, e));
    }
    out
}

/// Bulunan tek bir kişisel veri.
#[pyclass(frozen, module = "kalkan")]
struct Entity {
    /// Tür etiketi: "TCKN", "IBAN", "TELEFON", ...
    #[pyo3(get)]
    kind: String,
    /// Eşleşen metin.
    #[pyo3(get)]
    text: String,
    /// Başlangıç (karakter indeksi).
    #[pyo3(get)]
    start: usize,
    /// Bitiş (karakter indeksi, hariç).
    #[pyo3(get)]
    end: usize,
    /// 0-100 güven puanı.
    #[pyo3(get)]
    confidence: u8,
}

#[pymethods]
impl Entity {
    fn __repr__(&self) -> String {
        format!("Entity(kind={:?}, text={:?}, start={}, end={}, confidence={})", self.kind, self.text, self.start, self.end, self.confidence)
    }

    /// Sözlük olarak döndürür.
    fn to_dict(&self, py: Python<'_>) -> PyResult<PyObject> {
        let d = pyo3::types::PyDict::new(py);
        d.set_item("kind", &self.kind)?;
        d.set_item("text", &self.text)?;
        d.set_item("start", self.start)?;
        d.set_item("end", self.end)?;
        d.set_item("confidence", self.confidence)?;
        Ok(d.into_any().unbind())
    }
}

fn entities(text: &str, ms: &[core::Match]) -> Vec<Entity> {
    char_spans(text, ms)
        .into_iter()
        .zip(ms)
        .map(|((start, end), m)| Entity {
            kind: m.kind.label().to_string(),
            text: m.text(text).to_string(),
            start,
            end,
            confidence: m.confidence,
        })
        .collect()
}

/// Model cevabının denetim raporu.
#[pyclass(frozen, module = "kalkan")]
struct LeakReport {
    /// Bulgu yoksa True.
    #[pyo3(get)]
    clean: bool,
    /// Cevapta geçen, bizim maskelemediğimiz kişisel veriler: [(tür, metin), ...]
    #[pyo3(get)]
    new_pii: Vec<(String, String)>,
    /// Modele hiç gitmemesi gereken gerçek değerlerin cevapta görünmesi.
    #[pyo3(get)]
    echoed_originals: Vec<String>,
    /// Kasada karşılığı olmayan (uydurulmuş) yer tutucular.
    #[pyo3(get)]
    unknown_placeholders: Vec<String>,
}

#[pymethods]
impl LeakReport {
    fn __bool__(&self) -> bool {
        self.clean
    }

    fn __repr__(&self) -> String {
        if self.clean {
            "LeakReport(clean=True)".into()
        } else {
            format!(
                "LeakReport(clean=False, new_pii={:?}, echoed_originals={:?}, unknown_placeholders={:?})",
                self.new_pii, self.echoed_originals, self.unknown_placeholders
            )
        }
    }
}

/// Parça parça gelen (streaming) LLM cevapları için geri yükleyici.
#[pyclass(module = "kalkan")]
struct StreamRestorer(core::StreamRestorer);

#[pymethods]
impl StreamRestorer {
    /// Yeni parçayı ekler; kullanıcıya hemen iletilebilecek metni döndürür.
    fn push(&mut self, chunk: &str) -> String {
        self.0.push(chunk)
    }

    /// Akış bitti: bekletilen kuyruğu döndürür.
    fn finish(&mut self) -> String {
        self.0.finish()
    }
}

/// Bir sohbet/istek boyunca tutulan kasa.
#[pyclass(module = "kalkan")]
struct Session(core::Session);

#[pymethods]
impl Session {
    /// Metni maskeler.
    fn mask(&mut self, py: Python<'_>, text: &str) -> String {
        let s = &mut self.0;
        py.allow_threads(|| s.mask(text))
    }

    /// Metni maskeler; (maskeli_metin, [Entity, ...]) döndürür.
    fn mask_detailed(&mut self, text: &str) -> (String, Vec<Entity>) {
        let (out, ms) = self.0.mask_detailed(text);
        (out, entities(text, &ms))
    }

    /// LLM cevabındaki yer tutucuları gerçek değerlerle değiştirir.
    fn restore(&self, text: &str) -> String {
        self.0.restore(text)
    }

    /// Modelin ham cevabını denetler (geri yüklemeden önce çağırın).
    fn audit(&self, raw_response: &str) -> LeakReport {
        let r = self.0.audit(raw_response);
        LeakReport {
            clean: r.is_clean(),
            new_pii: r.new_pii.into_iter().map(|(k, t)| (k.label().to_string(), t)).collect(),
            echoed_originals: r.echoed_originals,
            unknown_placeholders: r.unknown_placeholders,
        }
    }

    /// Akış geri yükleyici (kasanın o anki kopyasıyla).
    fn stream(&self) -> StreamRestorer {
        StreamRestorer(self.0.stream())
    }

    /// Kasa: {yer_tutucu: orijinal}. Maskeleme ve geri yükleme farklı süreçlerdeyse
    /// bunu saklayıp `import_vault` ile yükleyin.
    #[getter]
    fn vault(&self) -> HashMap<String, String> {
        self.0.vault().map(|(a, b)| (a.to_string(), b.to_string())).collect()
    }

    /// Dışarıda saklanmış kasayı yükler.
    fn import_vault(&mut self, vault: HashMap<String, String>) {
        self.0.import_vault(vault);
    }

    fn __len__(&self) -> usize {
        self.0.len()
    }
}

/// Türkçe kişisel veri kalkanı.
///
/// Args:
///     strategy: "token" (varsayılan), "synthetic", "partial" veya "label".
///     kinds: Yalnızca bu türler, ör. ["TCKN", "IBAN"]. None → hepsi.
///     exclude: Bu türleri hariç tut, ör. ["PLAKA"].
///     min_confidence: 0-100 güven eşiği (varsayılan 70).
///     secret: Sentetik üretim anahtarı (int).
///     terms: Kuruma özel terimler, ör. ["Proje Atlas", "Acme Holding"].
#[pyclass(frozen, module = "kalkan", name = "Kalkan")]
struct PyKalkan(core::Kalkan);

#[pymethods]
impl PyKalkan {
    #[new]
    #[pyo3(signature = (strategy="token", kinds=None, exclude=None, min_confidence=70, secret=None, terms=None))]
    fn new(
        strategy: &str,
        kinds: Option<Vec<String>>,
        exclude: Option<Vec<String>>,
        min_confidence: u8,
        secret: Option<u64>,
        terms: Option<Vec<String>>,
    ) -> PyResult<Self> {
        let kind = |s: &String| core::Kind::from_label(s).ok_or_else(|| PyValueError::new_err(format!("bilinmeyen tür: {s:?}")));
        let mut b = core::Kalkan::builder().strategy(parse_strategy(strategy)?).min_confidence(min_confidence);
        if let Some(ks) = kinds {
            b = b.kinds(ks.iter().map(kind).collect::<PyResult<Vec<_>>>()?);
        }
        for k in exclude.unwrap_or_default().iter().map(kind) {
            b = b.without(k?);
        }
        if let Some(s) = secret {
            b = b.secret(s);
        }
        if let Some(t) = terms {
            b = b.terms(t);
        }
        Ok(Self(b.build()))
    }

    /// Kişisel verileri bulur: [Entity, ...]
    fn detect(&self, py: Python<'_>, text: &str) -> Vec<Entity> {
        let ms = py.allow_threads(|| self.0.detect(text));
        entities(text, &ms)
    }

    /// Tek seferlik maskeleme.
    fn mask(&self, py: Python<'_>, text: &str) -> String {
        py.allow_threads(|| self.0.mask(text))
    }

    /// Metin kişisel veri içeriyor mu?
    fn contains_pii(&self, py: Python<'_>, text: &str) -> bool {
        py.allow_threads(|| self.0.contains_pii(text))
    }

    /// Yeni oturum (kasa). Her sohbet/istek için bir tane kullanın.
    fn session(&self) -> Session {
        Session(self.0.session())
    }

    /// Çok sayıda metni **tüm çekirdeklerde paralel** maskeler (GIL serbest).
    /// Her metin kendi oturumunda işlenir. Toplu anonimleştirme için.
    #[pyo3(signature = (texts, threads=None))]
    fn mask_batch(&self, py: Python<'_>, texts: Vec<String>, threads: Option<usize>) -> Vec<String> {
        let k = &self.0;
        py.allow_threads(|| parallel(&texts, threads, |t| k.mask(t)))
    }

    /// Çok sayıda metinde paralel tespit: [[Entity, ...], ...]
    #[pyo3(signature = (texts, threads=None))]
    fn detect_batch(&self, py: Python<'_>, texts: Vec<String>, threads: Option<usize>) -> Vec<Vec<Entity>> {
        let k = &self.0;
        let all = py.allow_threads(|| parallel(&texts, threads, |t| k.detect(t)));
        texts.iter().zip(all).map(|(t, ms)| entities(t, &ms)).collect()
    }

    fn __repr__(&self) -> String {
        format!("Kalkan(strategy={:?})", self.0.strategy())
    }
}

/// Girdiyi parçalara bölüp iş parçacıklarında işler; sırayı korur.
fn parallel<T: Send>(texts: &[String], threads: Option<usize>, f: impl Fn(&str) -> T + Sync) -> Vec<T> {
    let n = threads
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get()))
        .clamp(1, 256);
    if n == 1 || texts.len() < 64 {
        return texts.iter().map(|t| f(t)).collect();
    }
    let chunk = texts.len().div_ceil(n);
    let f = &f;
    std::thread::scope(|sc| {
        let handles: Vec<_> = texts
            .chunks(chunk)
            .map(|c| sc.spawn(move || c.iter().map(|t| f(t)).collect::<Vec<T>>()))
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("iş parçacığı çöktü")).collect()
    })
}

fn digits(s: &str) -> Vec<u8> {
    s.bytes().filter(u8::is_ascii_digit).collect()
}

/// T.C. Kimlik No geçerli mi? (boşluklar yok sayılır)
#[pyfunction]
fn is_valid_tckn(s: &str) -> bool {
    s.chars().all(|c| c.is_ascii_digit() || c.is_whitespace()) && core::checksum::is_valid_tckn(&digits(s))
}

/// Vergi Kimlik No geçerli mi?
#[pyfunction]
fn is_valid_vkn(s: &str) -> bool {
    s.chars().all(|c| c.is_ascii_digit() || c.is_whitespace()) && core::checksum::is_valid_vkn(&digits(s))
}

/// TR IBAN geçerli mi? ("TR" ile başlamalı; boşluklar yok sayılır)
#[pyfunction]
fn is_valid_iban(s: &str) -> bool {
    let t: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let b = t.as_bytes();
    b.len() == 26 && b[..2].eq_ignore_ascii_case(b"TR") && core::checksum::is_valid_tr_iban(&b[2..])
}

/// Kart numarası Luhn kontrolünden geçiyor mu?
#[pyfunction]
fn luhn_valid(s: &str) -> bool {
    let d = digits(s);
    (12..=19).contains(&d.len()) && core::checksum::luhn_valid(&d)
}

#[pymodule]
fn _kalkan(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyKalkan>()?;
    m.add_class::<Session>()?;
    m.add_class::<StreamRestorer>()?;
    m.add_class::<Entity>()?;
    m.add_class::<LeakReport>()?;
    m.add_function(wrap_pyfunction!(is_valid_tckn, m)?)?;
    m.add_function(wrap_pyfunction!(is_valid_vkn, m)?)?;
    m.add_function(wrap_pyfunction!(is_valid_iban, m)?)?;
    m.add_function(wrap_pyfunction!(luhn_valid, m)?)?;
    m.add("KINDS", core::Kind::ALL.iter().map(|k| k.label()).collect::<Vec<_>>())?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
