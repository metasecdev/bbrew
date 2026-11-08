use semver::Version;
use serde::{Deserialize, Serialize};

/// A software package with metadata
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub version: Version,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub repository: Option<String>,
    pub license: Option<String>,
    pub dependencies: Vec<Dependency>,
    pub source: SourceRef,
}

/// Reference to a package source (registry, git, local path, etc.)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SourceRef {
    Registry { name: String },
    Git { url: String, rev: Option<String>, tag: Option<String>, branch: Option<String> },
    Path { path: String },
    Url { url: String, sha256: String },
}

/// A dependency requirement
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dependency {
    pub name: String,
    pub version_req: String,
    pub optional: bool,
    pub features: Vec<String>,
}

/// The manifest file (bbrew.toml) describing a project
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub metadata: ManifestMetadata,
    pub settings: Option<ManifestSettings>,
    pub packages: Vec<ManifestPackage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestMetadata {
    pub name: String,
    pub version: Version,
    pub description: Option<String>,
    pub authors: Vec<String>,
    pub license: Option<String>,
    pub repository: Option<String>,
    pub homepage: Option<String>,
    pub keywords: Vec<String>,
    pub categories: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ManifestSettings {
    pub install: Option<InstallSettings>,
    pub build: Option<BuildSettings>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallSettings {
    pub scope: InstallScope,
    pub prefix: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallScope {
    User,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildSettings {
    pub jobs: Option<usize>,
    pub target: Option<String>,
    pub features: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestPackage {
    pub name: String,
    pub version: Option<String>,
    pub source: Option<SourceRef>,
    pub features: Vec<String>,
    pub optional: bool,
    pub platform: Option<PlatformSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformSpec {
    pub os: Option<Vec<String>>,
    pub arch: Option<Vec<String>>,
    pub min_macos_version: Option<String>,
}

/// The lockfile (bbrew.lock) with resolved dependencies
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lockfile {
    pub version: u32,
    pub generated_at: chrono::DateTime<chrono::Utc>,
    pub manifest_hash: String,
    pub packages: Vec<LockfilePackage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockfilePackage {
    pub name: String,
    pub version: Version,
    pub source: SourceRef,
    pub dependencies: Vec<LockfileDependency>,
    pub checksum: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockfileDependency {
    pub name: String,
    pub version_req: String,
}

/// Configuration for bbrew (global/user config)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub install: InstallConfig,
    pub trust: TrustConfig,
    pub vulnerabilities: VulnerabilityConfig,
    pub sources: SourcesConfig,
    pub web: WebConfig,
    pub cache: CacheConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            install: InstallConfig::default(),
            trust: TrustConfig::default(),
            vulnerabilities: VulnerabilityConfig::default(),
            sources: SourcesConfig::default(),
            web: WebConfig::default(),
            cache: CacheConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallConfig {
    pub scope: InstallScope,
    pub default_source: Option<String>,
    pub prefer_binary: bool,
    pub parallel_jobs: usize,
    pub sandbox: bool,
}

impl Default for InstallConfig {
    fn default() -> Self {
        Self {
            scope: InstallScope::User,
            default_source: None,
            prefer_binary: true,
            parallel_jobs: num_cpus::get(),
            sandbox: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrustConfig {
    pub threshold: u32,
    pub min_success_rate: f64,
    pub min_downloads: u64,
    pub require_positive_reviews: bool,
    pub cache_ttl_hours: u32,
}

impl Default for TrustConfig {
    fn default() -> Self {
        Self {
            threshold: 70,
            min_success_rate: 0.80,
            min_downloads: 1000,
            require_positive_reviews: true,
            cache_ttl_hours: 24,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VulnerabilityConfig {
    pub auto_patch: bool,
    pub warn_on_high: bool,
    pub block_critical: bool,
    pub scan_on_install: bool,
    pub scan_on_update: bool,
    pub db_update_interval_hours: u32,
}

impl Default for VulnerabilityConfig {
    fn default() -> Self {
        Self {
            auto_patch: false,
            warn_on_high: true,
            block_critical: true,
            scan_on_install: true,
            scan_on_update: true,
            db_update_interval_hours: 12,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceConfig {
    pub enabled: bool,
    pub mirror: Option<String>,
    pub priority: i32,
}

impl Default for SourceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            mirror: None,
            priority: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourcesConfig {
    pub homebrew: SourceConfig,
    pub macports: SourceConfig,
    pub nixpkgs: SourceConfig,
    pub github: SourceConfig,
}

impl Default for SourcesConfig {
    fn default() -> Self {
        Self {
            homebrew: SourceConfig { enabled: true, mirror: None, priority: 100 },
            macports: SourceConfig { enabled: true, mirror: None, priority: 80 },
            nixpkgs: SourceConfig { enabled: true, mirror: None, priority: 70 },
            github: SourceConfig { enabled: true, mirror: None, priority: 90 },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebConfig {
    pub port: u16,
    pub host: String,
    pub auto_open: bool,
    pub tls: bool,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            port: 8080,
            host: "127.0.0.1".to_string(),
            auto_open: false,
            tls: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheConfig {
    pub dir: Option<String>,
    pub max_size_gb: u64,
    pub prune_after_days: u32,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            dir: None,
            max_size_gb: 10,
            prune_after_days: 30,
        }
    }
}


impl Lockfile {
    pub fn compute_manifest_hash(manifest: &Manifest) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        let manifest_str = toml::to_string(manifest).expect("Failed to serialize manifest");
        hasher.update(manifest_str.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_parsing() {
        let v = Version::parse("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert!(v.pre.is_empty());
        assert!(v.build.is_empty());

        let v = Version::parse("2.0.0-alpha.1").unwrap();
        assert_eq!(v.major, 2);
        assert_eq!(v.minor, 0);
        assert_eq!(v.patch, 0);
        assert_eq!(v.pre.to_string(), "alpha.1");

        let v = Version::parse("1.0.0+build.123").unwrap();
        assert_eq!(v.build.to_string(), "build.123");
    }

    #[test]
    fn test_version_ordering() {
        let v1 = Version::parse("1.0.0").unwrap();
        let v2 = Version::parse("1.0.1").unwrap();
        let v3 = Version::parse("1.1.0").unwrap();
        let v4 = Version::parse("2.0.0").unwrap();
        let v5 = Version::parse("1.0.0-alpha").unwrap();
        let v6 = Version::parse("1.0.0-alpha.1").unwrap();
        let v7 = Version::parse("1.0.0-beta").unwrap();
        let v8 = Version::parse("1.0.0-rc.1").unwrap();

        assert!(v1 < v2);
        assert!(v2 < v3);
        assert!(v3 < v4);
        assert!(v5 < v1);
        assert!(v5 < v6);
        assert!(v6 < v7);
        assert!(v7 < v8);
        assert!(v8 < v1);
    }

    #[test]
    fn test_manifest_roundtrip() {
        let manifest = Manifest {
            metadata: ManifestMetadata {
                name: "test-project".to_string(),
                version: Version::parse("1.0.0").unwrap(),
                description: Some("A test project".to_string()),
                authors: vec!["Test Author <test@example.com>".to_string()],
                license: Some("MIT".to_string()),
                repository: Some("https://github.com/test/test".to_string()),
                homepage: Some("https://test.example.com".to_string()),
                keywords: vec!["test".to_string(), "example".to_string()],
                categories: vec!["development".to_string()],
            },
            settings: Some(ManifestSettings {
                install: Some(InstallSettings {
                    scope: InstallScope::User,
                    prefix: Some("/usr/local".to_string()),
                }),
                build: Some(BuildSettings {
                    jobs: Some(4),
                    target: Some("x86_64-apple-darwin".to_string()),
                    features: vec!["feature1".to_string()],
                }),
            }),
            packages: vec![
                ManifestPackage {
                    name: "dep1".to_string(),
                    version: Some("^1.0".to_string()),
                    source: None,
                    features: vec![],
                    optional: false,
                    platform: None,
                },
                ManifestPackage {
                    name: "dep2".to_string(),
                    version: Some("~2.5".to_string()),
                    source: Some(SourceRef::Registry { name: "crates.io".to_string() }),
                    features: vec!["extra".to_string()],
                    optional: true,
                    platform: Some(PlatformSpec {
                        os: Some(vec!["macos".to_string()]),
                        arch: Some(vec!["x86_64".to_string()]),
                        min_macos_version: Some("12.0".to_string()),
                    }),
                },
            ],
        };

        let toml_str = toml::to_string(&manifest).unwrap();
        let parsed: Manifest = toml::from_str(&toml_str).unwrap();
        assert_eq!(manifest, parsed);
    }

    #[test]
    fn test_lockfile_roundtrip() {
        let lockfile = Lockfile {
            version: 1,
            generated_at: chrono::Utc::now(),
            manifest_hash: "abc123".to_string(),
            packages: vec![
                LockfilePackage {
                    name: "pkg1".to_string(),
                    version: Version::parse("1.0.0").unwrap(),
                    source: SourceRef::Registry { name: "crates.io".to_string() },
                    dependencies: vec![
                        LockfileDependency {
                            name: "dep1".to_string(),
                            version_req: "^1.0".to_string(),
                        },
                    ],
                    checksum: Some("sha256:abcdef".to_string()),
                },
                LockfilePackage {
                    name: "pkg2".to_string(),
                    version: Version::parse("2.0.0").unwrap(),
                    source: SourceRef::Git {
                        url: "https://github.com/example/pkg2".to_string(),
                        rev: Some("abc123".to_string()),
                        tag: None,
                        branch: None,
                    },
                    dependencies: vec![],
                    checksum: None,
                },
            ],
        };

        let toml_str = toml::to_string(&lockfile).unwrap();
        let parsed: Lockfile = toml::from_str(&toml_str).unwrap();
        assert_eq!(lockfile, parsed);
    }

    #[test]
    fn test_dependency_resolution_order() {
        let mut deps = vec![
            Dependency {
                name: "c".to_string(),
                version_req: "^1.0".to_string(),
                optional: false,
                features: vec![],
            },
            Dependency {
                name: "a".to_string(),
                version_req: "^2.0".to_string(),
                optional: false,
                features: vec![],
            },
            Dependency {
                name: "b".to_string(),
                version_req: "^1.5".to_string(),
                optional: true,
                features: vec!["extra".to_string()],
            },
        ];

        deps.sort_by(|a, b| a.name.cmp(&b.name));

        assert_eq!(deps[0].name, "a");
        assert_eq!(deps[1].name, "b");
        assert_eq!(deps[2].name, "c");
        assert!(!deps[0].optional);
        assert!(deps[1].optional);
        assert_eq!(deps[1].features, vec!["extra"]);
    }
}