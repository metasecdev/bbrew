use thiserror::Error;
use anyhow::Result as AnyhowResult;

pub type Result<T> = AnyhowResult<T>;

#[derive(Error, Debug)]
pub enum BbrewError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Manifest error: {0}")]
    Manifest(String),

    #[error("Lockfile error: {0}")]
    Lockfile(String),

    #[error("Package not found: {0}")]
    PackageNotFound(String),

    #[error("Version conflict: {0}")]
    VersionConflict(String),

    #[error("Dependency resolution failed: {0}")]
    ResolutionFailed(String),

    #[error("Source error: {0}")]
    Source(String),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("TOML serialize error: {0}")]
    TomlSerialize(#[from] toml::ser::Error),

    #[error("Semver error: {0}")]
    Semver(#[from] semver::Error),

    #[error("Checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    #[error("Untrusted package: {0}")]
    UntrustedPackage(String),

    #[error("Vulnerability found: {0}")]
    Vulnerability(String),

    #[error("Platform not supported: {0}")]
    PlatformNotSupported(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Already installed: {0}")]
    AlreadyInstalled(String),

    #[error("Not installed: {0}")]
    NotInstalled(String),

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl BbrewError {
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }

    pub fn manifest(msg: impl Into<String>) -> Self {
        Self::Manifest(msg.into())
    }

    pub fn lockfile(msg: impl Into<String>) -> Self {
        Self::Lockfile(msg.into())
    }

    pub fn package_not_found(name: impl Into<String>) -> Self {
        Self::PackageNotFound(name.into())
    }

    pub fn version_conflict(msg: impl Into<String>) -> Self {
        Self::VersionConflict(msg.into())
    }

    pub fn resolution_failed(msg: impl Into<String>) -> Self {
        Self::ResolutionFailed(msg.into())
    }

    pub fn source(msg: impl Into<String>) -> Self {
        Self::Source(msg.into())
    }

    pub fn checksum_mismatch(expected: impl Into<String>, actual: impl Into<String>) -> Self {
        Self::ChecksumMismatch {
            expected: expected.into(),
            actual: actual.into(),
        }
    }

    pub fn untrusted_package(msg: impl Into<String>) -> Self {
        Self::UntrustedPackage(msg.into())
    }

    pub fn vulnerability(msg: impl Into<String>) -> Self {
        Self::Vulnerability(msg.into())
    }

    pub fn platform_not_supported(msg: impl Into<String>) -> Self {
        Self::PlatformNotSupported(msg.into())
    }

    pub fn permission_denied(msg: impl Into<String>) -> Self {
        Self::PermissionDenied(msg.into())
    }

    pub fn already_installed(name: impl Into<String>) -> Self {
        Self::AlreadyInstalled(name.into())
    }

    pub fn not_installed(name: impl Into<String>) -> Self {
        Self::NotInstalled(name.into())
    }

    pub fn invalid_argument(msg: impl Into<String>) -> Self {
        Self::InvalidArgument(msg.into())
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = BbrewError::PackageNotFound("test-package".to_string());
        assert_eq!(err.to_string(), "Package not found: test-package");

        let err = BbrewError::checksum_mismatch("abc123", "def456");
        assert_eq!(err.to_string(), "Checksum mismatch: expected abc123, got def456");
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let bbrew_err: BbrewError = io_err.into();
        assert!(matches!(bbrew_err, BbrewError::Io(_)));
    }
}