//! Uçtan uca LLM akışı: maskele → (sahte) LLM'e gönder → akış halinde gelen
//! cevabı parça parça geri yükle.
//!
//! ```text
//! cargo run --example llm_akisi
//! ```

use kalkan::{Kalkan, Strategy};

/// LLM yerine geçen sahte fonksiyon: cevabı küçük parçalar halinde "akıtır".
/// Gerçekte burası OpenAI/Anthropic/yerel model çağrısıdır.
fn sahte_llm(istem: &str) -> Vec<String> {
    let cevap = if istem.contains("[AD_SOYAD_1]") {
        "Merhaba [AD_SOYAD_1], [TCKN_1] kimlik numaralı kaydınız bulundu. \
         İadeniz [IBAN_1] hesabına yapılacak; bilgi için [TELEFON_1] aranacaktır."
            .to_string()
    } else {
        format!("(Sentetik istem gördüm: {istem})")
    };
    // Token'ları ortadan bölen 3 karakterlik parçalar.
    let cs: Vec<char> = cevap.chars().collect();
    cs.chunks(3).map(|c| c.iter().collect()).collect()
}

fn main() {
    let kullanici = "Ben Ayşe Kaya, TC 10000000146. İadeyi TR33 0006 1005 1978 6457 8413 26 \
                     hesabıma yapın, telefonum 0532 123 45 67.";

    for strateji in [Strategy::Token, Strategy::Synthetic] {
        println!("\n=== Strateji: {strateji:?} ===");
        let kalkan = Kalkan::builder().strategy(strateji).build();
        let mut oturum = kalkan.session();

        let istem = oturum.mask(kullanici);
        println!("LLM'e giden : {istem}");

        let mut akis = oturum.stream();
        print!("Kullanıcıya : ");
        for parca in sahte_llm(&istem) {
            print!("{}", akis.push(&parca)); // her parça anında iletilir
        }
        println!("{}", akis.finish());
    }
}
