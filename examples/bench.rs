//! Performans ölçümü.
//!
//! ```text
//! python3 bench/gen_corpus.py corpus.txt 5
//! cargo run --release --example bench -- corpus.txt [tespitler.jsonl]
//! ```
//!
//! Derlem boş satırlarla ayrılmış mesajlardan oluşur; her mesaj ayrı bir
//! "LLM isteği" gibi işlenir (gerçek kullanıma en yakın senaryo).

use std::time::Instant;

use kalkan::Kalkan;

fn json_str(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("kullanım: bench <derlem.txt> [tespitler.jsonl]");
    let out = args.next();
    let text = std::fs::read_to_string(&path).expect("derlem okunamadı");
    let msgs: Vec<&str> = text.split("\n\n").filter(|m| !m.is_empty()).collect();
    let bytes: usize = msgs.iter().map(|m| m.len()).sum();
    let mb = bytes as f64 / (1024.0 * 1024.0);
    let k = Kalkan::new();

    // Isınma + tespit sayısı
    let total: usize = msgs.iter().map(|m| k.detect(m).len()).sum();

    const RUNS: usize = 7;
    let mut detect_t = Vec::new();
    for _ in 0..RUNS {
        let t = Instant::now();
        let mut n = 0;
        for m in &msgs {
            n += k.detect(m).len();
        }
        assert_eq!(n, total);
        detect_t.push(t.elapsed().as_secs_f64());
    }

    let mut roundtrip_t = Vec::new();
    for _ in 0..RUNS {
        let t = Instant::now();
        for m in &msgs {
            let mut s = k.session();
            let masked = s.mask(m);
            let back = s.restore(&masked);
            debug_assert_eq!(back, *m);
            std::hint::black_box(back);
        }
        roundtrip_t.push(t.elapsed().as_secs_f64());
    }

    let d = median(detect_t);
    let r = median(roundtrip_t);
    println!("derlem          : {:.2} MB, {} mesaj, {} kişisel veri", mb, msgs.len(), total);
    println!("tespit          : {:>8.1} MB/s  {:>9.0} mesaj/s  {:>6.2} µs/mesaj", mb / d, msgs.len() as f64 / d, d * 1e6 / msgs.len() as f64);
    println!("maskele+geri yük: {:>8.1} MB/s  {:>9.0} mesaj/s  {:>6.2} µs/mesaj", mb / r, msgs.len() as f64 / r, r * 1e6 / msgs.len() as f64);

    // Çok çekirdek: mesajları iş parçacıklarına böl (Kalkan `Send + Sync`).
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    if threads > 1 {
        let t = Instant::now();
        let k = &k;
        std::thread::scope(|sc| {
            for chunk in msgs.chunks(msgs.len().div_ceil(threads)) {
                sc.spawn(move || chunk.iter().map(|m| k.detect(m).len()).sum::<usize>());
            }
        });
        let p = t.elapsed().as_secs_f64();
        println!("tespit ({threads:>2} çekirdek): {:>6.1} MB/s", mb / p);
    }

    if let Some(out) = out {
        let mut s = String::new();
        for m in &msgs {
            let items: Vec<String> =
                k.detect(m).iter().map(|x| format!("[\"{}\",{}]", x.kind, json_str(x.text(m)))).collect();
            s.push('[');
            s.push_str(&items.join(","));
            s.push_str("]\n");
        }
        std::fs::write(&out, s).expect("çıktı yazılamadı");
    }
}
