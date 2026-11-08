# bbrew Design Document

**Date:** 2026-11-07  
**Status:** Draft for Review  
**Author:** Claude Code (with user input)

---

## 1. Intent & Vision

**bbrew** (Better Brew) is a next-generation package manager for x64 macOS that aggregates packages from multiple sources (Homebrew, MacPorts, Nixpkgs, GitHub Releases), intelligently resolves dependencies with trust scoring, proactively patches known vulnerabilities, and provides multiple interfaces (TUI, CLI, Web Dashboard) for different workflows.

### Why Now?

- Homebrew is excellent but single-source; users often need packages from MacPorts or Nix
- Security vulnerabilities in dependencies are discovered after installation
- No unified trust scoring across package ecosystems
- Reproducibility is hard (no lockfiles, no manifest)
- Modern Rust tooling enables fast, safe, concurrent operations

### Success Looks Like

```
$ bbrew install ffmpeg
→ Found 3 sources: homebrew (v6.1), macports (v6.0), github (v6.1.1)
→ Trust scores: homebrew ★★★★★, macports ★★★★☆, github ★★★☆☆
→ Known issues: homebrew has CVE-2024-XXXX (patched in v6.1.1)
→ Auto-selecting: github (latest, no known vulns, high trust)
→ Installing dependencies: x264, lame, opus (all trusted, >80% build success)
→ [████████████████████] 100% - ffmpeg installed successfully
```

---

## 2. Goals, Non-Goals, Anti-Goals

### Goals (Must Have)

| # | Goal | Description |
|---|------|-------------|
| G1 | Multi-source aggregation | Query Homebrew, MacPorts, Nixpkgs, GitHub Releases simultaneously |
| G2 | Trust scoring | Composite score: GitHub metrics + registry stats + security scanning |
| G3 | Vulnerability detection | Check CVEs, GHSA, OSV databases before install; show known issues |
| G4 | Auto-patch cascade | 1) Patch vulns → 2) Warn → 3) Rebuild with fixes → 4) Interactive choice |
| G5 | Dependency resolution | Install deps if trusted (>80% success, >1000 installs, positive reviews) |
| G6 | Reproducibility | Manifest (bbrew.toml) + Lockfile (bbrew.lock) with exact versions/hashes |
| G7 | Git-backed sync | Track config in git, sync across machines |
| G8 | Dual install scope | User-space (~/.local) default, --system flag for /opt/bbrew |
| G9 | Sandboxed builds | Optional container/VM isolation for building from source |
| G10 | Triple interface | TUI (default), CLI (--cli flag), Web Dashboard (bbrew serve) |

### Non-Goals (Explicitly Out of Scope)

| # | Non-Goal | Reason |
|---|----------|--------|
| NG1 | Replace Homebrew entirely | Complement, not replace; can use brew as a source |
| NG2 | Cross-platform (Linux/Windows) | Focus on macOS x64 excellence first |
| NG3 | Binary cache hosting | Use existing caches (GitHub Releases, etc.) |
| NG4 | GUI app (Electron/Tauri) | Web dashboard covers remote access |
| NG5 | Package publishing | Only consumption, not publication |

### Anti-Goals (Would Be Failure)

| # | Anti-Goal | Why It's Failure |
|---|-----------|------------------|
| AG1 | Slower than Homebrew for common ops | Must be competitive on speed |
| AG2 | Silent failures or hidden issues | Transparency is core value |
| AG3 | Breaking user's existing Homebrew | Must coexist peacefully |
| AG4 | Requiring sudo for normal use | User-space default is mandatory |

---

## 3. Constraints

| Constraint | Detail |
|------------|--------|
| **Platform** | macOS x64 (Apple Silicon via Rosetta 2) |
| **Language** | Rust 1.75+ (edition 2021) |
| **Min macOS** | 12.0 (Monterey) |
| **Dependencies** | Minimal; prefer stdlib + well-maintained crates |
| **Binary size** | < 50MB single binary |
| **Startup time** | < 100ms cold start |
| **Concurrency** | Full async (tokio), parallel downloads/builds |

---

