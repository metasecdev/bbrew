# bbrew Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build bbrew — a multi-source package manager for macOS x64 with trust scoring, vulnerability patching, and triple interface (TUI/CLI/Web)

**Architecture:** Rust Cargo workspace with 9 crates (core, sources, trust, vuln, resolve, build, cli, tui, web). Sources queried in parallel, trust/vuln checks cascade, resolver builds DAG, installer does atomic moves with rollback. TUI (ratatui) default, CLI (--cli) for scripting, Web (Axum+HTMX) for dashboard.

**Tech Stack:** Rust 1.75+, tokio, clap, ratatui, axum, htmx, tailwind, sqlx (SQLite), sled, reqwest, serde, anyhow, thiserror, cosign, syft

**Spec:** docs/superpowers/specs/2026-11-07-bbrew-design.md

## Global Constraints

- Platform: macOS x64 (Apple Silicon via Rosetta 2), min macOS 12.0
- Language: Rust 1.75+ (edition 2021)
- Binary size: < 50MB single binary
- Startup time: < 100ms cold start
- Concurrency: Full async (tokio), parallel downloads/builds
- No sudo for user installs (default: ~/.local/bbrew)
- Trust threshold: ≥70 overall, ≥80% build success, ≥1000 downloads/dependents
- Vuln cascade: Patch → Warn → Rebuild → Interactive
- Reproducibility: bbrew.toml (manifest) + bbrew.lock (lockfile) + Git sync
- Package format: **Custom tarball with metadata.toml (builder decision: Q1)**
- Binary cache: **GitHub Releases + local cache, optional S3-compatible (builder decision: Q2)**
- Telemetry: **Opt-in only, disabled by default (builder decision: Q3)**
- Homebrew tap: **Yes, publish `brew install bbrew` (user decision: Q4)**

## Review Focus

| Input/Failure Mode | Expected Behavior | Pinned to Task |
|--------------------|-------------------|----------------|
| Source API returns malformed JSON | Graceful degradation, log error, continue with other sources | Task 3 (Source Adapters) |
| Vuln DB unavailable/outdated | Use cached DB, warn user, don't block install | Task 6 (Vuln Scanner) |
| Dependency cycle detected | Clear error with cycle path, suggest manual resolution | Task 7 (Resolver) |
| Build sandbox unavailable (no Docker/VM) | Fall back: container → VM → none, with warning | Task 9 (Build Orchestrator) |
| Lockfile conflict on git merge | Git merge driver for bbrew.lock, manual resolution if needed | Task 11 (Lockfile/Sync) |

---

## Phase 1: Foundation (Tasks 1-5)

### Task 1: Workspace Setup & Core Types

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `crates/bbrew-core/Cargo.toml`
- Create: `crates/bbrew-core/src/lib.rs`
- Create: `crates/bbrew-core/src/types.rs`
- Create: `crates/bbrew-core/src/error.rs`
- Create: `crates/bbrew-core/src/config.rs`
- Test: `crates/bbrew-core/tests/types_test.rs`

**Interfaces:**
- Produces: `Package`, `Version`, `Source`, `Manifest`, `Lockfile`, `Config`, `BbrewError`

```rust
// crates/bbrew-core/src/types.rs
pub struct Package {
    pub name: String,
    pub version: Version,
    pub source: SourceRef,
    pub description: String,
    pub homepage: Option<String>,
    pub license: Option<String>,
    pub dependencies: Vec<Dependency>,
    pub provides: Vec<String>,
    pub conflicts: Vec<String>,
}

pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub pre: Option<String>,
    pub build: Option<String>,
}

pub struct SourceRef {
    pub name: String,      // "homebrew" | "macports" | "nixpkgs" | "github"
    pub url: String,
    pub priority: u8,
}

pub struct Dependency {
    pub name: String,
    pub req: VersionReq,   // semver range
    pub optional: bool,
}

pub struct Manifest {
    pub metadata: ManifestMetadata,
    pub settings: ManifestSettings,
    pub packages: Vec<ManifestPackage>,
}

pub struct Lockfile {
    pub version: u32,
    pub generated_at: DateTime<Utc>,
    pub manifest_hash: String,
    pub packages: Vec<LockPackage>,
}
```

- [ ] **Step 1: Write failing tests for core types**

```rust
// crates/bbrew-core/tests/types_test.rs
#[test] fn test_version_parsing() { ... }
#[test] fn test_version_ordering() { ... }
#[test] fn test_manifest_roundtrip() { ... }
#[test] fn test_lockfile_roundtrip() { ... }
#[test] fn test_dependency_resolution_order() { ... }
```

- [ ] **Step 2: Run tests to verify they fail**
- [ ] **Step 3: Implement types, Version (semver), Manifest, Lockfile, Config**
- [ ] **Step 4: Run tests to verify they pass**
- [ ] **Step 5: Commit**

```bash
git add crates/bbrew-core/
git commit -m "feat(core): add core types, version parsing, manifest/lockfile"
```

---

### Task 2: Configuration System

**Files:**
- Create: `crates/bbrew-core/src/config.rs` (extend)
- Create: `crates/bbrew-core/src/config_schema.rs`
- Test: `crates/bbrew-core/tests/config_test.rs`

**Interfaces:**
- Consumes: `Config` from Task 1
- Produces: `load_config() -> Result<Config>`, `merge_configs(base, override) -> Config`

```rust
// crates/bbrew-core/src/config.rs
pub fn load_config(paths: &[PathBuf]) -> Result<Config> { ... }
pub fn merge_configs(base: Config, override: Config) -> Config { ... }
pub fn default_user_config() -> Config { ... }
pub fn default_system_config() -> Config { ... }
```

- [ ] **Step 1: Write failing tests for config loading/merging**
- [ ] **Step 2: Run tests to verify they fail**
- [ ] **Step 3: Implement config with TOML, layering (defaults → system → user → CLI)**
- [ ] **Step 4: Run tests to verify they pass**
- [ ] **Step 5: Commit**

```bash
git add crates/bbrew-core/src/config*.rs crates/bbrew-core/tests/config_test.rs
git commit -m "feat(core): add configuration system with layering"
```

---

### Task 3: Storage Layer (Package DB, Cache, Lockfile)

**Files:**
- Create: `crates/bbrew-core/src/storage.rs`
- Create: `crates/bbrew-core/src/db.rs`
- Create: `crates/bbrew-core/src/cache.rs`
- Test: `crates/bbrew-core/tests/storage_test.rs`

**Interfaces:**
- Consumes: `Package`, `Lockfile`, `Config` from Tasks 1-2
- Produces: `PackageDb`, `Cache`, `LockfileStore`

