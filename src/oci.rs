#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum OciError {
    #[error("Digest must start with sha256:")]
    MissingSha256Prefix,
    #[error("SHA-256 digest must contain exactly 64 hexadecimal characters")]
    InvalidDigestLength,
    #[error("SHA-256 digest must contain lowercase ASCII hexadecimal characters")]
    InvalidDigestCharacters,
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
}
