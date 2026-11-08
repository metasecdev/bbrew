use crate::types::*;
use crate::error::{BbrewError, Result};
use crate::config_schema::{validate_config, validate_config_warnings};
use std::path::{Path, PathBuf};
use toml;

/// Default user config path: ~/.config/bbrew/config.toml
pub fn user_config_path() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| BbrewError::config("Could not find home directory"))?;
    Ok(home.join(".config").join("bbrew").join("config.toml"))
}

/// Default system config path: /etc/bbrew/config.toml
pub fn system_config_path() -> PathBuf {
    PathBuf::from("/etc/bbrew/config.toml")
}

/// Returns the default user-level configuration
pub fn default_user_config() -> Config {
    Config::default()
}

/// Returns the default system-level configuration
pub fn default_system_config() -> Config {
    let mut config = Config::default();
    config.install.scope = InstallScope::System;
    config.install.sandbox = true;
    config.cache.dir = Some("/var/cache/bbrew".to_string());
    config.web.host = "0.0.0.0".to_string();
    config
}

/// Merges two configurations, with `override` taking precedence over `base`
/// Only non-None/non-default values from override are applied
pub fn merge_configs(base: Config, r#override: Config) -> Config {
    Config {
        install: merge_install_config(base.install, r#override.install),
        trust: merge_trust_config(base.trust, r#override.trust),
        vulnerabilities: merge_vulnerability_config(base.vulnerabilities, r#override.vulnerabilities),
        sources: merge_sources_config(base.sources, r#override.sources),
        web: merge_web_config(base.web, r#override.web),
        cache: merge_cache_config(base.cache, r#override.cache),
    }
}

fn merge_install_config(base: InstallConfig, r#override: InstallConfig) -> InstallConfig {
    let default = InstallConfig::default();
    InstallConfig {
        scope: if r#override.scope != default.scope {
            r#override.scope
        } else {
            base.scope
        },
        default_source: r#override.default_source.or(base.default_source),
        prefer_binary: r#override.prefer_binary,
        parallel_jobs: if r#override.parallel_jobs != default.parallel_jobs {
            r#override.parallel_jobs
        } else {
            base.parallel_jobs
        },
        sandbox: r#override.sandbox,
    }
}

fn merge_trust_config(base: TrustConfig, r#override: TrustConfig) -> TrustConfig {
    let default = TrustConfig::default();
    TrustConfig {
        threshold: if r#override.threshold != default.threshold {
            r#override.threshold
        } else {
            base.threshold
        },
        min_success_rate: if (r#override.min_success_rate - default.min_success_rate).abs() > f64::EPSILON {
            r#override.min_success_rate
        } else {
            base.min_success_rate
        },
        min_downloads: if r#override.min_downloads != default.min_downloads {
            r#override.min_downloads
        } else {
            base.min_downloads
        },
        require_positive_reviews: r#override.require_positive_reviews,
        cache_ttl_hours: if r#override.cache_ttl_hours != default.cache_ttl_hours {
            r#override.cache_ttl_hours
        } else {
            base.cache_ttl_hours
        },
    }
}

fn merge_vulnerability_config(base: VulnerabilityConfig, r#override: VulnerabilityConfig) -> VulnerabilityConfig {
    let default = VulnerabilityConfig::default();
    VulnerabilityConfig {
        auto_patch: r#override.auto_patch,
        warn_on_high: r#override.warn_on_high,
        block_critical: r#override.block_critical,
        scan_on_install: r#override.scan_on_install,
        scan_on_update: r#override.scan_on_update,
        db_update_interval_hours: if r#override.db_update_interval_hours != default.db_update_interval_hours {
            r#override.db_update_interval_hours
        } else {
            base.db_update_interval_hours
        },
    }
}

fn merge_source_config(base: SourceConfig, r#override: SourceConfig) -> SourceConfig {
    let default = SourceConfig::default();
    SourceConfig {
        enabled: r#override.enabled,
        mirror: r#override.mirror.or(base.mirror),
        priority: if r#override.priority != default.priority {
            r#override.priority
        } else {
            base.priority
        },
    }
}

fn merge_sources_config(base: SourcesConfig, r#override: SourcesConfig) -> SourcesConfig {
    SourcesConfig {
        homebrew: merge_source_config(base.homebrew, r#override.homebrew),
        macports: merge_source_config(base.macports, r#override.macports),
        nixpkgs: merge_source_config(base.nixpkgs, r#override.nixpkgs),
        github: merge_source_config(base.github, r#override.github),
    }
}

fn merge_web_config(base: WebConfig, r#override: WebConfig) -> WebConfig {
    let default = WebConfig::default();
    WebConfig {
        port: if r#override.port != default.port {
            r#override.port
        } else {
            base.port
        },
        host: if r#override.host != default.host {
            r#override.host
        } else {
            base.host
        },
        auto_open: r#override.auto_open,
        tls: r#override.tls,
    }
}

fn merge_cache_config(base: CacheConfig, r#override: CacheConfig) -> CacheConfig {
    let default = CacheConfig::default();
    CacheConfig {
        dir: r#override.dir.or(base.dir),
        max_size_gb: if r#override.max_size_gb != default.max_size_gb {
            r#override.max_size_gb
        } else {
            base.max_size_gb
        },
        prune_after_days: if r#override.prune_after_days != default.prune_after_days {
            r#override.prune_after_days
        } else {
            base.prune_after_days
        },
    }
}