```rust
// crates/bbrew-core/src/db.rs
pub struct PackageDb { pool: SqlitePool }
impl PackageDb {
    pub async fn new(path: &Path) -> Result<Self> { ... }
    pub async fn insert(&self, pkg: &InstalledPackage) -> Result<()> { ... }
    pub async fn get(&self, name: &str) -> Result<Option<InstalledPackage>> { ... }
    pub async fn list(&self) -> Result<Vec<InstalledPackage>> { ... }
    pub async fn remove(&self, name: &str) -> Result<()> { ... }
    pub async fn update(&self, pkg: &InstalledPackage) -> Result<()> { ... }
}

// crates/bbrew-core/src/cache.rs
pub struct Cache { dir: PathBuf, max_size: u64 }
impl Cache {
    pub fn new(config: &Config) -> Result<Self> { ... }
    pub async fn get(&self, key: &str) -> Result<Option<Bytes>> { ... }
    pub async fn set(&self, key: &str, value: &[u8]) -> Result<()> { ... }
    pub async fn prune(&self) -> Result<u64> { ... } // bytes freed
}
```

- [ ] **Step 1: Write failing tests for DB operations and cache**
- [ ] **Step 2: Run tests to verify they fail**
- [ ] **Step 3: Implement SQLite (sqlx) for package DB, sled for cache index + files for blobs**
- [ ] **Step 4: Run tests to verify they pass**
- [ ] **Step 5: Commit**

```bash
git add crates/bbrew-core/src/storage.rs crates/bbrew-core/src/db.rs crates/bbrew-core/src/cache.rs
git commit -m "feat(core): add storage layer (SQLite package DB, sled+fs cache)"
```

---

### Task 4: CLI Framework & Commands Skeleton

**Files:**
- Create: `crates/bbrew-cli/Cargo.toml`
- Create: `crates/bbrew-cli/src/main.rs`
- Create: `crates/bbrew-cli/src/commands.rs`
- Create: `crates/bbrew-cli/src/output.rs`
- Test: `crates/bbrew-cli/tests/cli_test.rs`

**Interfaces:**
- Consumes: `Config`, `PackageDb`, `Cache` from core
- Produces: CLI entry point, command registry, output formatters (JSON, table, TUI)

```rust
// crates/bbrew-cli/src/commands.rs
pub enum Command {
    Install(InstallArgs),
    Remove(RemoveArgs),
    Update(UpdateArgs),
    Search(SearchArgs),
    Info(InfoArgs),
    List(ListArgs),
    Sync(SyncArgs),
    Lock(LockArgs),
    Doctor(DoctorArgs),
    Serve(ServeArgs),
    Trust(TrustArgs),
    Vuln(VulnArgs),
    Cache(CacheArgs),
    Config(ConfigArgs),
    Completions(CompletionsArgs),
    SelfUpdate(SelfUpdateArgs),
}

pub async fn execute(cmd: Command, ctx: CliContext) -> Result<()> { ... }

pub struct CliContext {
    pub config: Config,
    pub db: PackageDb,
    pub cache: Cache,
    pub output: OutputFormat,
}
```

- [ ] **Step 1: Write failing tests for command parsing and help output**
- [ ] **Step 2: Run tests to verify they fail**
- [ ] **Step 3: Implement clap command structure, global flags, JSON/table output**
- [ ] **Step 4: Run tests to verify they pass**
- [ ] **Step 5: Commit**

```bash
git add crates/bbrew-cli/
git commit -m "feat(cli): add command framework with clap, JSON/table output"
```

---

### Task 5: Install Script & Self-Update

**Files:**
- Create: `install.sh` (at repo root)
- Create: `crates/bbrew-cli/src/self_update.rs`
- Test: `crates/bbrew-cli/tests/self_update_test.rs`

**Interfaces:**
- Produces: `install.sh` (downloads, verifies, installs binary), `self_update() -> Result<()>`

```bash
# install.sh
# - Detects macOS/arch
# - Downloads from GitHub Releases (bbrew-x86_64-apple-darwin.tar.gz)
# - Verifies SHA256 + cosign signature
# - Installs to ~/.local/bin or /opt/bbrew/bin
# - Updates shell RC files
# - Runs `bbrew doctor`
```

```rust
// crates/bbrew-cli/src/self_update.rs
pub async fn self_update(current_version: &str, channel: Channel) -> Result<()> { ... }
pub enum Channel { Stable, Nightly }
```

- [ ] **Step 1: Write failing test for self-update logic (mock GitHub API)**
- [ ] **Step 2: Run test to verify it fails**
- [ ] **Step 3: Implement install.sh + self-update with signature verification**
- [ ] **Step 4: Run test to verify it passes**
- [ ] **Step 5: Commit**

```bash
git add install.sh crates/bbrew-cli/src/self_update.rs
git commit -m "feat: add install script and self-update with cosign verification"
```

---

## Phase 2: Sources (Tasks 6-9)

### Task 6: Source Adapter Trait & Registry

**Files:**
- Create: `crates/bbrew-sources/Cargo.toml`
- Create: `crates/bbrew-sources/src/lib.rs`
- Create: `crates/bbrew-sources/src/traits.rs`
- Create: `crates/bbrew-sources/src/registry.rs`
- Test: `crates/bbrew-sources/tests/registry_test.rs`

**Interfaces:**
- Consumes: `Package`, `Version`, `SourceRef` from core
- Produces: `PackageSource` trait, `SourceRegistry`, `SourceFactory`

```rust
// crates/bbrew-sources/src/traits.rs
#[async_trait]
pub trait PackageSource: Send + Sync {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<Package>>;
    async fn get_package(&self, name: &str) -> Result<Option<Package>>;
    async fn get_versions(&self, name: &str) -> Result<Vec<VersionInfo>>;
    async fn get_manifest(&self, name: &str, version: &Version) -> Result<PackageManifest>;
    fn name(&self) -> &str;
    fn priority(&self) -> u8;
    fn supports_binary(&self) -> bool { true }
}

pub struct VersionInfo {
    pub version: Version,
    pub url: String,
    pub sha256: Option<String>,
    pub size: Option<u64>,
}
```

```rust
// crates/bbrew-sources/src/registry.rs
pub struct SourceRegistry {
    sources: Vec<Box<dyn PackageSource>>,
}
impl SourceRegistry {
    pub fn new(config: &Config) -> Result<Self> { ... }
    pub async fn search_all(&self, query: &str, limit: usize) -> Result<Vec<SourcePackage>> { ... }
    pub async fn get_best(&self, name: &str, req: &VersionReq) -> Result<Option<SourcePackage>> { ... }
}

pub struct SourcePackage {
    pub package: Package,
    pub source: SourceRef,
    pub trust_score: Option<u8>, // Filled later by trust scorer
}
```