## 4. Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        bbrew CLI/TUI/Web                         │
├─────────────────────────────────────────────────────────────────┤
│  Commands: install, search, update, remove, sync, serve, doctor  │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Core Engine (bbrew-core)                    │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌───────────┐  │
│  │  Resolver   │ │  Trust      │ │  Vuln       │ │  Build    │  │
│  │  Engine     │ │  Scorer     │ │  Scanner    │ │  Orchestr.│  │
│  └─────────────┘ └─────────────┘ └─────────────┘ └───────────┘  │
└─────────────────────────────────────────────────────────────────┘
                              │
              ┌───────────────┼───────────────┐
              ▼               ▼               ▼
       ┌────────────┐ ┌────────────┐ ┌────────────┐
       │  Sources   │ │  Sources   │ │  Sources   │
       │  Registry  │ │  Registry  │ │  Registry  │
       │ (Homebrew) │ │ (MacPorts) │ │ (Nixpkgs)  │
       └────────────┘ └────────────┘ └────────────┘
              │               │               │
              ▼               ▼               ▼
       ┌──────────────────────────────────────────┐
       │           GitHub Releases API            │
       └──────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Storage Layer                               │
│  ~/.local/share/bbrew/  or  /opt/bbrew/                          │
│  ├── packages/          # Installed packages                     │
│  ├── cache/             # Downloaded sources, indexes            │
│  ├── builds/            # Sandboxed build directories            │
│  ├── bbrew.lock         # Lockfile                               │
│  └── trust-cache/       # Cached trust scores                    │
└─────────────────────────────────────────────────────────────────┘
```

---

## 5. Core Components

### 5.1 Source Adapters (bbrew-sources)

Each source implements a common trait:

```rust
#[async_trait]
trait PackageSource: Send + Sync {
    async fn search(&self, query: &str) -> Result<Vec<Package>>;
    async fn get_package(&self, name: &str) -> Result<Option<Package>>;
    async fn get_versions(&self, name: &str) -> Result<Vec<Version>>;
    async fn get_manifest(&self, name: &str, version: &str) -> Result<PackageManifest>;
    fn name(&self) -> &str;
    fn priority(&self) -> u8; // Higher = preferred
}
```

**Sources to implement:**

| Source | Adapter | Notes |
|--------|---------|-------|
| Homebrew | `HomebrewSource` | Parse `brew info --json`, use formulae.brew.sh API |
| MacPorts | `MacPortsSource` | Parse PortIndex, use macports.org API |
| Nixpkgs | `NixpkgsSource` | Use nixpkgs GitHub repo, search.nixos.org API |
| GitHub Releases | `GitHubSource` | GitHub REST/GraphQL API, filter by asset patterns |

### 5.2 Trust Scoring (bbrew-trust)

Composite trust score (0-100) from multiple signals:

```rust
struct TrustScore {
    overall: u8,           // 0-100
    github: GitHubMetrics, // stars, forks, activity, issues
    registry: RegistryMetrics, // downloads, success rate, dependents
    security: SecurityMetrics, // CVEs, advisories, SBOM
    community: CommunityMetrics, // reviews, ratings, maintenance
}

struct GitHubMetrics {
    stars: u32,
    forks: u32,
    open_issues: u32,
    closed_issues_30d: u32,
    last_commit_days: u32,
    contributors: u32,
    has_security_policy: bool,
    // Score: stars>1000=30, active=20, maintained=20, policy=10, contributors=20
}

struct RegistryMetrics {
    downloads_30d: u64,
    build_success_rate: f32,  // Must be >0.8
    dependents_count: u32,    // Must be >1000
    avg_rating: Option<f32>,  // Positive reviews
    // Score: success>80%=30, downloads>1000=25, dependents>1000=25, rating=20
}

struct SecurityMetrics {
    known_cves: Vec<Cve>,
    has_sbom: bool,
    signed_releases: bool,
    supply_chain_score: u8, // SLSA level
    // Score: no critical CVEs=40, SBOM=20, signed=20, SLSA=20
}
```

**Thresholds for auto-install:**
- Overall trust ≥ 70
- Registry success rate ≥ 80%
- Downloads/dependents ≥ 1000
- No unpatched critical CVEs

### 5.3 Vulnerability Scanner (bbrew-vuln)

```rust
struct VulnScanner {
    osv_client: OsvClient,       // osv.dev API
    ghsa_client: GhsaClient,     // GitHub Security Advisories
    nvd_client: NvdClient,       // NIST NVD
    local_db: LocalVulnDb,       // Cached, updated daily
}

