//! kalkan CLI
//!
//! ```text
//! kalkan mask    [--strategy token|synthetic|partial|label] [--vault kasa.tsv] < girdi
//! kalkan restore --vault kasa.tsv < llm_cevabi
//! kalkan detect  [--json] < girdi
//! kalkan audit   --vault kasa.tsv < llm_cevabi
//! ```
//!
//! `detect --json` çıktısındaki `start`/`end` değerleri **bayt** konumudur
//! (Python bağlantısı karakter, WASM/JS UTF-16 konumu kullanır).

use std::io::{self, Read, Write};
use std::process::ExitCode;

use kalkan::{Kalkan, Kind, Strategy};

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
kalkan — Türkçe kişisel verileri LLM'e gitmeden maskeler, cevapta geri yükler.

KULLANIM:
  kalkan mask    [SEÇENEKLER] < girdi.txt > maskeli.txt
  kalkan restore --vault kasa.tsv < cevap.txt
  kalkan detect  [--json] < girdi.txt
  kalkan audit   --vault kasa.tsv < cevap.txt     (modelin ham cevabını denetler)

SEÇENEKLER:
  -s, --strategy <S>     token (varsayılan) | synthetic | partial | label
  -v, --vault <DOSYA>    mask: kasayı yaz; restore: kasayı oku (TSV)
  -k, --kinds <LİSTE>    yalnızca bu türler: tckn,vkn,iban,telefon,eposta,kart,plaka,
                         ad_soyad,ip,dogum_tarihi,pasaport,ozel
  -t, --terms <LİSTE>    kuruma özel terimler, virgülle: 'Proje Atlas,Acme Holding'
      --terms-file <D>   kuruma özel terimler, satır başına bir tane
  -c, --min-confidence <N>  güven eşiği 0-100 (varsayılan 70)
      --secret <N>       sentetik üretim anahtarı
      --json             detect çıktısını JSON ver (konumlar bayt cinsinden)
  -h, --help             bu yardım
  -V, --version          sürümü göster

ÖRNEK (LLM boru hattı):
  kalkan mask -v kasa.tsv < istem.txt | llm-cli | kalkan restore -v kasa.tsv
";

struct Args {
    cmd: String,
    strategy: Strategy,
    vault: Option<String>,
    kinds: Option<Vec<Kind>>,
    min_conf: Option<u8>,
    secret: Option<u64>,
    json: bool,
    terms: Vec<String>,
}

fn parse() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let mut a = Args {
        cmd: String::new(),
        strategy: Strategy::Token,
        vault: None,
        kinds: None,
        min_conf: None,
        secret: None,
        json: false,
        terms: Vec::new(),
    };
    let need = |it: &mut dyn Iterator<Item = String>, f: &str| it.next().ok_or(format!("{f} bir değer bekliyor"));
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => a.cmd = "help".into(),
            "-V" | "--version" => a.cmd = "version".into(),
            "-s" | "--strategy" => {
                a.strategy = match need(&mut it, &arg)?.to_lowercase().as_str() {
                    "token" => Strategy::Token,
                    "synthetic" | "sentetik" => Strategy::Synthetic,
                    "partial" | "kismi" | "kısmi" => Strategy::Partial,
                    "label" | "etiket" => Strategy::Label,
                    s => return Err(format!("bilinmeyen strateji: {s}")),
                }
            }
            "-v" | "--vault" => a.vault = Some(need(&mut it, &arg)?),
            "-k" | "--kinds" => {
                let v = need(&mut it, &arg)?;
                let kinds = v
                    .split(',')
                    .map(|s| Kind::from_label(s).ok_or(format!("bilinmeyen tür: {s}")))
                    .collect::<Result<Vec<_>, _>>()?;
                a.kinds = Some(kinds);
            }
            "-c" | "--min-confidence" => {
                a.min_conf = Some(need(&mut it, &arg)?.parse().map_err(|_| "güven eşiği 0-100 olmalı")?)
            }
            "--secret" => a.secret = Some(need(&mut it, &arg)?.parse().map_err(|_| "secret bir sayı olmalı")?),
            "--json" => a.json = true,
            "-t" | "--terms" => a.terms.extend(need(&mut it, &arg)?.split(',').map(str::to_string)),
            "--terms-file" => {
                let path = need(&mut it, &arg)?;
                let body = std::fs::read_to_string(&path).map_err(|e| format!("terim dosyası okunamadı ({path}): {e}"))?;
                a.terms.extend(body.lines().map(str::to_string));
            }
            s if a.cmd.is_empty() && !s.starts_with('-') => a.cmd = s.to_string(),
            s => return Err(format!("bilinmeyen argüman: {s}")),
        }
    }
    Ok(a)
}