- [ ] **Step 1: Write failing tests for trait and registry**
- [ ] **Step 2: Run tests to verify they fail**
- [ ] **Step 3: Implement trait, registry with parallel search, priority ordering**
- [ ] **Step 4: Run tests to verify they pass**
- [ ] **Step 5: Commit**

```bash
git add crates/bbrew-sources/
git commit -m "feat(sources): add PackageSource trait and SourceRegistry"
```

---

### Task 7: Homebrew Source Adapter

**Files:**
- Create: `crates/bbrew-sources/src/homebrew.rs`
- Test: `crates/bbrew-sources/tests/homebrew_test.rs`

**Interfaces:**
- Implements: `PackageSource` from Task 6
- Consumes: `Config` (for mirror, API token)

```rust
// crates/bbrew-sources/src/homebrew.rs
pub struct HomebrewSource {
    client: reqwest::Client,
    formulae_api: String,  // "https://formulae.brew.sh/api"
    bottles_api: String,
    mirror: Option<String>,
}

impl HomebrewSource {
    pub fn new(config: &Config) -> Result<Self> { ... }
    async fn fetch_formula(&self, name: &str) -> Result<Formula> { ... }
    async fn fetch_bottle(&self, name: &str, version: &Version) -> Result<Bottle> { ... }
}

#[async_trait]
impl PackageSource for HomebrewSource { ... }
```

- [ ] **Step 1: Write failing tests (mock formulae.brew.sh API)**
- [ ] **Step 2: Run tests to verify they fail**
- [ ] **Step 3: Implement Homebrew formulae API parsing, bottle download, version resolution**
- [ ] **Step 4: Run tests to verify they pass**
- [ ] **Step 5: Commit**

```bash
git add crates/bbrew-sources/src/homebrew.rs
git commit -m "feat(sources): add Homebrew source adapter"
```

---

### Task 8: MacPorts Source Adapter

**Files:**
- Create: `crates/bbrew-sources/src/macports.rs`
- Test: `crates/bbrew-sources/tests/macports_test.rs`

**Interfaces:**
- Implements: `PackageSource` from Task 6

```rust
// crates/bbrew-sources/src/macports.rs
pub struct MacPortsSource {
    client: reqwest::Client,
    index_url: String,  // "https://packages.macports.org/portindex.json"
    mirrors: Vec<String>,
}

impl MacPortsSource {
    pub fn new(config: &Config) -> Result<Self> { ... }
    async fn fetch_portindex(&self) -> Result<PortIndex> { ... }
}
```

- [ ] **Step 1: Write failing tests (mock PortIndex)**
- [ ] **Step 2: Run tests to verify they fail**
- [ ] **Step 3: Implement PortIndex parsing, variant handling, source build support**
- [ ] **Step 4: Run tests to verify they pass**
- [ ] **Step 5: Commit**

```bash
git add crates/bbrew-sources/src/macports.rs
git commit -m "feat(sources): add MacPorts source adapter"
```

---

### Task 9: Nixpkgs & GitHub Releases Source Adapters

**Files:**
- Create: `crates/bbrew-sources/src/nixpkgs.rs`
- Create: `crates/bbrew-sources/src/github.rs`
- Test: `crates/bbrew-sources/tests/nixpkgs_test.rs`
- Test: `crates/bbrew-sources/tests/github_test.rs`

**Interfaces:**
- Both implement: `PackageSource` from Task 6

```rust
// crates/bbrew-sources/src/nixpkgs.rs
pub struct NixpkgsSource {
    client: reqwest::Client,
    github_token: Option<String>,
    channel: String,  // "nixos-unstable", "nixos-24.05"
}

// crates/bbrew-sources/src/github.rs
pub struct GitHubSource {
    client: reqwest::Client,
    token: Option<String>,
    asset_patterns: Vec<String>,  // e.g., "*.tar.gz", "*-x86_64-apple-darwin*"
}
```

- [ ] **Step 1: Write failing tests for both (mock search.nixos.org, GitHub API)**
- [ ] **Step 2: Run tests to verify they fail**
- [ ] **Step 3: Implement Nixpkgs (search.nixos.org API, GitHub fallback), GitHub Releases (asset filtering, version extraction from tags)**
- [ ] **Step 4: Run tests to verify they pass**
- [ ] **Step 5: Commit**

```bash
git add crates/bbrew-sources/src/nixpkgs.rs crates/bbrew-sources/src/github.rs
git commit -m "feat(sources): add Nixpkgs and GitHub Releases source adapters"
```

---

## Phase 3: Trust & Vulnerabilities (Tasks 10-13)

### Task 10: Trust Scorer

**Files:**
- Create: `crates/bbrew-trust/Cargo.toml`
- Create: `crates/bbrew-trust/src/lib.rs`
- Create: `crates/bbrew-trust/src/scorer.rs`
- Create: `crates/bbrew-trust/src/github.rs`
- Create: `crates/bbrew-trust/src/registry.rs`
- Create: `crates/bbrew-trust/src/security.rs`
- Create: `crates/bbrew-trust/src/community.rs`
- Create: `crates/bbrew-trust/src/cache.rs`
- Test: `crates/bbrew-trust/tests/scorer_test.rs`

**Interfaces:**
- Consumes: `Package`, `SourceRef` from core
- Produces: `TrustScorer`, `TrustScore`, `TrustCache`

```rust
// crates/bbrew-trust/src/scorer.rs
pub struct TrustScorer {
    github: GitHubClient,
    registry: RegistryClient,
    security: SecurityClient,
    cache: TrustCache,
    config: TrustConfig,
}

impl TrustScorer {
    pub async fn score(&self, pkg: &Package, source: &SourceRef) -> Result<TrustScore> { ... }
    pub async fn score_batch(&self, pkgs: &[(&Package, &SourceRef)]) -> Result<Vec<TrustScore>> { ... }
}

pub struct TrustScore {
    pub overall: u8,           // 0-100
    pub github: GitHubMetrics,
    pub registry: RegistryMetrics,
    pub security: SecurityMetrics,
    pub community: CommunityMetrics,
    pub passed_threshold: bool, // overall >= config.threshold
}

pub struct TrustConfig {
    pub threshold: u8,           // 70
    pub min_success_rate: f32,   // 0.80
    pub min_downloads: u64,      // 1000
    pub cache_ttl_hours: u64,    // 24
}
```

- [ ] **Step 1: Write failing tests for each metric and composite score**
- [ ] **Step 2: Run tests to verify they fail**
- [ ] **Step 3: Implement GitHub API (stars, activity), registry stats (downloads, success rate), security (CVE check), community (reviews), caching with TTL**
- [ ] **Step 4: Run tests to verify they pass**
- [ ] **Step 5: Commit**

```bash
git add crates/bbrew-trust/
git commit -m "feat(trust): add composite trust scorer with caching"
```

---

### Task 11: Vulnerability Scanner