struct VulnReport {
    package: PackageRef,
    vulnerabilities: Vec<Vulnerability>,
    fix_available: Option<Version>, // Patched version
    workaround: Option<String>,     // Config flag, env var
    severity: Severity,             // Critical/High/Medium/Low
}
```

**Cascade logic (user's requirement):**

1. **Auto-patch**: If fix_available && trust(fix) ≥ threshold → apply patch/rebuild
2. **Warn**: Show vulnerability, suggest workaround, ask to continue
3. **Rebuild**: Attempt to build from source with security flags (-fstack-protector, etc.)
4. **Interactive**: Present options: [Skip] [Install anyway] [Use alternative] [Report]

### 5.4 Dependency Resolver (bbrew-resolve)

```rust
struct Resolver {
    sources: Vec<Box<dyn PackageSource>>,
    trust_scorer: TrustScorer,
    vuln_scanner: VulnScanner,
    config: ResolverConfig,
}

struct ResolveResult {
    package: PackageRef,
    source: SourceRef,
    version: Version,
    dependencies: Vec<DepResolution>,
    trust_score: TrustScore,
    vuln_report: VulnReport,
    install_plan: InstallPlan,
}

struct DepResolution {
    name: String,
    required_version: VersionReq,
    resolved: Option<PackageRef>,
    trust_ok: bool,
    vuln_ok: bool,
    action: DepAction, // Install, Skip, UseSystem, UseAlternative
}
```

**Resolution algorithm:**
1. Query all sources in parallel for package + versions
2. Score each (version, source) combination
3. Filter by trust thresholds
4. Check vulnerabilities
5. Apply cascade fix logic
6. Build dependency graph
7. Topological sort for install order
8. Present plan to user (or auto-execute with --yes)

### 5.5 Build Orchestrator (bbrew-build)

```rust
struct BuildOrchestrator {
    sandbox: SandboxType, // None, Container, VM
    jobs: usize,          // Parallel builds
    cache: BuildCache,    # ccache, sccache support
}

enum SandboxType {
    None,           // Direct build (fastest, least isolation)
    Container,      // Docker/Podman (good isolation)
    VM,             # Lima/VM (strongest isolation, macOS only)
}
```

**For source builds (MacPorts, Nix, GitHub):**
- Fetch source
- Apply patches (security, macOS compatibility)
- Configure with secure flags
- Build in sandbox
- Verify artifacts (hashes, signatures)
- Package for bbrew store

---

## 6. Data Models

### 6.1 Manifest (bbrew.toml) - User Intent

```toml
# bbrew.toml - Declarative package manifest
[metadata]
name = "my-dev-env"
version = "1.0.0"
description = "Development environment for project X"

[settings]
install_scope = "user"        # user | system
sandbox_builds = false        # true | false | "auto"
prefer_binary = true          # Prefer pre-built over source
auto_patch_vulns = true       # Auto-apply security patches
trust_threshold = 70          # Minimum trust score (0-100)

[[packages]]
name = "ffmpeg"
version = "^6.1"              # Semver range
source = "github"             # Preferred source (optional)
features = ["full"]           # Package-specific options

[[packages]]
name = "neovim"
version = "latest"
source = "homebrew"

[[packages]]
name = "ripgrep"
# No version = latest
# No source = best match

[packages.git-tools]
# Package aliases / groups
members = ["git", "git-delta", "lazygit", "gh"]
```

### 6.2 Lockfile (bbrew.lock) - Reproducible State

```toml
# bbrew.lock - Generated, commit to git
version = 1
generated_at = "2026-11-07T10:30:00Z"
manifest_hash = "sha256:abc123..."  # Hash of bbrew.toml

