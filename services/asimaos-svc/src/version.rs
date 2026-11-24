//! Asima OS versions are dotted numbers, e.g. 2026.11.15.1 (date of the build, then a counter).

use std::cmp::Ordering;

pub fn parse(v: &str) -> Option<Vec<u64>> {
    let parts: Vec<u64> = v.split('.').map(|p| p.parse().ok()).collect::<Option<_>>()?;
    (parts.len() >= 2 && parts.len() <= 6).then_some(parts)
}

pub fn valid(v: &str) -> bool {
    parse(v).is_some()
}

pub fn compare(a: &str, b: &str) -> Option<Ordering> {
    Some(parse(a)?.cmp(&parse(b)?))
}

/// True when `a` is strictly newer than `b`.
pub fn newer(a: &str, b: &str) -> bool {
    compare(a, b) == Some(Ordering::Greater)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordering() {
        assert!(newer("2026.12.01.1", "2026.11.15.1"));
        assert!(newer("2026.11.15.2", "2026.11.15.1"));
        assert!(!newer("2026.11.15.1", "2026.11.15.1"));
        assert!(!newer("2026.11.01.9", "2026.11.15.1"));
        assert!(!valid("2026.11.x"));
        assert!(!valid("../etc"));
    }
}