**Files:**
- Create: `crates/bbrew-vuln/Cargo.toml`
- Create: `crates/bbrew-vuln/src/lib.rs`
- Create: `crates/bbrew-vuln/src/scanner.rs`
- Create: `crates/bbrew-vuln/src/osv.rs`
- Create: `crates/bbrew-vuln/src/ghsa.rs`
- Create: `crates/bbrew-vuln/src/nvd.rs`
- Create: `crates/bbrew-vuln/src/db.rs`
- Create: `crates/bbrew-vuln/src/cascade.rs`
- Test: `crates/bbrew-vuln/tests/scanner_test.rs`

**Interfaces:**
- Consumes: `Package`, `Version`, `TrustScore` from core/trust
- Produces: `VulnScanner`, `VulnReport`, `CascadeAction`

```rust
// crates/bbrew-vuln/src/scanner.rs
pub struct VulnScanner {
    osv: OsvClient,
    ghsa: GhsaClient,
    nvd: NvdClient,
    local_db: LocalVulnDb,
    config: VulnConfig,
}

impl VulnScanner {
    pub async fn scan(&self, pkg: &Package, version: &Version) -> Result<VulnReport> { ... }
    pub async fn scan_batch(&self, pkgs: &[(&Package, &Version)]) -> Result<Vec<VulnReport>> { ... }
    pub async fn update_db(&self) -> Result<()> { ... }
}

pub struct VulnReport {
    pub package: String,
    pub version: Version,
    pub vulnerabilities: Vec<Vulnerability>,
    pub fix_available: Option<Version>,
    pub workaround: Option<String>,
    pub max_severity: Severity,
}

pub struct CascadeAction {
    pub action: CascadeStep,
    pub reason: String,
    pub patched_version: Option<Version>,
}

pub enum CascadeStep {
    AutoPatch,      // 1: Fix available, trust >= threshold
    Warn,           // 2: Show vuln, suggest workaround
    Rebuild,        // 3: Build from source with security flags
    Interactive,    // 4: User chooses
}
```

```rust
// crates/bbrew-vuln/src/cascade.rs
pub fn decide_cascade(report: &VulnReport, trust: &TrustScore, config: &VulnConfig) -> CascadeAction {
    // 1. Auto-patch: fix_available && trust.passed_threshold && config.auto_patch
    // 2. Warn: report.max_severity >= High && config.warn_on_high
    // 3. Rebuild: config.rebuild_on_vuln && source_build_possible
    // 4. Interactive: default
}
```

- [ ] **Step 1: Write failing tests for OSV/GHSA/NVD clients, cascade logic**
- [ ] **Step 2: Run tests to verify they fail**
- [ ] **Step 3: Implement OSV.dev API, GHSA GraphQL, NVD API, local SQLite DB with daily sync, cascade decision logic**
- [ ] **Step 4: Run tests to verify they pass**
- [ ] **Step 5: Commit**

```bash
git add crates/bbrew-vuln/
git commit -m "feat(vuln): add vulnerability scanner with cascade logic"
```

---

### Task 12: SBOM Generation & Supply Chain

**Files:**
- Create: `crates/bbrew-vuln/src/sbom.rs`
- Test: `crates/bbrew-vuln/tests/sbom_test.rs`

**Interfaces:**
- Consumes: Installed package path
- Produces: `SbomGenerator`, SPDX/JSON output

```rust
// crates/bbrew-vuln/src/sbom.rs
pub struct SbomGenerator;
impl SbomGenerator {
    pub fn generate(&self, install_path: &Path) -> Result<Sbom> { ... }
    pub fn generate_for_package(&self, pkg: &InstalledPackage) -> Result<Sbom> { ... }
}

pub struct Sbom {
    pub spdx: String,
    pub packages: Vec<SbomPackage>,
    pub relationships: Vec<SbomRelationship>,
}
```

- [ ] **Step 1: Write failing test (use syft library)**
- [ ] **Step 2: Run test to verify it fails**
- [ ] **Step 3: Implement SBOM generation via syft, store with package metadata**
- [ ] **Step 4: Run test to verify it passes**
- [ ] **Step 5: Commit**

```bash
git add crates/bbrew-vuln/src/sbom.rs
git commit -m "feat(vuln): add SBOM generation with syft"
```

---

### Task 13: Trust & Vuln CLI Commands

**Files:**
- Modify: `crates/bbrew-cli/src/commands.rs` (add trust/vuln subcommands)
- Create: `crates/bbrew-cli/src/trust_cmd.rs`
- Create: `crates/bbrew-cli/src/vuln_cmd.rs`
- Test: `crates/bbrew-cli/tests/trust_vuln_test.rs`

**Interfaces:**
- Consumes: `TrustScorer`, `VulnScanner` from Tasks 10-11
- Produces: `bbrew trust <pkg>`, `bbrew vuln <pkg>` commands

```rust
// crates/bbrew-cli/src/trust_cmd.rs
pub async fn trust_command(args: TrustArgs, ctx: CliContext) -> Result<()> { ... }
// Shows: overall score, breakdown (GitHub/Registry/Security/Community), passed threshold

// crates/bbrew-cli/src/vuln_cmd.rs
pub async fn vuln_command(args: VulnArgs, ctx: CliContext) -> Result<()> { ... }
// Shows: CVEs, fix version, workaround, cascade action
```

- [ ] **Step 1: Write failing tests for CLI output formats**
- [ ] **Step 2: Run tests to verify they fail**
- [ ] **Step 3: Implement trust/vuln commands with JSON/table/TUI output**
- [ ] **Step 4: Run tests to verify they pass**
- [ ] **Step 5: Commit**

```bash
git add crates/bbrew-cli/src/trust_cmd.rs crates/bbrew-cli/src/vuln_cmd.rs
git commit -m "feat(cli): add trust and vuln commands"
```

---

## Phase 4: Resolver & Installer (Tasks 14-18)

### Task 14: Dependency Resolver

**Files:**
- Create: `crates/bbrew-resolve/Cargo.toml`
- Create: `crates/bbrew-resolve/src/lib.rs`
- Create: `crates/bbrew-resolve/src/resolver.rs`
- Create: `crates/bbrew-resolve/src/graph.rs`
- Create: `crates/bbrew-resolve/src/planner.rs`
- Test: `crates/bbrew-resolve/tests/resolver_test.rs`

**Interfaces:**
- Consumes: `SourceRegistry`, `TrustScorer`, `VulnScanner` from Tasks 6, 10, 11
- Produces: `Resolver`, `ResolveResult`, `InstallPlan`