[[packages]]
name = "ffmpeg"
version = "6.1.1"
source = "github"
source_url = "https://github.com/FFmpeg/FFmpeg/releases/tag/n6.1.1"
sha256 = "sha256:def456..."
dependencies = [
  { name = "x264", version = "20240101", sha256 = "..." },
  { name = "lame", version = "3.100", sha256 = "..." },
  { name = "opus", version = "1.5.1", sha256 = "..." },
]
trust_score = 85
vulns_checked = "2026-11-07T10:30:00Z"
vulns_found = []
installed_at = "2026-11-07T10:30:15Z"
install_path = "/Users/wo/.local/bbrew/packages/ffmpeg/6.1.1"

[[packages]]
name = "neovim"
version = "0.10.1"
source = "homebrew"
# ... similar structure
```

---

## 7. CLI Interface Design

### 7.1 Command Structure

```
bbrew [GLOBAL_OPTS] <COMMAND> [ARGS] [OPTS]

GLOBAL_OPTS:
  --cli              Use classic CLI mode (no TUI)
  --system           Install system-wide (/opt/bbrew)
  --user             Install user-space (~/.local/bbrew) [default]
  --sandbox          Enable sandboxed builds
  --no-sandbox       Disable sandboxed builds
  -y, --yes          Auto-confirm all prompts
  -v, --verbose      Verbose output
  -q, --quiet        Quiet output
  --json             JSON output for scripting
  --config PATH      Custom config file
  --manifest PATH    Custom manifest file (default: bbrew.toml)
```

### 7.2 Commands

| Command | Description | Key Options |
|---------|-------------|-------------|
| `install <pkg>...` | Install packages | `--source`, `--version`, `--no-deps`, `--force` |
| `remove <pkg>...` | Remove packages | `--cascade` (remove unused deps) |
| `update [pkg]...` | Update packages | `--all`, `--dry-run`, `--security-only` |
| `search <query>` | Search packages | `--source`, `--limit`, `--json` |
| `info <pkg>` | Show package details | `--versions`, `--deps`, `--vulns` |
| `list` | List installed packages | `--outdated`, `--json`, `--tree` |
| `sync` | Sync from manifest | `--lock`, `--dry-run`, `--force` |
| `lock` | Generate lockfile | `--check` (verify lockfile matches) |
| `doctor` | Health check | `--fix`, `--verbose` |
| `serve` | Start web dashboard | `--port`, `--host`, `--no-browser` |
| `trust <pkg>` | Show trust breakdown | `--refresh`, `--json` |
| `vuln <pkg>` | Show vulnerability report | `--fix`, `--json` |
| `cache` | Manage caches | `clean`, `prune`, `verify`, `stats` |
| `config` | Manage configuration | `get`, `set`, `edit`, `reset` |
| `completions` | Shell completions | `bash`, `zsh`, `fish`, `powershell` |

### 7.3 TUI Layout (Default Mode)

```
┌─────────────────────────────────────────────────────────────────┐
│ bbrew v0.1.0  ●  User: ~/.local/bbrew  ●  247 packages          │
├──────────────┬──────────────────────────────────────────────────┤
│  SEARCH      │  Results: "ffmpeg"                               │
│  ┌────────┐  │  ┌────────────────────────────────────────────┐  │
│  │ffmpeg  │  │  │ ▸ ffmpeg 6.1.1  (github)  ★★★★★  Trust: 85 │  │
│  │        │  │  │   Latest, no known vulns, 5 deps           │  │
│  │        │  │  ├────────────────────────────────────────────┤  │
│  │        │  │  │ ▸ ffmpeg 6.1    (homebrew) ★★★★★  Trust: 82 │  │
│  │        │  │  │   CVE-2024-XXXX patched in 6.1.1           │  │
│  │        │  │  ├────────────────────────────────────────────┤  │
│  │        │  │  │ ▸ ffmpeg 6.0    (macports) ★★★★☆  Trust: 78 │  │
│  │        │  │  │   Builds from source, slower               │  │
│  │        │  │  └────────────────────────────────────────────┘  │
│  └────────┘  │  [Tab: Details] [Enter: Install] [Esc: Back]    │
├──────────────┼──────────────────────────────────────────────────┤
│  INSTALLED   │  [i] ffmpeg 6.1.1   [o] neovim 0.10.0→0.10.1    │
│  (247)       │  [✓] ripgrep 14.1.0  [✓] fd 10.1.0               │
└──────────────┴──────────────────────────────────────────────────┘
  [1] Search  [2] Installed  [3] Updates  [4] Vulns  [5] Settings