/// Loads configuration from multiple paths, merging them in order (last wins)
/// Priority: defaults < system < user < CLI (paths provided in order of increasing priority)
pub fn load_config(paths: &[PathBuf]) -> Result<Config> {
    let mut config = Config::default();

    for path in paths {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            let layer: Config = toml::from_str(&content)?;
            config = merge_configs(config, layer);
        }
    }

    // Validate the final merged configuration
    validate_config(&config)?;

    Ok(config)
}

/// Loads configuration with standard layer paths (system, user)
/// Does not include CLI args - caller should add those paths
pub fn load_standard_config() -> Result<Config> {
    let system_path = system_config_path();
    let user_path = user_config_path()?;
    load_config(&[system_path, user_path])
}

/// Loads a config from a single path
pub fn load_config_from(path: &Path) -> Result<Config> {
    let content = std::fs::read_to_string(path)?;
    let config: Config = toml::from_str(&content)?;
    validate_config(&config)?;
    Ok(config)
}

impl Config {
    pub fn save(&self) -> Result<()> {
        let config_path = user_config_path()?;
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

    pub fn get_source(&self, name: &str) -> Option<&SourceConfig> {
        match name {
            "homebrew" => Some(&self.sources.homebrew),
            "macports" => Some(&self.sources.macports),
            "nixpkgs" => Some(&self.sources.nixpkgs),
            "github" => Some(&self.sources.github),
            _ => None,
        }
    }

    pub fn get_source_mut(&mut self, name: &str) -> Option<&mut SourceConfig> {
        match name {
            "homebrew" => Some(&mut self.sources.homebrew),
            "macports" => Some(&mut self.sources.macports),
            "nixpkgs" => Some(&mut self.sources.nixpkgs),
            "github" => Some(&mut self.sources.github),
            _ => None,
        }
    }

    /// Returns validation warnings for this config
    pub fn warnings(&self) -> Vec<String> {
        validate_config_warnings(self)
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
        assert_eq!(config.install.parallel_jobs, num_cpus::get());
        assert_eq!(config.trust.threshold, 70);
        assert_eq!(config.vulnerabilities.db_update_interval_hours, 12);
        assert_eq!(config.web.port, 8080);
        assert_eq!(config.cache.max_size_gb, 10);
    }

    #[test]
    fn test_default_user_config() {
        let config = default_user_config();
        assert_eq!(config.install.scope, InstallScope::User);
        assert!(!config.install.sandbox);
    }

    #[test]
    fn test_default_system_config() {
        let config = default_system_config();
        assert_eq!(config.install.scope, InstallScope::System);
        assert!(config.install.sandbox);
        assert_eq!(config.cache.dir, Some("/var/cache/bbrew".to_string()));
        assert_eq!(config.web.host, "0.0.0.0");
    }

    #[test]
    fn test_merge_configs() {
        let mut base = Config::default();
        base.install.scope = InstallScope::User;
        base.install.parallel_jobs = 4;
        base.trust.threshold = 50;
        base.web.port = 8080;

        let mut r#override = Config::default();
        r#override.install.scope = InstallScope::System;
        r#override.install.parallel_jobs = 8;
        r#override.trust.threshold = 80;
        r#override.web.port = 9090;

        let merged = merge_configs(base, r#override);

        assert_eq!(merged.install.scope, InstallScope::System);
        assert_eq!(merged.install.parallel_jobs, 8);
        assert_eq!(merged.trust.threshold, 80);
        assert_eq!(merged.web.port, 9090);
    }

    #[test]
    fn test_merge_preserves_non_overlapping() {
        let mut base = Config::default();
        base.install.default_source = Some("homebrew".to_string());
        base.trust.min_downloads = 500;
        base.cache.dir = Some("/custom/cache".to_string());

        let mut r#override = Config::default();
        r#override.install.scope = InstallScope::System;

        let merged = merge_configs(base, r#override);

        assert_eq!(merged.install.default_source, Some("homebrew".to_string()));
        assert_eq!(merged.trust.min_downloads, 500);
        assert_eq!(merged.cache.dir, Some("/custom/cache".to_string()));
        assert_eq!(merged.install.scope, InstallScope::System);
    }

    #[test]
    fn test_load_config_layering() {
        let dir = tempdir().unwrap();

        // System config
        let system_path = dir.path().join("system.toml");
        let mut system_config = Config::default();
        system_config.install.scope = InstallScope::System;
        system_config.install.parallel_jobs = 2;
        system_config.trust.threshold = 60;
        system_config.web.port = 8080;
        system_config.save_to(&system_path).unwrap();

        // User config - use values different from defaults
        let user_path = dir.path().join("user.toml");
        let mut user_config = Config::default();
        user_config.install.parallel_jobs = 8; // Different from typical default (num_cpus)
        user_config.trust.threshold = 80; // Different from default (70)
        user_config.web.port = 9090;
        user_config.save_to(&user_path).unwrap();

        // Load with layering: system then user (user wins for explicitly set values)
        let loaded = load_config(&[system_path.clone(), user_path.clone()]).unwrap();

        // System scope wins because user config didn't explicitly override (uses default)
        assert_eq!(loaded.install.scope, InstallScope::System); // from system (user didn't override)
        assert_eq!(loaded.install.parallel_jobs, 8); // from user (explicitly set)
        assert_eq!(loaded.trust.threshold, 80); // from user (explicitly set)
        assert_eq!(loaded.web.port, 9090); // from user (explicitly set)
    }

    #[test]
    fn test_load_config_missing_files() {
        let dir = tempdir().unwrap();
        let missing_path = dir.path().join("missing.toml");

        // Should not fail on missing files, just use defaults
        let loaded = load_config(&[missing_path]).unwrap();
        assert_eq!(loaded, Config::default());
    }

    #[test]
    fn test_load_standard_config() {
        // This will use actual system/user paths, may not exist
        // Just ensure it doesn't panic
        let _ = load_standard_config();
    }

    #[test]
    fn test_config_validation() {
        let mut config = Config::default();
        assert!(validate_config(&config).is_ok());

        config.install.parallel_jobs = 0;
        assert!(validate_config(&config).is_err());

        config.install.parallel_jobs = 4;
        config.trust.threshold = 101;
        assert!(validate_config(&config).is_err());

        config.trust.threshold = 70;
        config.trust.min_success_rate = 1.5;
        assert!(validate_config(&config).is_err());

        config.trust.min_success_rate = 0.8;
        config.vulnerabilities.db_update_interval_hours = 0;
        assert!(validate_config(&config).is_err());

        config.vulnerabilities.db_update_interval_hours = 12;
        config.sources.homebrew.enabled = false;
        config.sources.macports.enabled = false;
        config.sources.nixpkgs.enabled = false;
        config.sources.github.enabled = false;
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn test_config_warnings() {
        let config = Config::default();
        let warnings = config.warnings();
        assert!(!warnings.is_empty()); // Should warn about default_source not set

        let mut config = Config::default();
        config.install.default_source = Some("homebrew".to_string());
        config.trust.threshold = 50;
        config.vulnerabilities.scan_on_install = true;
        config.vulnerabilities.scan_on_update = true;
        config.cache.dir = Some("/tmp/cache".to_string());
        let warnings = config.warnings();
        assert!(warnings.len() < config.warnings().len() || warnings.is_empty());
    }

    #[test]
    fn test_source_config_access() {
        let mut config = Config::default();

        let source = config.get_source("homebrew");
        assert!(source.is_some());
        assert!(source.unwrap().enabled);

        let source = config.get_source("nonexistent");
        assert!(source.is_none());

        let source = config.get_source_mut("homebrew");
        assert!(source.is_some());
        source.unwrap().enabled = false;

        let source = config.get_source("homebrew");
        assert!(source.is_some());
        assert!(!source.unwrap().enabled);
    }

    #[test]
    fn test_config_roundtrip() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("config.toml");

        let mut config = Config::default();
        config.install.scope = InstallScope::System;
        config.install.default_source = Some("homebrew".to_string());
        config.install.prefer_binary = false;
        config.install.parallel_jobs = 8;
        config.install.sandbox = true;
        config.trust.threshold = 80;
        config.trust.min_success_rate = 0.9;
        config.trust.min_downloads = 5000;
        config.trust.require_positive_reviews = false;
        config.trust.cache_ttl_hours = 48;
        config.vulnerabilities.auto_patch = true;
        config.vulnerabilities.warn_on_high = false;
        config.vulnerabilities.block_critical = true;
        config.vulnerabilities.scan_on_install = true;
        config.vulnerabilities.scan_on_update = false;
        config.vulnerabilities.db_update_interval_hours = 24;
        config.sources.homebrew.enabled = true;
        config.sources.homebrew.mirror = Some("https://mirror.example.com".to_string());
        config.sources.homebrew.priority = 10;
        config.sources.macports.enabled = false;
        config.sources.nixpkgs.enabled = true;
        config.sources.nixpkgs.priority = 5;
        config.sources.github.enabled = true;
        config.web.port = 9090;
        config.web.host = "0.0.0.0".to_string();
        config.web.auto_open = true;
        config.web.tls = true;
        config.cache.dir = Some("/custom/cache".to_string());
        config.cache.max_size_gb = 20;
        config.cache.prune_after_days = 60;

        config.save_to(&config_path).unwrap();
        let loaded = load_config_from(&config_path).unwrap();

        assert_eq!(config, loaded);
    }
}