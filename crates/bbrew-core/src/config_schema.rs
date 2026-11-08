use crate::types::*;
use crate::error::{BbrewError, Result};

/// Validation result for a config field
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

impl ValidationError {
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

/// Validates an InstallConfig
pub fn validate_install_config(config: &InstallConfig) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    if config.parallel_jobs == 0 {
        errors.push(ValidationError::new(
            "install.parallel_jobs",
            "parallel_jobs must be greater than 0",
        ));
    }

    if config.parallel_jobs > 256 {
        errors.push(ValidationError::new(
            "install.parallel_jobs",
            "parallel_jobs must not exceed 256",
        ));
    }

    if let Some(ref source) = config.default_source {
        if source.is_empty() {
            errors.push(ValidationError::new(
                "install.default_source",
                "default_source cannot be empty",
            ));
        }
    }

    errors
}

/// Validates a TrustConfig
pub fn validate_trust_config(config: &TrustConfig) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    if config.threshold > 100 {
        errors.push(ValidationError::new(
            "trust.threshold",
            "threshold must be between 0 and 100",
        ));
    }

    if !(0.0..=1.0).contains(&config.min_success_rate) {
        errors.push(ValidationError::new(
            "trust.min_success_rate",
            "min_success_rate must be between 0.0 and 1.0",
        ));
    }

    if config.min_downloads == 0 {
        errors.push(ValidationError::new(
            "trust.min_downloads",
            "min_downloads must be greater than 0",
        ));
    }

    if config.cache_ttl_hours == 0 {
        errors.push(ValidationError::new(
            "trust.cache_ttl_hours",
            "cache_ttl_hours must be greater than 0",
        ));
    }

    errors
}

/// Validates a VulnerabilityConfig
pub fn validate_vulnerability_config(config: &VulnerabilityConfig) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    if config.db_update_interval_hours == 0 {
        errors.push(ValidationError::new(
            "vulnerabilities.db_update_interval_hours",
            "db_update_interval_hours must be greater than 0",
        ));
    }

    if config.db_update_interval_hours > 168 {
        errors.push(ValidationError::new(
            "vulnerabilities.db_update_interval_hours",
            "db_update_interval_hours must not exceed 168 (one week)",
        ));
    }

    errors
}

/// Validates a SourceConfig
pub fn validate_source_config(name: &str, config: &SourceConfig) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    if config.priority < -100 || config.priority > 100 {
        errors.push(ValidationError::new(
            format!("sources.{}.priority", name),
            "priority must be between -100 and 100",
        ));
    }

    if let Some(ref mirror) = config.mirror {
        if mirror.is_empty() {
            errors.push(ValidationError::new(
                format!("sources.{}.mirror", name),
                "mirror cannot be empty if set",
            ));
        } else if !mirror.starts_with("http://") && !mirror.starts_with("https://") {
            errors.push(ValidationError::new(
                format!("sources.{}.mirror", name),
                "mirror must be a valid HTTP/HTTPS URL",
            ));
        }
    }

    errors
}

/// Validates a SourcesConfig
pub fn validate_sources_config(config: &SourcesConfig) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    errors.extend(validate_source_config("homebrew", &config.homebrew));
    errors.extend(validate_source_config("macports", &config.macports));
    errors.extend(validate_source_config("nixpkgs", &config.nixpkgs));
    errors.extend(validate_source_config("github", &config.github));

    // At least one source must be enabled
    if !config.homebrew.enabled
        && !config.macports.enabled
        && !config.nixpkgs.enabled
        && !config.github.enabled
    {
        errors.push(ValidationError::new(
            "sources",
            "at least one source must be enabled",
        ));
    }

    errors
}

/// Validates a WebConfig
pub fn validate_web_config(config: &WebConfig) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    if config.port == 0 {
        errors.push(ValidationError::new(
            "web.port",
            "port must be greater than 0",
        ));
    }

    if config.host.is_empty() {
        errors.push(ValidationError::new(
            "web.host",
            "host cannot be empty",
        ));
    }

    errors
}

/// Validates a CacheConfig
pub fn validate_cache_config(config: &CacheConfig) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    if config.max_size_gb == 0 {
        errors.push(ValidationError::new(
            "cache.max_size_gb",
            "max_size_gb must be greater than 0",
        ));
    }

    if config.max_size_gb > 1024 {
        errors.push(ValidationError::new(
            "cache.max_size_gb",
            "max_size_gb must not exceed 1024 (1 TB)",
        ));
    }

    if config.prune_after_days == 0 {
        errors.push(ValidationError::new(
            "cache.prune_after_days",
            "prune_after_days must be greater than 0",
        ));
    }

    if let Some(ref dir) = config.dir {
        if dir.is_empty() {
            errors.push(ValidationError::new(
                "cache.dir",
                "dir cannot be empty if set",
            ));
        }
    }

    errors
}