```

---

## 8. Web Dashboard (bbrew serve)

**Tech:** Axum + HTMX + Tailwind (server-rendered, minimal JS)

**Features:**
- Package browser with trust scores, vuln status
- Install/remove/update from browser
- Real-time build logs via WebSocket
- System health: disk, cache, outdated, vulnerabilities
- Manifest editor with live validation
- Sync status across machines
- API for automation: `GET /api/v1/packages`, `POST /api/v1/install`

**Endpoints:**
```
GET  /                    Dashboard
GET  /packages            Package list (filterable, sortable)
GET  /packages/:name      Package detail + versions + vulns
POST /api/v1/install      {name, version, source}
POST /api/v1/remove       {name}
POST /api/v1/sync         Sync from manifest
GET  /api/v1/health       System health
WS   /api/v1/logs/:id     Build/install logs
```

---

## 9. Installation & Bootstrapping

### 9.1 Install Script

```bash
# One-liner install (like Homebrew)
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/bbrew/bbrew/main/install.sh)"
```

**Install script does:**
1. Detect macOS version, arch
2. Download latest bbrew binary (universal x64/arm64)
3. Verify signature/checksum
4. Install to `~/.local/bin` (user) or `/opt/bbrew/bin` (system)
5. Add to PATH (shell rc files)
6. Run `bbrew doctor` to verify
7. Offer to create initial `bbrew.toml`

### 9.2 Self-Update

```bash
bbrew self-update          # Update bbrew itself
bbrew self-update --check  # Check only
bbrew self-update --nightly # Nightly build
```

---

## 10. Configuration

**Config file:** `~/.config/bbrew/config.toml` (or `/etc/bbrew/config.toml` for system)

```toml
[install]
scope = "user"              # user | system
default_source = "auto"     # auto | homebrew | macports | nixpkgs | github
prefer_binary = true
parallel_jobs = 4
sandbox = "auto"            # never | auto | always

[trust]
threshold = 70
min_success_rate = 0.80
min_downloads = 1000
require_positive_reviews = true
cache_ttl_hours = 24

[vulnerabilities]
auto_patch = true
warn_on_high = true
block_critical = false
scan_on_install = true
scan_on_update = true
db_update_interval_hours = 12

[sources]
homebrew = { enabled = true, mirror = "", priority = 100 }
macports = { enabled = true, mirror = "", priority = 80 }
nixpkgs = { enabled = true, mirror = "", priority = 70 }
github = { enabled = true, token = "", priority = 90 }

[web]
port = 8080
host = "127.0.0.1"
auto_open = true
tls = false

