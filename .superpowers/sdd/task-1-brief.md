# Task 1: Workspace Setup & Core Types

## Context
First task in Phase 1 (Foundation). Establishes the Cargo workspace and core types used by all subsequent tasks.

## Requirements (from plan)

### Files to Create
- `Cargo.toml` (workspace root)
- `crates/bbrew-core/Cargo.toml`
- `crates/bbrew-core/src/lib.rs`
- `crates/bbrew-core/src/types.rs`
- `crates/bbrew-core/src/error.rs`
- `crates/bbrew-core/src/config.rs`
- `crates/bbrew-core/tests/types_test.rs`

### Interfaces to Produce
- `Package`, `Version`, `SourceRef`, `Dependency`, `Manifest`, `Lockfile`, `Config`, `BbrewError`

### Exact Values from Spec
- Platform: macOS x64, min macOS 12.0
- Language: Rust 1.75+ (edition 2021)
- Version: semver parsing (major.minor.patch-pre+build)
- Manifest: bbrew.toml with metadata, settings, packages array
- Lockfile: bbrew.lock with version, generated_at, manifest_hash, packages array
- Config: TOML with install, trust, vulnerabilities, sources, web, cache sections

### Test Cases (must pass)
```rust
#[test] fn test_version_parsing() { ... }
#[test] fn test_version_ordering() { ... }
#[test] fn test_manifest_roundtrip() { ... }
#[test] fn test_lockfile_roundtrip() { ... }
#[test] fn test_dependency_resolution_order() { ... }
```

### Implementation Notes
- Use `semver` crate for Version
- Use `serde` with `toml` for config/manifest/lockfile
- Use `thiserror` for BbrewError
- Use `anyhow` for Result type alias
- Config structs must match spec exactly (install.scope, trust.threshold, etc.)

## Report File
Write report to: `.superpowers/sdd/task-1-report.md`

## Completion Criteria
- All 5 tests pass
- Workspace builds with `cargo build --workspace`
- Types compile and are usable by downstream crates
