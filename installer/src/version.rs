//! App versions as "major.minor.patch[.build]". Shared with build.rs (via `#[path]`), so the
//! version Windows shows in the file properties and the one setup compares are read the same way.

/// "1.2.10" -> [1, 2, 10, 0]; missing or non-numeric parts count as 0, parts past the fourth are ignored.
pub fn version_parts(v: &str) -> [u64; 4] {
    let mut parts = [0; 4];
    for (slot, p) in parts.iter_mut().zip(v.split('.')) {
        *slot = p.trim().parse().unwrap_or(0);
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_numerically() {
        assert!(version_parts("0.1.10") > version_parts("0.1.9"));
        assert!(version_parts("0.2") > version_parts("0.1.9"));
        assert_eq!(version_parts("0.1.1"), version_parts("0.1.1"));
    }

    #[test]
    fn missing_version_parts_count_as_zero() {
        assert_eq!(version_parts("0.1"), version_parts("0.1.0"));
        assert_eq!(version_parts("1.2.3"), [1, 2, 3, 0]);
    }
}
