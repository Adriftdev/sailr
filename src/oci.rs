use crate::workflow::error::ArtifactError;

pub fn validate_sha256_digest(value: &str) -> Result<(), ArtifactError> {
    let hex = value
        .strip_prefix("sha256:")
        .ok_or_else(|| ArtifactError::Validation("digest must start with sha256:".to_string()))?;

    if hex.len() != 64 {
        return Err(ArtifactError::Validation(
            "sha256 digest must contain exactly 64 hexadecimal characters".to_string(),
        ));
    }

    if !hex.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')) {
        return Err(ArtifactError::Validation(
            "sha256 digest must contain lowercase ASCII hexadecimal characters".to_string(),
        ));
    }

    Ok(())
}