```rust
// crates/bbrew-resolve/src/resolver.rs
pub struct Resolver {
    sources: SourceRegistry,
    trust: TrustScorer,
    vuln: VulnScanner,
    config: ResolverConfig,
}

impl Resolver {
    pub async fn resolve(&self, spec: &PackageSpec) -> Result<ResolveResult> { ... }
    pub async fn resolve_manifest(&self, manifest: &Manifest) -> Result<Vec<ResolveResult>> { ... }
}

pub struct ResolveResult {
    pub package: Package,
    pub source: SourceRef,
    pub version: Version,
    pub dependencies: Vec<DepResolution>,
    pub trust_score: TrustScore,
    pub vuln_report: VulnReport,
    pub cascade_action: CascadeAction,
    pub install_plan: InstallPlan,
}

pub struct DepResolution {
    pub name: String,
    pub req: VersionReq,
    pub resolved: Option<PackageRef>,
    pub trust_ok: bool,
    pub vuln_ok: bool,
    pub action: DepAction,
}

pub enum DepAction {
    Install,
    Skip,
    UseSystem,
    UseAlternative(String),
}
```

```rust
// crates/bbrew-resolve/src/planner.rs
pub struct InstallPlan {
    pub steps: Vec<InstallStep>,
    pub total_size: u64,
    pub estimated_time: Duration,
}

pub struct InstallStep {
    pub package: PackageRef,
    pub action: StepAction,
    pub depends_on: Vec<String>,  // Package names
}

pub enum StepAction {
    DownloadBinary { url: String, sha256: String },
    BuildFromSource { url: String, sha256: String, patches: Vec<Patch> },
    LinkSystem { path: PathBuf },
}
```

- [ ] **Step 1: Write failing tests for resolution, cycle detection, topological sort**
- [ ] **Step 2: Run tests to verify they fail**
- [ ] **Step 3: Implement parallel source queries, trust/vuln checks, cascade, DAG build, topological sort**
- [ ] **Step 4: Run tests to verify they pass**
- [ ] **Step 5: Commit**

```bash
git add crates/bbrew-resolve/
git commit -m "feat(resolve): add dependency resolver with trust/vuln integration"
```

---

### Task 15: Binary Downloader & Verifier

**Files:**
- Create: `crates/bbrew-build/Cargo.toml`
- Create: `crates/bbrew-build/src/lib.rs`
- Create: `crates/bbrew-build/src/downloader.rs`
- Create: `crates/bbrew-build/src/verifier.rs`
- Test: `crates/bbrew-build/tests/downloader_test.rs`

**Interfaces:**
- Consumes: `InstallPlan`, `Cache`, `Config`
- Produces: `Downloader`, `Verifier`, verified artifacts in cache

```rust
// crates/bbrew-build/src/downloader.rs
pub struct Downloader {
    client: reqwest::Client,
    cache: Cache,
    jobs: usize,
}

impl Downloader {
    pub async fn download(&self, plan: &InstallPlan) -> Result<Vec<DownloadedArtifact>> { ... }
}

pub struct DownloadedArtifact {
    pub package: PackageRef,
    pub path: PathBuf,
    pub sha256: String,
    pub size: u64,
}
```

```rust
// crates/bbrew-build/src/verifier.rs
pub struct Verifier;
impl Verifier {
    pub fn verify_sha256(&self, path: &Path, expected: &str) -> Result<()> { ... }
    pub fn verify_signature(&self, path: &Path, sig: &Signature) -> Result<()> { ... }
    pub fn verify_cosign(&self, path: &Path, public_key: &str) -> Result<()> { ... }
}
```

- [ ] **Step 1: Write failing tests for parallel download, hash verification, cosign**
- [ ] **Step 2: Run tests to verify they fail**
- [ ] **Step 3: Implement parallel downloads with resume, SHA256 verification, cosign/sigstore verification**
- [ ] **Step 4: Run tests to verify they pass**
- [ ] **Step 5: Commit**

```bash
git add crates/bbrew-build/src/downloader.rs crates/bbrew-build/src/verifier.rs
git commit -m "feat(build): add binary downloader and verifier"
```

---

### Task 16: Build Orchestrator (Source Builds)

**Files:**
- Create: `crates/bbrew-build/src/orchestrator.rs`
- Create: `crates/bbrew-build/src/sandbox.rs`
- Create: `crates/bbrew-build/src/patches.rs`
- Test: `crates/bbrew-build/tests/orchestrator_test.rs`

**Interfaces:**
- Consumes: `InstallPlan` (BuildFromSource steps), `Config` (sandbox settings)
- Produces: `BuildOrchestrator`, built artifacts

```rust
// crates/bbrew-build/src/orchestrator.rs
pub struct BuildOrchestrator {
    sandbox: SandboxManager,
    cache: BuildCache,
    jobs: usize,
    config: BuildConfig,
}

impl BuildOrchestrator {
    pub async fn build(&self, steps: Vec<BuildStep>) -> Result<Vec<BuiltArtifact>> { ... }
}

pub struct BuildStep {
    pub package: PackageRef,
    pub source_url: String,
    pub sha256: String,
    pub patches: Vec<Patch>,
    pub configure_args: Vec<String>,
    pub make_jobs: usize,
}
```

```rust
// crates/bbrew-build/src/sandbox.rs
pub enum SandboxType { None, Container, VM }

pub struct SandboxManager {
    sandbox_type: SandboxType,
    docker: Option<DockerClient>,
    lima: Option<LimaClient>,
}

impl SandboxManager {
    pub async fn run(&self, cmd: &[&str], workdir: &Path, env: &HashMap<String, String>) -> Result<Output> { ... }
    pub fn available(&self) -> SandboxType { ... } // Detects Docker/VM, falls back
}
```

```rust
// crates/bbrew-build/src/patches.rs
pub struct Patch {
    pub url: String,
    pub sha256: String,
    pub strip: usize,
}
```

- [ ] **Step 1: Write failing tests for sandbox detection, build execution, patch application**
- [ ] **Step 2: Run tests to verify they fail**
- [ ] **Step 3: Implement sandbox auto-detection (Docker → Lima VM → none), secure build flags (-fstack-protector-strong, -D_FORTIFY_SOURCE=2), patch fetching/application, ccache/sccache integration**
- [ ] **Step 4: Run tests to verify they pass**
- [ ] **Step 5: Commit**

```bash
git add crates/bbrew-build/src/orchestrator.rs crates/bbrew-build/src/sandbox.rs crates/bbrew-build/src/patches.rs
git commit -m "feat(build): add build orchestrator with sandbox support"
```

---

### Task 17: Atomic Installer with Rollback

**Files:**
- Create: `crates/bbrew-build/src/installer.rs`
- Create: `crates/bbrew-build/src/rollback.rs`
- Test: `crates/bbrew-build/tests/installer_test.rs`

**Interfaces:**
- Consumes: `DownloadedArtifact` or `BuiltArtifact`, `PackageDb`, `Config`
- Produces: `Installer`, atomic install with rollback on failure

