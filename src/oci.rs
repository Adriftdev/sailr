#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum OciError {
    #[error("Digest must start with sha256:")]
    MissingSha256Prefix,
    #[error("SHA-256 digest must contain exactly 64 hexadecimal characters")]
    InvalidDigestLength,
    #[error("SHA-256 digest must contain lowercase ASCII hexadecimal characters")]
    InvalidDigestCharacters,
    #[error("Repository component must be lowercase and repository-safe")]
    InvalidRepositoryComponent,
    #[error("Tag must be 1-128 OCI-safe ASCII characters")]
    InvalidTag,
}

pub fn validate_sha256_digest(value: &str) -> Result<(), OciError> {
    let hex = value
        .strip_prefix("sha256:")
        .ok_or(OciError::MissingSha256Prefix)?;

    if hex.len() != 64 {
        return Err(OciError::InvalidDigestLength);
    }

    if !hex.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')) {
        return Err(OciError::InvalidDigestCharacters);
    }

    Ok(())
}

pub fn validate_repository_component(value: &str) -> Result<(), OciError> {
    let bytes = value.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_lowercase() && !bytes[0].is_ascii_digit() {
        return Err(OciError::InvalidRepositoryComponent);
    }

    let mut previous_was_separator = false;
    for byte in bytes {
        if byte.is_ascii_lowercase() || byte.is_ascii_digit() {
            previous_was_separator = false;
        } else if matches!(byte, b'.' | b'_' | b'-') && !previous_was_separator {
            previous_was_separator = true;
        } else {
            return Err(OciError::InvalidRepositoryComponent);
        }
    }

    if previous_was_separator {
        return Err(OciError::InvalidRepositoryComponent);
    }
    Ok(())
}

pub fn validate_tag(value: &str) -> Result<(), OciError> {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() > 128 {
        return Err(OciError::InvalidTag);
    }
    if !bytes[0].is_ascii_alphanumeric() && bytes[0] != b'_' {
        return Err(OciError::InvalidTag);
    }
    if !bytes
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
    {
        return Err(OciError::InvalidTag);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_only_exact_lowercase_ascii_sha256_digests() {
        assert!(validate_sha256_digest(&format!("sha256:{}", "a".repeat(64))).is_ok());
        assert_eq!(
            validate_sha256_digest(&"a".repeat(64)),
            Err(OciError::MissingSha256Prefix)
        );
        assert_eq!(
            validate_sha256_digest(&format!("sha256:{}", "a".repeat(63))),
            Err(OciError::InvalidDigestLength)
        );
        assert_eq!(
            validate_sha256_digest(&format!("sha256:{}", "A".repeat(64))),
            Err(OciError::InvalidDigestCharacters)
        );
        assert!(validate_sha256_digest(&format!("sha256:{}", "１".repeat(64))).is_err());
    }

    #[test]
    fn validates_repository_components() {
        for valid in ["api", "api-worker", "api.worker", "api_worker", "api2"] {
            assert!(validate_repository_component(valid).is_ok());
        }
        for invalid in [
            "",
            "API",
            "api worker",
            "/api",
            "api/",
            "api//worker",
            "api@worker",
            "api:worker",
            "api--worker",
            "-api",
            "api-",
        ] {
            assert_eq!(
                validate_repository_component(invalid),
                Err(OciError::InvalidRepositoryComponent)
            );
        }
    }

    #[test]
    fn validates_bounded_oci_tags() {
        for valid in ["release", "Release_1.2-rc", "_internal"] {
            assert!(validate_tag(valid).is_ok());
        }
        for invalid in [
            "",
            "release:candidate",
            "release@prod",
            "release/candidate",
            "-leading",
        ] {
            assert_eq!(validate_tag(invalid), Err(OciError::InvalidTag));
        }
        assert!(validate_tag(&"a".repeat(128)).is_ok());
        assert_eq!(validate_tag(&"a".repeat(129)), Err(OciError::InvalidTag));
    }
}
