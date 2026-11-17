//! Sağlama toplamı algoritmaları. Hem doğrulama hem de sentetik (ama geçerli)
//! değer üretimi için kullanılır. Girdiler ASCII rakam baytlarıdır (`b'0'..=b'9'`).

#[inline]
fn d(b: u8) -> u32 {
    (b - b'0') as u32
}

/// T.C. Kimlik No'nun 10. ve 11. hanelerini ilk 9 haneden hesaplar.
#[must_use]
pub fn tckn_check_digits(first9: &[u8]) -> (u8, u8) {
    debug_assert_eq!(first9.len(), 9);
    let odd: u32 = [0, 2, 4, 6, 8].iter().map(|&i| d(first9[i])).sum();
    let even: u32 = [1, 3, 5, 7].iter().map(|&i| d(first9[i])).sum();
    let d10 = ((odd * 7 + 100 - even) % 10) as u8; // +100: negatif olmasın
    let sum9: u32 = first9.iter().map(|&b| d(b)).sum();
    let d11 = ((sum9 + d10 as u32) % 10) as u8;
    (b'0' + d10, b'0' + d11)
}

/// 11 haneli T.C. Kimlik No geçerli mi?
#[must_use]
pub fn is_valid_tckn(digits: &[u8]) -> bool {
    if digits.len() != 11 || digits[0] == b'0' || !digits.iter().all(u8::is_ascii_digit) {
        return false;
    }
    if digits[..9].iter().all(|&b| b == digits[0]) {
        return false; // 11111111110 gibi sağlamadan geçen sahte kalıplar
    }
    tckn_check_digits(&digits[..9]) == (digits[9], digits[10])
}

/// Vergi Kimlik No'nun son hanesini ilk 9 haneden hesaplar (GİB algoritması).
#[must_use]
pub fn vkn_check_digit(first9: &[u8]) -> u8 {
    debug_assert_eq!(first9.len(), 9);
    let mut sum = 0u32;
    for (i, &b) in first9.iter().enumerate() {
        let tmp = (d(b) + 9 - i as u32) % 10;
        let mut v = (tmp * (1 << (9 - i as u32))) % 9;
        if tmp != 0 && v == 0 {
            v = 9;
        }
        sum += v;
    }
    b'0' + ((10 - sum % 10) % 10) as u8
}

/// 10 haneli VKN geçerli mi?
#[must_use]
pub fn is_valid_vkn(digits: &[u8]) -> bool {
    digits.len() == 10
        && digits.iter().all(u8::is_ascii_digit)
        && !digits.iter().all(|&b| b == digits[0])
        && vkn_check_digit(&digits[..9]) == digits[9]
}

/// Luhn algoritması (kart numaraları).
#[must_use]
pub fn luhn_valid(digits: &[u8]) -> bool {
    luhn_sum(digits, false) % 10 == 0
}

fn luhn_sum(digits: &[u8], double_first_from_right: bool) -> u32 {
    let mut sum = 0;
    let mut double = double_first_from_right;
    for &b in digits.iter().rev() {
        let mut v = d(b);
        if double {
            v *= 2;
            if v > 9 {
                v -= 9;
            }
        }
        sum += v;
        double = !double;
    }
    sum
}

/// Kart gövdesine eklenecek Luhn hanesini hesaplar.
#[must_use]
pub fn luhn_check_digit(body: &[u8]) -> u8 {
    b'0' + ((10 - luhn_sum(body, true) % 10) % 10) as u8
}

/// IBAN mod-97: `country` iki harf (ör. b"TR"), `digits` = 2 kontrol hanesi + BBAN.
#[must_use]
pub fn iban_mod97(country: [u8; 2], check: &[u8], bban: &[u8]) -> u32 {
    let mut rem: u32 = 0;
    let mut push = |v: u32| {
        rem = if v >= 10 { (rem * 100 + v) % 97 } else { (rem * 10 + v) % 97 };
    };
    for &b in bban {
        push(d(b));
    }
    for c in country {
        push((c.to_ascii_uppercase() - b'A') as u32 + 10);
    }
    for &b in check {
        push(d(b));
    }
    rem
}

/// TR IBAN geçerli mi? `digits` = TR'den sonraki 24 hane.
#[must_use]
pub fn is_valid_tr_iban(digits: &[u8]) -> bool {
    digits.len() == 24
        && digits.iter().all(u8::is_ascii_digit)
        && iban_mod97(*b"TR", &digits[..2], &digits[2..]) == 1
}

/// Verilen 22 haneli BBAN için TR IBAN kontrol hanelerini hesaplar.
#[must_use]
pub fn tr_iban_check_digits(bban: &[u8]) -> [u8; 2] {
    let r = iban_mod97(*b"TR", b"00", bban);
    let c = 98 - r;
    [b'0' + (c / 10) as u8, b'0' + (c % 10) as u8]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tckn_known() {
        assert!(is_valid_tckn(b"10000000146"));
        assert!(!is_valid_tckn(b"10000000147"));
        assert!(!is_valid_tckn(b"01234567890"));
        assert!(!is_valid_tckn(b"11111111110"));
    }

    #[test]
    fn iban_known() {
        assert!(is_valid_tr_iban(b"330006100519786457841326"));
        assert!(!is_valid_tr_iban(b"330006100519786457841327"));
        let bban = b"0006100519786457841326";
        assert_eq!(&tr_iban_check_digits(bban), b"33");
    }

    #[test]
    fn luhn_known() {
        assert!(luhn_valid(b"4111111111111111"));
        assert!(!luhn_valid(b"4111111111111112"));
        assert_eq!(luhn_check_digit(b"411111111111111"), b'1');
    }

    #[test]
    fn vkn_roundtrip() {
        for seed in 0..500u32 {
            let s = format!("{:09}", seed.wrapping_mul(2654435761) % 1_000_000_000);
            let mut v = s.into_bytes();
            v.push(vkn_check_digit(&v));
            if !v.iter().all(|&b| b == v[0]) {
                assert!(is_valid_vkn(&v));
            }
        }
    }
}