```rust
// crates/bbrew-build/src/installer.rs
pub struct Installer {
    db: PackageDb,
    install_root: PathBuf,
    config: InstallConfig,
}

impl Installer {
    pub async fn install(&self, artifact: &Artifact, plan: &InstallPlan) -> Result<InstalledPackage> { ... }
    pub async fn uninstall(&self, name: &str, cascade: bool) -> Result<()> { ... }
}

pub struct InstalledPackage {
    pub name: String,
    pub version: Version,
    pub source: SourceRef,
    pub install_path: PathBuf,
    pub files: Vec<InstalledFile>,
    pub env: EnvironmentModifications,
    pub trust_score: TrustScore,
    pub vuln_report: VulnReport,
    pub installed_at: DateTime<Utc>,
}
```

```rust
// crates/bbrew-build/src/rollback.rs
pub struct RollbackManager;
impl RollbackManager {
    pub fn snapshot(&self, name: &str) -> Result<Snapshot> { ... }
    pub fn restore(&self, snapshot: Snapshot) -> Result<()> { ... }
}
```

- [ ] **Step 1: Write failing tests for atomic install, rollback on failure, uninstall with cascade**
- [ ] **Step 2: Run tests to verify they fail**
- [ ] **Step 3: Implement atomic moves (rename), snapshot/rollback, environment modifications (PATH, MANPATH), symlink management**
- [ ] **Step 4: Run tests to verify they pass**
- [ ] **Step 5: Commit**

```bash
git add crates/bbrew-build/src/installer.rs crates/bbrew-build/src/rollback.rs
git commit -m "feat(build): add atomic installer with rollback"
```

---

### Task 18: Install/Update/Remove CLI Commands

**Files:**
- Modify: `crates/bbrew-cli/src/commands.rs`
- Create: `crates/bbrew-cli/src/install_cmd.rs`
- Create: `crates/bbrew-cli/src/update_cmd.rs`
- Create: `crates/bbrew-cli/src/remove_cmd.rs`
- Create: `crates/bbrew-cli/src/list_cmd.rs`
- Create: `crates/bbrew-cli/src/search_cmd.rs`
- Create: `crates/bbrew-cli/src/info_cmd.rs`
- Test: `crates/bbrew-cli/tests/install_test.rs`

**Interfaces:**
- Consumes: `Resolver`, `Downloader`, `BuildOrchestrator`, `Installer` from Tasks 14-17
- Produces: Full install/update/remove/search/info/list commands

```rust
// crates/bbrew-cli/src/install_cmd.rs
pub async fn install_command(args: InstallArgs, ctx: CliContext) -> Result<()> { ... }
// Flow: resolve → show plan → confirm → download/build → install → update lockfile

// crates/bbrew-cli/src/update_cmd.rs
pub async fn update_command(args: UpdateArgs, ctx: CliContext) -> Result<()> { ... }
// --all, --security-only, --dry-run

// crates/bbrew-cli/src/remove_cmd.rs
pub async fn remove_command(args: RemoveArgs, ctx: CliContext) -> Result<()> { ... }
// --cascade removes unused deps
```

- [ ] **Step 1: Write failing tests for each command (mock resolver/installer)**
- [ ] **Step 2: Run tests to verify they fail**
- [ ] **Step 3: Implement commands with progress bars, --json output, --yes flag, plan display**
- [ ] **Step 4: Run tests to verify they pass**
- [ ] **Step 5: Commit**

```bash
git add crates/bbrew-cli/src/*_cmd.rs
git commit -m "feat(cli): add install, update, remove, search, info, list commands"
```

---

## Phase 5: Interfaces (Tasks 19-22)

### Task 19: TUI (ratatui)

**Files:**
- Create: `crates/bbrew-tui/Cargo.toml`
- Create: `crates/bbrew-tui/src/main.rs`
- Create: `crates/bbrew-tui/src/app.rs`
- Create: `crates/bbrew-tui/src/views/search.rs`
- Create: `crates/bbrew-tui/src/views/installed.rs`
- Create: `crates/bbrew-tui/src/views/updates.rs`
- Create: `crates/bbrew-tui/src/views/vulns.rs`
- Create: `crates/bbrew-tui/src/views/settings.rs`
- Create: `crates/bbrew-tui/src/components.rs`
- Test: `crates/bbrew-tui/tests/tui_test.rs`

**Interfaces:**
- Consumes: All core crates via shared `AppContext`
- Produces: TUI application, default entry point

```rust
// crates/bbrew-tui/src/app.rs
pub struct App {
    state: AppState,
    context: AppContext,
    current_view: View,
}

pub enum View {
    Search(SearchView),
    Installed(InstalledView),
    Updates(UpdatesView),
    Vulns(VulnsView),
    Settings(SettingsView),
}

pub async fn run_tui(context: AppContext) -> Result<()> { ... }
```

- [ ] **Step 1: Write failing tests for view rendering (snapshot tests)**
- [ ] **Step 2: Run tests to verify they fail**
- [ ] **Step 3: Implement ratatui app with 5 views, keyboard navigation, async search/install, real-time progress**
- [ ] **Step 4: Run tests to verify they pass**
- [ ] **Step 5: Commit**

```bash
git add crates/bbrew-tui/
git commit -m "feat(tui): add terminal UI with search, installed, updates, vulns, settings views"
```

---

### Task 20: Web Dashboard (Axum + HTMX)

**Files:**
- Create: `crates/bbrew-web/Cargo.toml`
- Create: `crates/bbrew-web/src/main.rs`
- Create: `crates/bbrew-web/src/server.rs`
- Create: `crates/bbrew-web/src/routes/packages.rs`
- Create: `crates/bbrew-web/src/routes/api.rs`
- Create: `crates/bbrew-web/src/templates/` (HTML templates)
- Create: `crates/bbrew-web/src/static/` (Tailwind CSS, minimal JS)
- Test: `crates/bbrew-web/tests/web_test.rs`

**Interfaces:**
- Consumes: All core crates via `WebContext`
- Produces: HTTP server, WebSocket for logs, REST API

```rust
// crates/bbrew-web/src/server.rs
pub struct WebServer {
    context: WebContext,
    port: u16,
    host: String,
}

impl WebServer {
    pub async fn serve(&self) -> Result<()> { ... }
}

// Routes:
// GET  /                    → Dashboard
// GET  /packages            → Package list (HTMX)
// GET  /packages/:name      → Package detail
// POST /api/v1/install      → {name, version, source}
// POST /api/v1/remove       → {name}
// POST /api/v1/sync         → Sync from manifest
// GET  /api/v1/health       → System health
// WS   /api/v1/logs/:id     → Build/install logs
```

