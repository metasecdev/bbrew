use crate::types::{Config, InstallScope, TrustThreshold, SeverityThreshold, RegistryConfig};
use crate::error::{BbrewError, Result};
use std::path::{Path, PathBuf};
use toml;

fn config_path() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| BbrewError::config("Could not find home directory"))?;
    Ok(home.join(".config").join("bbrew").join("config.toml"))
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = config_path()?;
        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            let config: Config = toml::from_str(&content)?;
            Ok(config)
        } else {
            Ok(Config::default())
        }
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let config_path = config_path()?;
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&config_path, content)?;
        Ok(())
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn get_registry(&self, name: &str) -> Option<&RegistryConfig> {
        self.sources.registries.get(name)
    }

    pub fn default_registry(&self) -> Option<&RegistryConfig> {
        self.sources.default_registry.as_ref()
            .and_then(|name| self.sources.registries.get(name))
    }

    pub fn add_registry(&mut self, name: String, registry: RegistryConfig) {
        self.sources.registries.insert(name, registry);
    }

    pub fn set_default_registry(&mut self, name: String) {
        self.sources.default_registry = Some(name);
    }
}

impl Default for InstallScope {
    fn default() -> Self {
        InstallScope::User
    }
}

impl Default for TrustThreshold {
    fn default() -> Self {
        TrustThreshold::Signed
    }
}

impl Default for SeverityThreshold {
    fn default() -> Self {
        SeverityThreshold::Medium
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.install.scope, InstallScope::User);
        assert_eq!(config.trust.threshold, TrustThreshold::Signed);
        assert_eq!(config.vulnerabilities.severity_threshold, SeverityThreshold::Medium);
        assert!(config.web.timeout_seconds > 0);
        assert_eq!(config.cache.max_size_mb, 500);
    }

    #[test]
    fn test_config_roundtrip() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("config.toml");

        let mut config = Config::default();
        config.install.scope = InstallScope::System;
        config.install.prefix = Some("/opt/bbrew".to_string());
        config.trust.threshold = TrustThreshold::Verified;
        config.trust.trusted_keys = vec!["key1".to_string(), "key2".to_string()];
        config.vulnerabilities.severity_threshold = SeverityThreshold::High;
        config.sources.default_registry = Some("my-registry".to_string());
        config.sources.registries.insert(
            "my-registry".to_string(),
            RegistryConfig {
                url: "https://registry.example.com".to_string(),
                token: Some("secret-token".to_string()),
                priority: 10,
            },
        );

        config.save_to(&config_path).unwrap();
        let loaded = Config::load_from(&config_path).unwrap();

        assert_eq!(config, loaded);
    }

    #[test]
    fn test_config_registry_management() {
        let mut config = Config::default();
        config.add_registry(
            "test-reg".to_string(),
            RegistryConfig {
                url: "https://test.example.com".to_string(),
                token: None,
                priority: 5,
            },
        );
        config.set_default_registry("test-reg".to_string());

        assert!(config.get_registry("test-reg").is_some());
        assert_eq!(config.default_registry().unwrap().url, "https://test.example.com");
    }
}