fn json_str(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn run() -> Result<(), String> {
    let a = parse()?;
    if a.cmd.is_empty() || a.cmd == "help" {
        print!("{HELP}");
        return Ok(());
    }
    if a.cmd == "version" {
        println!("kalkan {VERSION}");
        return Ok(());
    }
    let mut b = Kalkan::builder().strategy(a.strategy);
    if let Some(k) = a.kinds {
        b = b.kinds(k);
    }
    if let Some(c) = a.min_conf {
        b = b.min_confidence(c);
    }
    if let Some(s) = a.secret {
        b = b.secret(s);
    }
    if !a.terms.is_empty() {
        b = b.terms(&a.terms);
    }
    let kalkan = b.build();

    let mut input = String::new();
    io::stdin().read_to_string(&mut input).map_err(|e| format!("girdi okunamadı: {e}"))?;
    let mut out = io::stdout().lock();
    let w = |r: io::Result<()>| r.map_err(|e| format!("yazılamadı: {e}"));

    match a.cmd.as_str() {
        "mask" => {
            let mut s = kalkan.session();
            let masked = s.mask(&input);
            w(out.write_all(masked.as_bytes()))?;
            if let Some(path) = a.vault {
                let mut tsv = String::new();
                let mut pairs: Vec<_> = s.vault().collect();
                pairs.sort();
                for (rep, orig) in pairs {
                    tsv.push_str(&format!("{rep}\t{orig}\n"));
                }
                std::fs::write(&path, tsv).map_err(|e| format!("kasa yazılamadı ({path}): {e}"))?;
            }
        }
        "restore" => {
            let path = a.vault.ok_or("restore için --vault gerekli")?;
            let tsv = std::fs::read_to_string(&path).map_err(|e| format!("kasa okunamadı ({path}): {e}"))?;
            let mut s = kalkan.session();
            s.import_vault(tsv.lines().filter_map(|l| l.split_once('\t')));
            w(out.write_all(s.restore(&input).as_bytes()))?;
        }
        "audit" => {
            let path = a.vault.ok_or("audit için --vault gerekli")?;
            let tsv = std::fs::read_to_string(&path).map_err(|e| format!("kasa okunamadı ({path}): {e}"))?;
            let mut s = kalkan.session();
            s.import_vault(tsv.lines().filter_map(|l| l.split_once('\t')));
            let r = s.audit(&input);
            if a.json {
                let list = |v: &[String]| v.iter().map(|x| json_str(x)).collect::<Vec<_>>().join(",");
                let new: Vec<String> =
                    r.new_pii.iter().map(|(k, t)| format!("{{\"kind\":\"{k}\",\"text\":{}}}", json_str(t))).collect();
                w(writeln!(
                    out,
                    "{{\"clean\":{},\"new_pii\":[{}],\"echoed_originals\":[{}],\"unknown_placeholders\":[{}]}}",
                    r.is_clean(),
                    new.join(","),
                    list(&r.echoed_originals),
                    list(&r.unknown_placeholders)
                ))?;
            } else if r.is_clean() {
                w(writeln!(out, "✓ temiz: cevapta yeni kişisel veri yok"))?;
            } else {
                for (k, t) in &r.new_pii {
                    w(writeln!(out, "YENİ VERİ     {:<12} {t}", k.label()))?;
                }
                for t in &r.echoed_originals {
                    w(writeln!(out, "GERÇEK DEĞER  {t}"))?;
                }
                for t in &r.unknown_placeholders {
                    w(writeln!(out, "UYDURMA TOKEN {t}"))?;
                }
            }
            if !r.is_clean() {
                return Err("cevapta bulgu var".into());
            }
        }
        "detect" => {
            let ms = kalkan.detect(&input);
            if a.json {
                let items: Vec<String> = ms
                    .iter()
                    .map(|m| {
                        format!(
                            "{{\"kind\":\"{}\",\"start\":{},\"end\":{},\"confidence\":{},\"text\":{}}}",
                            m.kind,
                            m.span.start,
                            m.span.end,
                            m.confidence,
                            json_str(m.text(&input))
                        )
                    })
                    .collect();
                w(writeln!(out, "[{}]", items.join(",")))?;
            } else {
                for m in &ms {
                    w(writeln!(out, "{:<9} {:>3}  {}", m.kind.label(), m.confidence, m.text(&input)))?;
                }
                w(writeln!(out, "— {} kişisel veri bulundu", ms.len()))?;
            }
        }
        c => return Err(format!("bilinmeyen komut: {c} (mask | restore | detect | audit)")),
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("kalkan: {e}");
            ExitCode::FAILURE
        }
    }
}
