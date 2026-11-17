use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let n = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let p = std::env::temp_dir().join(format!("kalkan-cli-{}-{tag}-{n}", std::process::id()));
        std::fs::create_dir_all(&p).expect("geçici dizin oluşturulamadı");
        Self(p)
    }

    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run(args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_kalkan"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("CLI başlatılamadı");
    child.stdin.take().expect("stdin yok").write_all(input.as_bytes()).expect("stdin yazılamadı");
    child.wait_with_output().expect("CLI tamamlanamadı")
}

fn stdout(o: &Output) -> String {
    assert!(o.status.success(), "stderr: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout.clone()).expect("stdout UTF-8 değil")
}

#[test]
fn mask_vault_restore_roundtrip() {
    let dir = TempDir::new("roundtrip");
    let vault = dir.file("kasa.tsv");
    let kaynak = "Ayşe Kaya TC 10000000146 tel 0532 123 45 67";
    let masked = stdout(&run(&["mask", "-v", vault.to_str().unwrap()], kaynak));
    assert_eq!(masked, "[AD_SOYAD_1] TC [TCKN_1] tel [TELEFON_1]");

    let tsv = std::fs::read_to_string(&vault).expect("kasa okunamadı");
    assert!(tsv.contains("[TCKN_1]\t10000000146"), "{tsv}");

    let restored = stdout(&run(&["restore", "-v", vault.to_str().unwrap()], &masked));
    assert_eq!(restored.trim_end(), kaynak);
}

#[test]
fn detect_json_reports_byte_offsets() {
    let o = run(&["detect", "--json"], "Çağrı: TC 10000000146");
    let json = stdout(&o);
    assert!(json.contains("\"kind\":\"TCKN\""), "{json}");
    assert!(json.contains("\"start\":13,\"end\":24"), "{json}");
}

#[test]
fn audit_reports_findings_and_exits_nonzero() {
    let dir = TempDir::new("audit");
    let vault = dir.file("kasa.tsv");
    stdout(&run(&["mask", "-v", vault.to_str().unwrap()], "TC 10000000146"));

    let o = run(&["audit", "--json", "-v", vault.to_str().unwrap()], "[TCKN_9]");
    assert!(!o.status.success());
    let json = String::from_utf8(o.stdout).expect("stdout UTF-8 değil");
    assert!(json.contains("\"clean\":false"), "{json}");
    assert!(json.contains("[TCKN_9]"), "{json}");

    let clean = run(&["audit", "--json", "-v", vault.to_str().unwrap()], "[TCKN_1]");
    assert!(clean.status.success());
    assert!(String::from_utf8_lossy(&clean.stdout).contains("\"clean\":true"));
}

#[test]
fn help_and_invalid_input() {
    assert!(stdout(&run(&["--help"], "")).contains("KULLANIM:"));
    assert_eq!(stdout(&run(&["--version"], "")).trim(), "kalkan 0.2.0");
    assert!(stdout(&run(&[], "")).contains("kalkan —"));
    assert!(!run(&["mask", "-s", "yok"], "x").status.success());
    assert!(!run(&["yok"], "x").status.success());
}

#[test]
fn kinds_and_terms_filters_are_applied() {
    let o = run(&["mask", "-k", "tckn,ozel", "-t", "Proje Atlas"], "TC 10000000146, Proje Atlas, 0532 123 45 67");
    assert_eq!(stdout(&o), "TC [TCKN_1], [OZEL_1], 0532 123 45 67");
}

#[test]
fn vault_file_is_required_for_restore() {
    assert!(!run(&["restore"], "[TCKN_1]").status.success());
}

#[test]
fn audit_needs_vault_file() {
    assert!(!run(&["audit"], "[TCKN_1]").status.success());
}

#[test]
fn terms_file_is_read_line_by_line() {
    let dir = TempDir::new("terms");
    let path = dir.file("terimler.txt");
    std::fs::write(&path, "Proje Atlas\nAcme Holding\n").expect("terim dosyası yazılamadı");
    let o = run(&["mask", "--terms-file", path.to_str().unwrap()], "PROJE IŞIK değil, PROJE ATLAS ve ACME HOLDING");
    assert_eq!(stdout(&o), "PROJE IŞIK değil, [OZEL_1] ve [OZEL_2]");
}

#[test]
fn partial_and_label_are_not_reversible() {
    let dir = TempDir::new("strategies");
    let partial = dir.file("partial.tsv");
    let out = stdout(&run(&["mask", "-s", "partial", "-v", partial.to_str().unwrap()], "TC 10000000146"));
    assert_eq!(out, "TC *********46");
    assert_eq!(std::fs::read_to_string(&partial).expect("kasa okunamadı"), "");

    let label = stdout(&run(&["mask", "-s", "label"], "TC 10000000146"));
    assert_eq!(label, "TC [TCKN]");
}

#[test]
fn min_confidence_filters_weak_matches() {
    let input = "12 TL 500";
    let low = stdout(&run(&["detect", "--json", "-c", "0"], input));
    let high = stdout(&run(&["detect", "--json", "-c", "100"], input));
    assert!(low.contains("PLAKA"), "{low}");
    assert_eq!(high.trim(), "[]");
}

#[test]
fn unknown_vault_line_does_not_panic() {
    let dir = TempDir::new("vault");
    let vault = dir.file("kasa.tsv");
    std::fs::write(&vault, "[TCKN_1]\t10000000146\nboş satır\n[TELEFON_1]\n").expect("kasa yazılamadı");
    let o = run(&["restore", "-v", vault.to_str().unwrap()], "[TCKN_1] tamam");
    assert_eq!(stdout(&o).trim_end(), "10000000146 tamam");
}
