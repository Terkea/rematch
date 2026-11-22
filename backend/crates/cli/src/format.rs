//! Number formatting that matches JavaScript.

/// JavaScript's `Number.prototype.toFixed` for finite `x` with `|x| < 1e21`: take the exact decimal
/// expansion of the double and round half up in magnitude. (Rust's `{:.N}` rounds exact ties to
/// even: `1.125` gives `1.12` where JavaScript gives `1.13`.)
pub fn to_fixed(x: f64, digits: usize) -> String {
    if x.is_nan() {
        return "NaN".to_owned();
    }
    if x.abs() >= 1e21 || x.is_infinite() {
        return format!("{x}");
    }
    if x < 0.0 {
        return format!("-{}", to_fixed(-x, digits));
    }
    // 1100 fractional digits are more than any double needs, so this expansion is exact.
    let exact = format!("{:.1100}", x.abs());
    let (int, frac) = exact.split_once('.').unwrap_or((&exact, ""));
    let keep = &frac[..digits.min(frac.len())];
    let round_up = frac.as_bytes().get(digits).is_some_and(|d| *d >= b'5');
    let mut all: Vec<u8> = int.bytes().chain(keep.bytes()).collect();
    if round_up {
        let mut i = all.len();
        loop {
            if i == 0 {
                all.insert(0, b'1');
                break;
            }
            i -= 1;
            if all[i] == b'9' {
                all[i] = b'0';
            } else {
                all[i] += 1;
                break;
            }
        }
    }
    let split = all.len() - keep.len();
    let (i, f) = all.split_at(split);
    let i = String::from_utf8_lossy(i);
    if digits == 0 {
        i.into_owned()
    } else {
        format!("{i}.{}", String::from_utf8_lossy(f))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_fixed_matches_js_on_ties() {
        assert_eq!(to_fixed(1.125, 2), "1.13");
        assert_eq!(to_fixed(0.125, 2), "0.13");
        assert_eq!(to_fixed(85.5, 1), "85.5");
        assert_eq!(to_fixed(65.05, 1), "65.0");
        assert_eq!(to_fixed(0.0, 2), "0.00");
    }

    #[test]
    fn to_fixed_carries_and_signs() {
        assert_eq!(to_fixed(9.995, 2), "9.99"); // 9.995 is below the tie in binary
        assert_eq!(to_fixed(99.96, 1), "100.0");
        assert_eq!(to_fixed(0.5, 0), "1");
        assert_eq!(to_fixed(2.5, 0), "3");
        assert_eq!(to_fixed(-1.125, 2), "-1.13");
        assert_eq!(to_fixed(-0.0001, 2), "-0.00");
        assert_eq!(to_fixed(-0.0, 2), "0.00");
        assert_eq!(to_fixed(66.1, 1), "66.1");
        assert_eq!(to_fixed(3.125, 2), "3.13");
    }
}