- [ ] **Step 1: Write failing tests for API endpoints and HTMX partials**
- [ ] **Step 2: Run tests to verify they fail**
- [ ] **Step 3: Implement Axum server, HTMX templates (dashboard, package list, detail), Tailwind styling, WebSocket logs, REST API**
- [ ] **Step 4: Run tests to verify they pass**
- [ ] **Step 5: Commit**

```bash
git add crates/bbrew-web/
git commit -m "feat(web): add web dashboard with HTMX, WebSocket logs, REST API"
```

---

### Task 21: CLI/TUI/Web Integration & Entry Point

**Files:**
- Modify: `crates/bbrew-cli/src/main.rs`
- Create: `crates/bbrew-cli/src/entry.rs`

**Interfaces:**
- Produces: Unified binary with mode selection

```rust
// crates/bbrew-cli/src/entry.rs
pub enum RunMode {
    Cli(Command),
    Tui,
    Web(ServeArgs),
}

pub fn determine_mode(args: &[String]) -> RunMode { ... }
// Default: TUI if stdout is TTY and no args
// --cli flag: CLI mode
// serve subcommand: Web mode
```

- [ ] **Step 1: Write failing tests for mode detection**
- [ ] **Step 2: Run tests to verify they fail**
- [ ] **Step 3: Implement mode detection, shared context creation, dispatch**
- [ ] **Step 4: Run tests to verify they pass**
- [ ] **Step 5: Commit**

```bash
git add crates/bbrew-cli/src/main.rs crates/bbrew-cli/src/entry.rs
git commit -m "feat: unify CLI/TUI/Web entry point with mode detection"
```

---

### Task 22: Serve Command & Doctor

**Files:**
- Create: `crates/bbrew-cli/src/serve_cmd.rs`
- Create: `crates/bbrew-cli/src/doctor_cmd.rs`
- Test: `crates/bbrew-cli/tests/serve_doctor_test.rs`

**Interfaces:**
- `serve`: Starts web server, opens browser
- `doctor`: Health checks (disk, cache, outdated, vulns, config)

```rust
// crates/bbrew-cli/src/serve_cmd.rs
pub async fn serve_command(args: ServeArgs, ctx: CliContext) -> Result<()> { ... }

// crates/bbrew-cli/src/doctor_cmd.rs
pub async fn doctor_command(args: DoctorArgs, ctx: CliContext) -> Result<()> { ... }
// Checks: config valid, db accessible, cache healthy, outdated packages, vulnerabilities, disk space
```

- [ ] **Step 1: Write failing tests**
- [ ] **Step 2: Run tests to verify they fail**
- [ ] **Step 3: Implement serve (launch web, auto-open browser) and doctor (comprehensive health checks with --fix)**
- [ ] **Step 4: Run tests to verify they pass**
- [ ] **Step 5: Commit**

```bash
git add crates/bbrew-cli/src/serve_cmd.rs crates/bbrew-cli/src/doctor_cmd.rs
git commit -m "feat(cli): add serve and doctor commands"
```

---

## Phase 6: Polish & Release (Tasks 23-27)

### Task 23: Sync Command & Lockfile Management

**Files:**
- Create: `crates/bbrew-cli/src/sync_cmd.rs`
- Create: `crates/bbrew-cli/src/lock_cmd.rs`
- Create: `crates/bbrew-core/src/sync.rs`
- Test: `crates/bbrew-cli/tests/sync_lock_test.rs`

**Interfaces:**
- Consumes: `Manifest`, `Lockfile`, `Resolver`, `Installer`, `Git`
- Produces: `sync`, `lock`, `lock check` commands

```rust
// crates/bbrew-core/src/sync.rs
pub struct Syncer {
    resolver: Resolver,
    installer: Installer,
    db: PackageDb,
}

impl Syncer {
    pub async fn sync(&self, manifest: &Manifest, lock: &mut Lockfile, dry_run: bool) -> Result<SyncResult> { ... }
    pub async fn generate_lock(&self, manifest: &Manifest) -> Result<Lockfile> { ... }
    pub fn check_lock(&self, manifest: &Manifest, lock: &Lockfile) -> Result<LockCheck> { ... }
}
```

```rust
// crates/bbrew-cli/src/sync_cmd.rs
pub async fn sync_command(args: SyncArgs, ctx: CliContext) -> Result<()> { ... }
// --lock: update lockfile
// --dry-run: show plan only
// --force: ignore trust/vuln warnings

// crates/bbrew-cli/src/lock_cmd.rs
pub async fn lock_command(args: LockArgs, ctx: CliContext) -> Result<()> { ... }
// generate, check
```

- [ ] **Step 1: Write failing tests for sync, lock generation, lock check**
- [ ] **Step 2: Run tests to verify they fail**
- [ ] **Step 3: Implement sync (manifest → resolve → install → update lock), lock generate/check, git merge driver for bbrew.lock**
- [ ] **Step 4: Run tests to verify they pass**
- [ ] **Step 5: Commit**

```bash
git add crates/bbrew-cli/src/sync_cmd.rs crates/bbrew-cli/src/lock_cmd.rs crates/bbrew-core/src/sync.rs
git commit -m "feat: add sync and lockfile management with git merge driver"
```

---

### Task 24: Cache, Config, Completions Commands

**Files:**
- Create: `crates/bbrew-cli/src/cache_cmd.rs`
- Create: `crates/bbrew-cli/src/config_cmd.rs`
- Create: `crates/bbrew-cli/src/completions_cmd.rs`
- Test: `crates/bbrew-cli/tests/misc_cmd_test.rs`

**Interfaces:**
- `cache`: clean, prune, verify, stats
- `config`: get, set, edit, reset
- `completions`: bash, zsh, fish, powershell

- [ ] **Step 1: Write failing tests**
- [ ] **Step 2: Run tests to verify they fail**
- [ ] **Step 3: Implement all three commands**
- [ ] **Step 4: Run tests to verify they pass**
- [ ] **Step 5: Commit**

```bash
git add crates/bbrew-cli/src/cache_cmd.rs crates/bbrew-cli/src/config_cmd.rs crates/bbrew-cli/src/completions_cmd.rs
git commit -m "feat(cli): add cache, config, completions commands"
```

---

### Task 25: Git-Backed Config Sync

**Files:**
- Create: `crates/bbrew-core/src/git_sync.rs`
- Create: `crates/bbrew-cli/src/git_sync_cmd.rs`
- Test: `crates/bbrew-core/tests/git_sync_test.rs`

**Interfaces:**
- Consumes: `Manifest`, `Lockfile`, `Config`, git2
- Produces: `GitSync`, push/pull/status commands

```rust
// crates/bbrew-core/src/git_sync.rs
pub struct GitSync {
    repo: Repository,
    config: GitSyncConfig,
}

impl GitSync {
    pub async fn push(&self, message: &str) -> Result<()> { ... }
    pub async fn pull(&self) -> Result<()> { ... }
    pub async fn status(&self) -> Result<GitStatus> { ... }
}
```

