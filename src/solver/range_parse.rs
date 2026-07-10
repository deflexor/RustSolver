//! PPT / PokerStove hyphen range expansion (`QQ-22`, `A5s-A4s`, `AQs-A2s`).
//!
//! `rust_poker::HandRange::from_string` supports `+` but not `-`; this module
//! expands hyphen tokens before parsing.

const RANKS: &[u8] = b"23456789TJQKA";

fn rank_idx(c: char) -> Option<usize> {
    RANKS.iter().position(|&r| r == c as u8 || r == c.to_ascii_uppercase() as u8)
}

/// Expand one PPT token that may contain a hyphen range.
fn expand_token(token: &str) -> String {
    let token = token.trim();
    if token.is_empty() {
        return String::new();
    }
    if !token.contains('-') {
        return token.to_string();
    }

    let parts: Vec<&str> = token.splitn(2, '-').collect();
    if parts.len() != 2 {
        return token.to_string();
    }
    let hi = parts[0].trim();
    let lo = parts[1].trim();
    if hi.len() < 2 || lo.len() < 2 {
        return token.to_string();
    }

    let hi_r1 = hi.chars().next().unwrap().to_ascii_uppercase();
    let hi_r2 = hi.chars().nth(1).unwrap().to_ascii_uppercase();
    let lo_r1 = lo.chars().next().unwrap().to_ascii_uppercase();
    let lo_r2 = lo.chars().nth(1).unwrap().to_ascii_uppercase();

    let hi_suffix = hi.chars().skip(2).collect::<String>().to_ascii_lowercase();
    let lo_suffix = lo.chars().skip(2).collect::<String>().to_ascii_lowercase();
    if hi_suffix != lo_suffix {
        return token.to_string();
    }

    // Pair range: QQ-22
    if hi_r1 == hi_r2 && lo_r1 == lo_r2 && hi_suffix.is_empty() {
        let Some(hi_i) = rank_idx(hi_r1) else {
            return token.to_string();
        };
        let Some(lo_i) = rank_idx(lo_r1) else {
            return token.to_string();
        };
        if hi_i < lo_i {
            return token.to_string();
        }
        let mut out = Vec::new();
        for i in (lo_i..=hi_i).rev() {
            let r = RANKS[i] as char;
            out.push(format!("{r}{r}"));
        }
        return out.join(",");
    }

    // Suited / offsuit run: AQs-A2s, K9o-K5o
    if hi_r1 == lo_r1 && !hi_suffix.is_empty() {
        let Some(hi_i) = rank_idx(hi_r2) else {
            return token.to_string();
        };
        let Some(lo_i) = rank_idx(lo_r2) else {
            return token.to_string();
        };
        if hi_i < lo_i {
            return token.to_string();
        }
        let mut out = Vec::new();
        for i in (lo_i..=hi_i).rev() {
            let r2 = RANKS[i] as char;
            out.push(format!("{hi_r1}{r2}{hi_suffix}"));
        }
        return out.join(",");
    }

    token.to_string()
}

/// Expand all hyphen tokens in a PPT range string.
pub fn expand_ppt_range(range: &str) -> String {
    range
        .split(',')
        .filter_map(|part| {
            let expanded = expand_token(part);
            if expanded.is_empty() {
                None
            } else {
                Some(expanded)
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_poker::hand_range::HandRange;

    #[test]
    fn expand_pair_range() {
        let s = expand_ppt_range("QQ-22");
        assert!(s.contains("QQ") && s.contains("22") && s.contains("JJ"));
        let r = HandRange::from_string(s);
        assert!(r.hands.len() > 50);
    }

    #[test]
    fn expand_suited_range() {
        let s = expand_ppt_range("A5s-A4s");
        assert_eq!(s, "A5s,A4s");
    }

    #[test]
    fn expand_mixed_range_string() {
        let s = expand_ppt_range("A5s-A4s");
        assert_eq!(s, "A5s,A4s");
        let full = expand_ppt_range("66+,A8s+,A5s-A4s,AJo+");
        assert!(full.contains("A5s") && full.contains("A4s"));
        let r = HandRange::from_string(full);
        assert!(r.hands.len() > 50);
    }
}