[cache]
dir = "~/.cache/bbrew"
max_size_gb = 10
prune_after_days = 30
```

---

## 11. Security Model

### 11.1 Supply Chain Security

- All downloads verified via SHA256 (from lockfile or source metadata)
- GitHub releases verified via cosign/sigstore if available
- Homebrew bottles verified via Homebrew's SHA256
- Nixpkgs verified via Nix's nar hashes
- SBOM generation for all installed packages (Syft)

### 11.2 Runtime Security

- No setuid, no sudo for user installs
- Sandbox builds: network disabled, limited filesystem access
- Package installation: atomic moves, no partial states
- Rollback on failure (keep previous version)

### 11.3 Vulnerability Management

- Daily sync of OSV, GHSA, NVD databases
- Scan on install, update, and scheduled (daily)
- `bbrew vuln` shows all known issues
- `bbrew update --security-only` patches vulns only

---

## 12. Implementation Phases

### Phase 1: Foundation (Weeks 1-3)
- [ ] Project setup: Cargo workspace, CI/CD
- [ ] Core types: Package, Version, Source, Manifest, Lockfile
- [ ] Config system (load, merge, validate)
- [ ] Storage layer (package DB, cache, lockfile)
- [ ] Basic CLI framework (clap) with `--json` output

### Phase 2: Sources (Weeks 3-5)
- [ ] Homebrew source adapter
- [ ] MacPorts source adapter
- [ ] Nixpkgs source adapter
- [ ] GitHub Releases source adapter
- [ ] Unified search across sources
- [ ] Source priority/fallback logic

### Phase 3: Trust & Vulns (Weeks 5-7)
- [ ] Trust scorer (GitHub, registry, security, community)
- [ ] Vulnerability scanner (OSV, GHSA, NVD)
- [ ] Local vuln DB with daily updates
- [ ] Cascade fix logic (patch → warn → rebuild → interactive)
- [ ] Trust/vuln reporting commands

### Phase 4: Resolver & Installer (Weeks 7-9)
- [ ] Dependency resolver with topological sort
- [ ] Install planner (plan → confirm → execute)
- [ ] Binary download & verification
- [ ] Source build orchestrator (with sandbox options)
- [ ] Atomic install with rollback
- [ ] Package database (SQLite or sled)

### Phase 5: Interfaces (Weeks 9-11)
- [ ] TUI (ratatui) with search, install, list, updates views
- [ ] Web dashboard (Axum + HTMX + Tailwind)
- [ ] `bbrew serve` command
- [ ] Real-time logs via WebSocket

### Phase 6: Polish & Release (Weeks 11-13)
- [ ] `bbrew sync` with manifest + lockfile
- [ ] Git-backed config sync
- [ ] Shell completions
- [ ] Install script + self-update
- [ ] Documentation, man pages
- [ ] Homebrew tap for bbrew (dogfood)
- [ ] Beta release, feedback iteration

---

## 13. Testing Strategy

| Level | Tool | Coverage |
|-------|------|----------|
| Unit | `cargo test` | >80% for core logic |
| Integration | `cargo test --test integration` | Source adapters, resolver |
| Contract | `cargo test --test contract` | CLI commands, JSON output |
| E2E | Custom test harness | Full install flows |
| Fuzz | `cargo fuzz` | Parsers, version resolution |
| Security | `cargo audit`, `deny` | Dependency scanning |

**Test fixtures:** Mock source servers, sample packages, vuln DB snapshots

---

## 14. Risks & Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Source API changes | High | Medium | Adapter pattern, versioned APIs, fallback |
| Vuln DB false positives | Medium | High | Severity filtering, user override, caching |
| Build sandbox failures | Medium | Medium | Graceful degradation (auto → container → VM → none) |
| Lockfile conflicts | Low | High | Git merge driver for bbrew.lock |
| Performance at scale | Low | Medium | Incremental indexing, parallel ops, caching |
| macOS version incompatibility | Low | High | CI matrix: 12, 13, 14, 15 (beta) |

---

## 15. Open Questions (For Review)

1. **Package format:** Custom tarball? Nix-style store paths? App bundles?
2. **Binary cache:** Host our own? Use GitHub Releases? S3-compatible?
3. **Plugin system:** Allow custom sources/hooks? (Post-v1)
4. **Telemetry:** Opt-in anonymous usage stats? (Privacy-first)
5. **Homebrew tap:** Should `bbrew` be installable via `brew install bbrew`?

---

## 16. Builder Check Notes

**Answered by conversation:**
- Language: Rust
- Sources: Homebrew, MacPorts, Nixpkgs, GitHub Releases
- Fix cascade: Patch → Warn → Rebuild → Interactive
- Trust: Composite (GitHub + Registry + Security + Community)
- Interfaces: TUI + CLI + Web Dashboard
- Reproducibility: Manifest + Lockfile + Git sync
- Install scope: User default, --system flag, sandboxed builds optional

**Builder decisions (my call):**
- Storage: SQLite for package DB, sled for cache (embedded, fast)
- TUI: ratatui + crossterm (modern, active)
- Web: Axum + HTMX (server-rendered, minimal JS)
- Config: TOML (standard, human-readable)
- Lockfile: TOML with semantic versioning
- Parallelism: tokio, rayon for CPU-bound
- Error handling: anyhow + thiserror

**Needs your input:**
- [ ] Package format preference?
- [ ] Binary cache strategy?
- [ ] Telemetry/opt-in?
- [ ] Homebrew tap for bbrew?

---

*End of Design Document*