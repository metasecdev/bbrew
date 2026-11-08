use bbrew_core::types::*;
use semver::Version;

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

#[test]
fn test_source_ref_serialization() {
    let registry = SourceRef::Registry { name: "crates.io".to_string() };
    let toml_str = toml::to_string(&registry).unwrap();
    assert!(toml_str.contains("type = \"registry\""));
    assert!(toml_str.contains("name = \"crates.io\""));

    let git = SourceRef::Git {
        url: "https://github.com/test/repo".to_string(),
        rev: Some("abc123".to_string()),
        tag: Some("v1.0.0".to_string()),
        branch: None,
    };
    let toml_str = toml::to_string(&git).unwrap();
    assert!(toml_str.contains("type = \"git\""));
    assert!(toml_str.contains("url = \"https://github.com/test/repo\""));
    assert!(toml_str.contains("rev = \"abc123\""));
    assert!(toml_str.contains("tag = \"v1.0.0\""));

    let path = SourceRef::Path { path: "/local/path".to_string() };
    let toml_str = toml::to_string(&path).unwrap();
    assert!(toml_str.contains("type = \"path\""));
    assert!(toml_str.contains("path = \"/local/path\""));

    let url = SourceRef::Url {
        url: "https://example.com/pkg.tar.gz".to_string(),
        sha256: "abcdef123456".to_string(),
    };
    let toml_str = toml::to_string(&url).unwrap();
    assert!(toml_str.contains("type = \"url\""));
    assert!(toml_str.contains("sha256 = \"abcdef123456\""));
}

#[test]
fn test_config_defaults() {
    let config = Config::default();
    assert_eq!(config.install.scope, InstallScope::User);
    assert_eq!(config.trust.threshold, TrustThreshold::Signed);
    assert_eq!(config.vulnerabilities.severity_threshold, SeverityThreshold::Medium);
    assert!(config.web.timeout_seconds > 0);
    assert_eq!(config.cache.max_size_mb, 500);
}

#[test]
fn test_lockfile_manifest_hash() {
    let manifest = Manifest {
        metadata: ManifestMetadata {
            name: "test".to_string(),
            version: Version::parse("1.0.0").unwrap(),
            description: None,
            authors: vec![],
            license: None,
            repository: None,
            homepage: None,
            keywords: vec![],
            categories: vec![],
        },
        settings: None,
        packages: vec![],
    };

    let hash1 = Lockfile::compute_manifest_hash(&manifest);
    let hash2 = Lockfile::compute_manifest_hash(&manifest);
    assert_eq!(hash1, hash2);
    assert_eq!(hash1.len(), 64); // SHA256 hex string

    let mut manifest2 = manifest.clone();
    manifest2.metadata.version = Version::parse("2.0.0").unwrap();
    let hash3 = Lockfile::compute_manifest_hash(&manifest2);
    assert_ne!(hash1, hash3);
}