/// Validates the entire Config
pub fn validate_config(config: &Config) -> Result<()> {
    let mut all_errors = Vec::new();

    all_errors.extend(validate_install_config(&config.install));
    all_errors.extend(validate_trust_config(&config.trust));
    all_errors.extend(validate_vulnerability_config(&config.vulnerabilities));
    all_errors.extend(validate_sources_config(&config.sources));
    all_errors.extend(validate_web_config(&config.web));
    all_errors.extend(validate_cache_config(&config.cache));

    if !all_errors.is_empty() {
        let messages: Vec<String> = all_errors
            .iter()
            .map(|e| format!("{}: {}", e.field, e.message))
            .collect();
        return Err(BbrewError::config(messages.join("; ")).into());
    }

    Ok(())
}

/// Validates and returns a list of warnings (non-fatal issues)
pub fn validate_config_warnings(config: &Config) -> Vec<String> {
    let mut warnings = Vec::new();

    // Warn if no default source is set
    if config.install.default_source.is_none() {
        warnings.push("install.default_source is not set; will use first enabled source".to_string());
    }

    // Warn if trust threshold is very low
    if config.trust.threshold < 30 {
        warnings.push("trust.threshold is very low (< 30); may allow untrusted packages".to_string());
    }

    // Warn if vulnerability scanning is disabled
    if !config.vulnerabilities.scan_on_install || !config.vulnerabilities.scan_on_update {
        warnings.push("vulnerability scanning is disabled for install or update".to_string());
    }

    // Warn if cache dir is not set
    if config.cache.dir.is_none() {
        warnings.push("cache.dir is not set; will use default system cache directory".to_string());
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_install_config() {
        let config = InstallConfig::default();
        let errors = validate_install_config(&config);
        assert!(errors.is_empty());

        let mut config = InstallConfig::default();
        config.parallel_jobs = 0;
        let errors = validate_install_config(&config);
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.field == "install.parallel_jobs"));
    }

    #[test]
    fn test_validate_trust_config() {
        let config = TrustConfig::default();
        let errors = validate_trust_config(&config);
        assert!(errors.is_empty());

        let mut config = TrustConfig::default();
        config.threshold = 101;
        let errors = validate_trust_config(&config);
        assert!(!errors.is_empty());

        let mut config = TrustConfig::default();
        config.min_success_rate = 1.5;
        let errors = validate_trust_config(&config);
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_validate_vulnerability_config() {
        let config = VulnerabilityConfig::default();
        let errors = validate_vulnerability_config(&config);
        assert!(errors.is_empty());

        let mut config = VulnerabilityConfig::default();
        config.db_update_interval_hours = 0;
        let errors = validate_vulnerability_config(&config);
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_validate_source_config() {
        let config = SourceConfig::default();
        let errors = validate_source_config("homebrew", &config);
        assert!(errors.is_empty());

        let mut config = SourceConfig::default();
        config.priority = 101;
        let errors = validate_source_config("homebrew", &config);
        assert!(!errors.is_empty());

        let mut config = SourceConfig::default();
        config.mirror = Some("not-a-url".to_string());
        let errors = validate_source_config("homebrew", &config);
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_validate_sources_config() {
        let config = SourcesConfig::default();
        let errors = validate_sources_config(&config);
        assert!(errors.is_empty());

        let mut config = SourcesConfig::default();
        config.homebrew.enabled = false;
        config.macports.enabled = false;
        config.nixpkgs.enabled = false;
        config.github.enabled = false;
        let errors = validate_sources_config(&config);
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.field == "sources"));
    }

    #[test]
    fn test_validate_web_config() {
        let config = WebConfig::default();
        let errors = validate_web_config(&config);
        assert!(errors.is_empty());

        let mut config = WebConfig::default();
        config.port = 0;
        let errors = validate_web_config(&config);
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_validate_cache_config() {
        let config = CacheConfig::default();
        let errors = validate_cache_config(&config);
        assert!(errors.is_empty());

        let mut config = CacheConfig::default();
        config.max_size_gb = 0;
        let errors = validate_cache_config(&config);
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_validate_config() {
        let config = Config::default();
        assert!(validate_config(&config).is_ok());

        let mut config = Config::default();
        config.install.parallel_jobs = 0;
        assert!(validate_config(&config).is_err());
    }
}