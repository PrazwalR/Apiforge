# Apiforge 🔥

> Production-grade API release automation CLI. From merged code to healthy pods in production — one command, zero tribal knowledge required.

[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

---

## The Problem

Releasing an API to production typically involves:

1. ✏️ Bumping version numbers in multiple files
2. 📝 Writing changelog entries manually
3. 📦 Building Docker images with correct tags
4. 🔐 Authenticating with container registries (ECR, DockerHub, GHCR)
5. ⬆️ Pushing images to the registry
6. ☸️ Updating Kubernetes deployments
7. ⏳ Waiting for rollouts to complete
8. 🏥 Running health checks
9. 🏷️ Creating Git tags and GitHub releases
10. 📢 Sending notifications to Slack/webhooks
11. 🔙 Rolling back if anything fails

**This is error-prone, time-consuming, and requires tribal knowledge.**

## The Solution

```bash
apiforge release patch
```

**One command. That's it.** Apiforge handles the entire pipeline automatically:

- ✅ Validates your environment before making any changes
- ✅ Bumps versions in Cargo.toml, package.json, etc.
- ✅ Generates changelogs from commit messages
- ✅ Builds, tags, and pushes Docker images
- ✅ Updates Kubernetes deployments
- ✅ Monitors rollout progress
- ✅ Runs health checks
- ✅ Creates Git tags and GitHub releases
- ✅ Sends success/failure notifications
- ✅ **Automatically rolls back on failure**

---

## Table of Contents

- [Installation](#installation)
- [Quick Start](#quick-start)
- [Commands](#commands)
- [Configuration](#configuration)
- [How It Works](#how-it-works)
- [Supported Integrations](#supported-integrations)
- [CI/CD Integration](#cicd-integration)
- [Troubleshooting](#troubleshooting)
- [Contributing](#contributing)

---

## Installation

### Option 1: Install from Cargo (Recommended for Rust users)

```bash
cargo install apiforge
```

### Option 2: Download Prebuilt Binary

```bash
# Linux (x86_64)
curl -L https://github.com/PrazwalR/Apiforge/releases/latest/download/apiforge-linux-x86_64 -o apiforge
chmod +x apiforge
sudo mv apiforge /usr/local/bin/

# macOS (Apple Silicon)
curl -L https://github.com/PrazwalR/Apiforge/releases/latest/download/apiforge-darwin-arm64 -o apiforge
chmod +x apiforge
sudo mv apiforge /usr/local/bin/

# macOS (Intel)
curl -L https://github.com/PrazwalR/Apiforge/releases/latest/download/apiforge-darwin-x86_64 -o apiforge
chmod +x apiforge
sudo mv apiforge /usr/local/bin/
```

### Option 3: Build from Source

```bash
git clone https://github.com/PrazwalR/Apiforge.git
cd Apiforge
cargo build --release
sudo cp target/release/apiforge /usr/local/bin/
```

### Verify Installation

```bash
apiforge --version
# apiforge 0.1.0
```

---

## Quick Start

### 1. Initialize Configuration

```bash
cd your-api-project
apiforge init
```

This creates `apiforge.toml` with sensible defaults based on your project.

### 2. Validate Your Environment

```bash
apiforge doctor
```

Output:
```
▸ Checking dependencies
  ✓ git (2.39.0)
  ✓ docker (24.0.5)
  ✓ kubectl (1.28.0)

▸ Checking configuration
  ✓ apiforge.toml found
  ✓ Dockerfile exists
  ✓ Kubernetes context 'prod' available

▸ Checking connectivity
  ✓ Docker daemon accessible
  ✓ Kubernetes cluster reachable
  ✓ AWS credentials valid

✓ All checks passed! Ready to release.
```

### 3. Preview a Release (Dry Run)

```bash
apiforge release patch --dry-run
```

This shows exactly what will happen without making any changes.

### 4. Execute the Release

```bash
apiforge release patch
```

Output:
```
▸ Pre-flight checks
  ✓ git-preflight
  ✓ version-bump
  ✓ docker-build
  ✓ k8s-update

▸ Executing release pipeline
  ✓ git-preflight         Repository clean, on main branch (12ms)
  ✓ version-bump          Bumped version from 1.2.3 to 1.2.4 (5ms)
  ✓ changelog             Generated changelog with 3 commits (8ms)
  ✓ git-commit            Committed: Release v1.2.4 (15ms)
  ✓ git-tag               Created tag v1.2.4 (3ms)
  ✓ docker-build          Built image sha256:abc123 with tags: 1.2.4, latest (45s)
  ✓ docker-push           Pushed 123456789.dkr.ecr.us-east-1.amazonaws.com/my-api:1.2.4 (12s)
  ✓ git-push              Pushed to origin/main (2s)
  ✓ k8s-update            Updated deployment api-server container 'api' to 1.2.4 (1s)
  ✓ k8s-rollout           Rollout complete: 3/3 replicas ready (25s)
  ✓ health-check          Health check passed: https://api.example.com/health (500ms)
  ✓ github-release        Created release v1.2.4 (800ms)

✨ Release 1.2.4 complete!
   12 steps executed successfully
```

---

## Commands

### `apiforge init`

Generates a configuration file for your project.

```bash
apiforge init
```

Detects your project type (Rust, Node.js, Python, Go, Java) and creates appropriate defaults.

### `apiforge doctor`

Validates your environment and configuration.

```bash
apiforge doctor
```

Checks:
- Required CLI tools (git, docker, kubectl)
- Configuration file validity
- Docker daemon connectivity
- Kubernetes cluster access
- AWS/GitHub credentials

### `apiforge release <bump>`

Executes the full release pipeline.

```bash
# Patch release (1.2.3 → 1.2.4)
apiforge release patch

# Minor release (1.2.3 → 1.3.0)
apiforge release minor

# Major release (1.2.3 → 2.0.0)
apiforge release major
```

**Options:**

| Flag | Description |
|------|-------------|
| `--dry-run` | Preview without making changes |
| `--skip-docker` | Skip Docker build and push |
| `--skip-k8s` | Skip Kubernetes deployment |
| `--skip-github` | Skip GitHub release creation |
| `--skip-notify` | Skip notifications |
| `--no-changelog` | Skip changelog generation |
| `--output json` | Output results as JSON |
| `-y, --yes` | Skip confirmation prompt |

### `apiforge rollback`

Roll back to a previous version.

```bash
# Roll back to previous version (auto-detect)
apiforge rollback

# Roll back to specific version
apiforge rollback --to v1.2.3

# Preview rollback
apiforge rollback --dry-run
```

### `apiforge history`

View release history.

```bash
# Show recent releases
apiforge history

# Limit results
apiforge history --limit 5

# Filter by status
apiforge history --filter success
apiforge history --filter failed

# JSON output
apiforge history --output json
```

### `apiforge status`

Show current deployment status.

```bash
apiforge status
```

Output:
```
▸ Current State
  Project: my-api
  Language: rust
  Current Version: 1.2.4
  Git Branch: main
  Kubernetes Context: prod-cluster

▸ Deployment Status
  Namespace: production
  Deployment: api-server
  Ready: 3/3 replicas
  Image: 123456789.dkr.ecr.us-east-1.amazonaws.com/my-api:1.2.4
```

---

## Configuration

Apiforge uses `apiforge.toml` in your project root.

### Full Configuration Reference

```toml
# Project metadata
[project]
name = "my-api"
language = "rust"  # rust, node, python, go, java

# Git configuration
[git]
require_clean = true           # Require clean working tree
require_main_branch = true     # Only release from main/master
main_branch = "main"           # Name of main branch
tag_format = "v{version}"      # Tag format ({version} is replaced)
changelog = true               # Generate changelog
commit_message = "Release v{{ version }}"

# Docker configuration
[docker]
registry = "aws-ecr"           # aws-ecr, dockerhub, ghcr, custom
repository = "my-api"          # Repository name
dockerfile = "Dockerfile"      # Dockerfile path
context = "."                  # Build context
tags = ["{version}", "latest"] # Tag patterns

# Optional: Build arguments
[docker.build_args]
NODE_ENV = "production"
BUILD_DATE = "$(date -u +%Y-%m-%dT%H:%M:%SZ)"

# AWS configuration (for ECR)
[aws]
region = "us-east-1"
# profile = "production"       # Optional: AWS profile

# Kubernetes configuration
[kubernetes]
context = "prod-cluster"       # kubectl context name
namespace = "production"       # Target namespace
deployment = "api-server"      # Deployment name
image_field = "api"            # Container name or index ("0", "api", "sidecar")
rollout_timeout = 300          # Seconds to wait for rollout
min_ready_percent = 100        # Minimum ready replicas percentage

# GitHub configuration (optional)
[github]
token = "${GITHUB_TOKEN}"      # Environment variable reference
create_release = true
prerelease = false
draft = false
generate_notes = true          # Auto-generate release notes

# Health check (optional)
[health_check]
url = "https://api.example.com/health"
method = "GET"
expected_status = 200
timeout = 60                   # Seconds
interval = 5                   # Check interval
# expected_body_field = "status"
# expected_body_value = "healthy"

# Notifications (optional)
[notifications.slack]
webhook_url = "${SLACK_WEBHOOK_URL}"
channel = "#releases"
message = "🚀 {{ project }} {{ version }} released! {{ status_emoji }}"
```

### Environment Variables

Sensitive values can be referenced using `${VAR_NAME}` syntax:

```toml
[github]
token = "${GITHUB_TOKEN}"

[notifications.slack]
webhook_url = "${SLACK_WEBHOOK_URL}"
```

---

## How It Works

### Release Pipeline

When you run `apiforge release patch`, the following steps execute in order:

```
┌─────────────────┐
│  Pre-flight     │ ← Validates all steps before executing
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Git Preflight  │ ← Checks clean tree, correct branch
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Version Bump   │ ← Updates Cargo.toml/package.json
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Changelog      │ ← Generates CHANGELOG.md from commits
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Git Commit     │ ← Commits version + changelog
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Git Tag        │ ← Creates version tag (e.g., v1.2.4)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Docker Build   │ ← Builds image with version tags
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Docker Push    │ ← Pushes to registry (ECR/DockerHub/GHCR)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Git Push       │ ← Pushes commit and tag to remote
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  K8s Update     │ ← Patches deployment with new image
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  K8s Rollout    │ ← Waits for rollout to complete
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Health Check   │ ← Verifies API is healthy
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  GitHub Release │ ← Creates GitHub release with notes
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Notifications  │ ← Sends Slack/webhook notifications
└─────────────────┘
```

### Automatic Rollback

**If any step fails, Apiforge automatically rolls back completed steps in reverse order:**

```
Step 7 (K8s Update) FAILED!

▸ Rolling back completed steps
  ✓ git-push (rolled back)       ← Deletes remote tag (preserves commit)
  ✓ git-tag (rolled back)        ← Deletes local tag
  ✓ git-commit (rolled back)     ← Resets to previous commit
  ✓ changelog (rolled back)      ← Restores original file
  ✓ version-bump (rolled back)   ← Restores original version

✗ Release failed. All changes have been rolled back.
```

This ensures your repository and infrastructure never end up in an inconsistent state.

### Auto-Created Resources

**ECR Repositories**: If your ECR repository doesn't exist, Apiforge automatically creates it with scan-on-push enabled before pushing images. No manual setup required!

---

## Supported Integrations

### Container Registries

| Registry | Config Value | Authentication |
|----------|--------------|----------------|
| AWS ECR | `aws-ecr` | AWS credentials (env/profile/IAM role) |
| Docker Hub | `dockerhub` | `DOCKER_USERNAME` + `DOCKER_PASSWORD` |
| GitHub Container Registry | `ghcr` | `GITHUB_TOKEN` |
| Custom Registry | `custom` | Docker config.json |

### Languages

| Language | Version File | Detection |
|----------|-------------|-----------|
| Rust | `Cargo.toml` | `package.version` |
| Node.js | `package.json` | `version` |
| Python | `pyproject.toml` | `tool.poetry.version` or `project.version` |
| Go | `go.mod` / `version.go` | Module comment or `Version` constant |
| Java | `pom.xml` | `project.version` |

### Kubernetes

- **Deployments**: Update container images
- **Rollout monitoring**: Wait for ready replicas
- **Automatic rollback**: Uses revision history

### Notifications

| Service | Configuration |
|---------|--------------|
| Slack | Webhook URL + message template |
| Custom Webhook | Any HTTP endpoint |

---

## CI/CD Integration

### GitHub Actions

```yaml
name: Release

on:
  workflow_dispatch:
    inputs:
      bump:
        description: 'Version bump type'
        required: true
        type: choice
        options:
          - patch
          - minor
          - major

jobs:
  release:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0  # Full history for changelog

      - name: Configure AWS credentials
        uses: aws-actions/configure-aws-credentials@v4
        with:
          aws-access-key-id: ${{ secrets.AWS_ACCESS_KEY_ID }}
          aws-secret-access-key: ${{ secrets.AWS_SECRET_ACCESS_KEY }}
          aws-region: us-east-1

      - name: Setup kubectl
        uses: azure/setup-kubectl@v3

      - name: Configure kubeconfig
        run: |
          aws eks update-kubeconfig --name my-cluster --region us-east-1

      - name: Install Apiforge
        run: |
          curl -L https://github.com/PrazwalR/Apiforge/releases/latest/download/apiforge-linux-x86_64 -o apiforge
          chmod +x apiforge
          sudo mv apiforge /usr/local/bin/

      - name: Release
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          SLACK_WEBHOOK_URL: ${{ secrets.SLACK_WEBHOOK_URL }}
        run: |
          apiforge release ${{ inputs.bump }} --yes
```

### GitLab CI

```yaml
release:
  stage: deploy
  image: rust:latest
  before_script:
    - cargo install apiforge
    - aws eks update-kubeconfig --name my-cluster
  script:
    - apiforge release patch --yes
  only:
    - main
  when: manual
```

---

## Troubleshooting

### "Docker daemon not accessible"

```bash
# Check Docker is running
docker ps

# If using Docker Desktop, ensure it's started
# If using Linux, check the socket
sudo systemctl status docker
```

### "Kubernetes context not found"

```bash
# List available contexts
kubectl config get-contexts

# Set the correct context in apiforge.toml
[kubernetes]
context = "your-context-name"
```

### "AWS credentials invalid"

```bash
# Check AWS credentials
aws sts get-caller-identity

# If using profiles, set in config
[aws]
profile = "your-profile"
```

### "Tag already exists"

Apiforge validates that the version tag doesn't exist before creating it. If you see this error:

```bash
# Either bump to a new version
apiforge release minor

# Or delete the existing tag (use with caution!)
git tag -d v1.2.4
git push origin :refs/tags/v1.2.4
```

### "Rollout timeout"

```bash
# Check pod status
kubectl get pods -n your-namespace

# Check events
kubectl describe deployment your-deployment -n your-namespace

# Increase timeout in config
[kubernetes]
rollout_timeout = 600  # 10 minutes
```

---

## Architecture

```
apiforge/
├── src/
│   ├── main.rs              # CLI entry point
│   ├── cli.rs               # Command definitions (clap)
│   ├── config.rs            # Configuration parsing
│   ├── error.rs             # Error types
│   │
│   ├── integrations/        # External service clients
│   │   ├── aws.rs           # AWS ECR authentication
│   │   ├── docker.rs        # Docker build/push (bollard)
│   │   ├── git/             # Git operations (git2)
│   │   ├── github.rs        # GitHub releases (octocrab)
│   │   └── kubernetes.rs    # K8s deployments (kube-rs)
│   │
│   ├── steps/               # Pipeline steps
│   │   ├── mod.rs           # Step trait definition
│   │   ├── docker/          # Docker build/push steps
│   │   ├── git/             # Git operations steps
│   │   ├── github/          # GitHub release step
│   │   ├── health/          # Health check step
│   │   ├── kubernetes/      # K8s update/rollout steps
│   │   └── notify/          # Notification steps
│   │
│   ├── orchestrator/        # Pipeline execution engine
│   ├── audit/               # Release history tracking
│   ├── output/              # Colored terminal output
│   └── utils/               # Version parsing, templates
│
├── apiforge.toml            # Example configuration
├── Cargo.toml               # Rust dependencies
└── README.md                # This file
```

---

## Contributing

Contributions are welcome! Please read our [Contributing Guide](CONTRIBUTING.md) first.

### Development Setup

```bash
# Clone the repository
git clone https://github.com/PrazwalR/Apiforge.git
cd Apiforge

# Build
cargo build

# Run tests
cargo test

# Run lints
cargo clippy

# Format code
cargo fmt
```

### Running Locally

```bash
# Run without installing
cargo run -- doctor
cargo run -- release patch --dry-run
```

---

## License

MIT License - see [LICENSE](LICENSE) for details.

---

## Acknowledgments

Built with:
- [clap](https://github.com/clap-rs/clap) - Command line argument parsing
- [bollard](https://github.com/fussybeaver/bollard) - Docker API client
- [kube-rs](https://github.com/kube-rs/kube) - Kubernetes client
- [octocrab](https://github.com/XAMPPRocky/octocrab) - GitHub API client
- [git2](https://github.com/rust-lang/git2-rs) - Git operations

---

<p align="center">
  Made with ❤️ for DevOps engineers tired of manual releases
</p>