- [ ] **Step 1: Write failing tests (mock git repo)**
- [ ] **Step 2: Run tests to verify they fail**
- [ ] **Step 3: Implement git operations for bbrew.toml + bbrew.lock, conflict resolution for lockfile**
- [ ] **Step 4: Run tests to verify they pass**
- [ ] **Step 5: Commit**

```bash
git add crates/bbrew-core/src/git_sync.rs crates/bbrew-cli/src/git_sync_cmd.rs
git commit -m "feat: add git-backed config sync"
```

---

### Task 26: Documentation, Man Pages, Homebrew Tap

**Files:**
- Create: `docs/` (markdown docs)
- Create: `man/` (man pages via clap_mangen)
- Create: `Formula/bbrew.rb` (Homebrew tap formula)
- Create: `.github/workflows/release.yml`

**Interfaces:**
- Produces: Documentation site, man pages, Homebrew formula, release automation

```ruby
# Formula/bbrew.rb
class Bbrew < Formula
  desc "Better package manager for macOS - multi-source, trust-scored, vuln-aware"
  homepage "https://github.com/bbrew/bbrew"
  url "https://github.com/bbrew/bbrew/releases/download/v#{version}/bbrew-x86_64-apple-darwin.tar.gz"
  sha256 "..."
  license "MIT"
  
  def install
    bin.install "bbrew"
    # Generate completions
    generate_completions_from_executable(bin/"bbrew", "completions")
  end
end
```

- [ ] **Step 1: Generate man pages from clap**
- [ ] **Step 2: Write docs (install, config, manifest, TUI, web, sync, FAQ)**
- [ ] **Step 3: Create Homebrew formula, test `brew install bbrew`**
- [ ] **Step 4: Create release workflow (build universal binary, sign, upload, update tap)**
- [ ] **Step 5: Commit**

```bash
git add docs/ man/ Formula/ .github/workflows/release.yml
git commit -m "docs: add documentation, man pages, Homebrew tap formula, release workflow"
```

---

### Task 27: Integration Tests & Beta Release Prep

**Files:**
- Create: `tests/integration/` (E2E tests)
- Create: `tests/fixtures/` (mock sources, packages)
- Modify: `Cargo.toml` (workspace test config)

**Interfaces:**
- Full E2E: install → search → update → remove → sync → vuln scan

```rust
// tests/integration/install_flow.rs
#[tokio::test]
async fn test_full_install_flow() {
    // 1. Create temp dir with mock Homebrew/GitHub sources
    // 2. Run `bbrew install ffmpeg` 
    // 3. Verify: resolved from best source, trust scored, vulns checked, installed
    // 4. Verify: bbrew.lock created, package in DB
    // 5. Run `bbrew remove ffmpeg` with --cascade
    // 6. Verify: clean removal
}
```

- [ ] **Step 1: Write E2E tests for critical user flows**
- [ ] **Step 2: Run tests to verify they pass**
- [ ] **Step 3: Run full test suite: `cargo test --workspace`**
- [ ] **Step 4: Run `cargo audit`, `cargo deny`, `cargo clippy -- -D warnings`**
- [ ] **Step 5: Build release binary, verify size < 50MB, startup < 100ms**
- [ ] **Step 6: Commit**

```bash
git add tests/integration/
git commit -m "test: add E2E integration tests, pass all quality gates"
```

---

## Task Dependencies

```
Task 1 (Core Types)
    ├── Task 2 (Config)
    ├── Task 3 (Storage)
    └── Task 4 (CLI Framework)
        ├── Task 5 (Install Script)
        ├── Task 13 (Trust/Vuln CLI) ← Task 10, 11
        └── Task 18 (Install/Update/Remove CLI) ← Task 14, 15, 16, 17

Task 6 (Source Trait)
    ├── Task 7 (Homebrew)
    ├── Task 8 (MacPorts)
    └── Task 9 (Nixpkgs + GitHub)
        └── Task 14 (Resolver) ← Task 10 (Trust), Task 11 (Vuln)
            ├── Task 15 (Downloader)
            ├── Task 16 (Build Orchestrator)
            └── Task 17 (Installer)
                └── Task 18 (Install/Update/Remove CLI)

Task 10 (Trust Scorer) ← Task 6
Task 11 (Vuln Scanner) ← Task 6
Task 12 (SBOM) ← Task 11

Task 19 (TUI) ← Task 18
Task 20 (Web) ← Task 18
Task 21 (Entry Point) ← Task 19, 20
Task 22 (Serve/Doctor) ← Task 20, 21

Task 23 (Sync/Lock) ← Task 14, 18
Task 24 (Cache/Config/Completions) ← Task 18
Task 25 (Git Sync) ← Task 23
Task 26 (Docs/Tap) ← Task 21
Task 27 (E2E Tests) ← All
```

---

## Review Focus Tests Added

| Review Focus | Test Added To |
|--------------|---------------|
| Malformed source JSON | Task 7, 8, 9 (source adapter tests) |
| Vuln DB unavailable | Task 11 (scanner_test.rs: test_scan_with_stale_db) |
| Dependency cycle | Task 14 (resolver_test.rs: test_cycle_detection) |
| Sandbox fallback | Task 16 (orchestrator_test.rs: test_sandbox_fallback) |
| Lockfile merge conflict | Task 23 (sync_lock_test.rs: test_lockfile_merge_conflict) |

---

## Estimated Timeline

| Phase | Tasks | Weeks |
|-------|-------|-------|
| 1: Foundation | 1-5 | 3 |
| 2: Sources | 6-9 | 3 |
| 3: Trust & Vulns | 10-13 | 3 |
| 4: Resolver & Installer | 14-18 | 3 |
| 5: Interfaces | 19-22 | 3 |
| 6: Polish & Release | 23-27 | 2 |
| **Total** | **27 tasks** | **~17 weeks** |

---

**Plan complete and saved to `docs/superpowers/plans/2026-11-07-bbrew-implementation.md`.**

Please review the plan. Which execution approach would you prefer?

- **Subagent-driven** - A fresh subagent implements each task and a fresh reviewer checks it before the next one starts, then a whole-branch review at the end. Most thorough; costs a fresh context per task and per review.
- **Native** - I implement every task myself in this session, the way this harness runs work, then one fresh reviewer on the most capable model checks the whole branch. Cheapest and fastest; no independent review until the end.

**For this plan I recommend Subagent-driven, because** the 27 tasks have complex cross-crate dependencies (resolver needs trust+vuln+sources, installer needs downloader+build+db) and independent review at each boundary will catch interface mismatches early. A shipped mistake in the resolver or trust scoring would cascade to all commands.

Does the plan capture what you want, and which approach should we use?