# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.5] - 2026-05-21

### Fixed
- CI action versions updated to latest (checkout@v6, upload-artifact@v7, download-artifact@v8).
- MSRV documentation updated to reflect Rust 1.91.1 minimum.
- Security audit configuration stabilized with documented advisory ignores.
- Changelog timestamp formatting improved.

## [0.2.4] - 2026-04-12

### Fixed
- Release workflow now targets `macos-15-intel` for `x86_64-apple-darwin` so the amd64 macOS artifact builds on a native Intel runner.
- Prevents x86_64 macOS link failures caused by building that target on arm64-hosted `macos-latest`.

## [0.2.3] - 2026-04-12

### Fixed
- Release workflow now uses `macos-latest` for both macOS targets to avoid unavailable `macos-13` runner cancellations.
- macOS release builds now install OpenSSL and pkg-config before compiling to prevent `openssl-sys` detection failures.

## [0.2.2] - 2026-04-12

### Fixed
- Release workflow Linux ARM64 build now uses a native ARM runner (`ubuntu-24.04-arm`) instead of cross-compiling through a container lacking OpenSSL dev libraries.
- Release workflow installs Linux build dependencies (`pkg-config`, `libssl-dev`) before compiling to avoid OpenSSL discovery failures.

## [0.2.1] - 2026-04-12

### Fixed
- Release workflow macOS build matrix now uses native runners per architecture:
  - `x86_64-apple-darwin` on `macos-13`
  - `aarch64-apple-darwin` on `macos-14`

## [0.2.0] - 2026-04-10

### Highlights
- End-to-end rollback coverage across git, GitHub release, Kubernetes, and audit flows.
- Production hardening with git operation timeouts, retry logic, and bounded audit storage.
- Cross-platform and developer-experience improvements through Docker context updates, CI automation, and richer diagnostics.

### Added
- Git operation timeout configuration in `[git]`:
  - `fetch_timeout_secs`
  - `push_timeout_secs`
  - `operation_timeout_secs`
- Timeout-aware Git preflight/push rollback flow to prevent stuck remote operations.
- Audit store retry wrapper for transient sled I/O failures.
- Audit retention/compaction support to prevent unbounded local audit growth.
- Orchestrator integration tests covering:
  - successful pipeline execution,
  - rollback ordering,
  - rollback-disabled behavior,
  - dry-run execution path.
- Retry utility edge-case tests (retry exhaustion, zero-retry behavior, profile checks).
- Criterion benchmark target (`cargo bench --bench performance`) for:
  - semver bumping/formatting,
  - template rendering,
  - environment variable resolution.
- Health-checkable demo API container and CLI runtime Dockerfile.
- GitHub Actions CI/release workflows and Dependabot automation.

### Changed
- Docker build context creation now runs in blocking worker threads to avoid blocking async runtime threads.
- Docker build context loading preallocates by file size for large contexts to reduce allocation churn.
- Shared version-read logic extracted into a dedicated utility module.
- `BumpType` now uses `FromStr` trait implementation.
- Regex-heavy validations and sanitization paths optimized to avoid repeated compilation.

### Fixed
- Rollback paths for git commit/changelog/tag push edge cases.
- Health check config validation (`health_check.interval > 0`).
- Cross-platform Docker context archive generation (removed shell `tar` dependency).
- Audit store path handling and flush reliability.

### Documentation
- Expanded README with:
  - git timeout config fields,
  - end-to-end configuration examples,
  - benchmark command usage.
- Added crate/module-level rustdoc for core public APIs.
