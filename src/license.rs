pub fn license_family(license: &str) -> &str {
    let license = license.trim();
    match license.rsplit_once('-') {
        Some((head, tail)) if is_version(tail) => head,
        _ => license,
    }
}

fn is_version(value: &str) -> bool {
    let mut parts = value.split('.');
    let (Some(major), Some(minor), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    !major.is_empty()
        && !minor.is_empty()
        && major.chars().all(|c| c.is_ascii_digit())
        && minor.chars().all(|c| c.is_ascii_digit())
}

pub fn is_free(license: &str) -> bool {
    let family = license_family(license);
    ["cc0", "pdm", "cc-by", "cc-by-sa"]
        .iter()
        .any(|free| family.eq_ignore_ascii_case(free))
}

pub fn is_allowed(license: &str, allowed: &[&str]) -> bool {
    let family = license_family(license);
    allowed
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(family))
}

pub fn requires_attribution(license: &str) -> bool {
    license_family(license)
        .get(..5)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("cc-by"))
}

pub fn requires_sharealike(license: &str) -> bool {
    license_family(license).eq_ignore_ascii_case("cc-by-sa")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn families_ignore_version_suffix() {
        assert_eq!(license_family("cc-by-sa-4.0"), "cc-by-sa");
        assert_eq!(license_family("cc-by-3.0"), "cc-by");
        assert_eq!(license_family("cc0"), "cc0");
        assert_eq!(license_family("cc-by-nc-sa-3.0"), "cc-by-nc-sa");
    }

    #[test]
    fn non_free_licenses_are_rejected() {
        assert!(is_free("cc-by-sa-4.0"));
        assert!(!is_free("cc-by-nc-4.0"));
        assert!(!is_free("cc-by-nd-4.0"));
        assert!(!is_free("gfdl"));
        assert!(is_allowed("cc-by-4.0", &["cc-by", "cc0"]));
        assert!(!is_allowed("cc-by-nc-4.0", &["cc-by"]));
    }

    #[test]
    fn attribution_and_sharealike() {
        assert!(requires_attribution("cc-by-4.0"));
        assert!(!requires_attribution("cc0"));
        assert!(requires_sharealike("cc-by-sa-4.0"));
        assert!(!requires_sharealike("cc-by-4.0"));
    }

    #[test]
    fn checks_ignore_case() {
        assert!(is_free("CC-BY-4.0"));
        assert!(is_free("CC0"));
        assert!(requires_attribution("CC-BY-SA-4.0"));
        assert!(requires_sharealike("CC-BY-SA-4.0"));
        assert!(is_allowed("CC-BY-4.0", &["cc-by"]));
    }
}
