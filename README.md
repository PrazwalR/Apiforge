# Apiforge

> **Production-grade API release automation CLI**.  
> From merged code to healthy pods in production — one command.

[![Rust](https://img.shields.io/badge/rust-1.70.0%2B-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

---

## Table of Contents

1. [What Apiforge does](#what-apiforge-does)
2. [Installation](#installation)
3. [Quick start](#quick-start)
4. [CLI reference](#cli-reference)
5. [Release pipeline behavior](#release-pipeline-behavior)
6. [Rollback semantics](#rollback-semantics)
7. [Configuration reference (`apiforge.toml`)](#configuration-reference-apiforgetoml)
8. [Template variables](#template-variables)
9. [CI/CD integration](#cicd-integration)
10. [Security and reliability model](#security-and-reliability-model)
11. [Developer guide](#developer-guide)
12. [Troubleshooting](#troubleshooting)
13. [Known limitations](#known-limitations)
14. [Contributing](#contributing)
15. [License](#license)

---

## What Apiforge does

Apiforge automates a full release path for API services:

1. Preflight checks for repo and environment.
2. Version bump in language-specific version files.
3. Optional changelog generation.
4. Commit and tag creation.
5. Push to git remote.
6. Optional Docker build/push.
7. Optional Kubernetes image update and rollout wait.
8. Optional GitHub release creation.
9. Optional health-check verification.
10. Automatic rollback of completed steps when a later step fails.

The goal is to make releases **repeatable, reviewable, and recoverable**.

---

## Installation

### Prerequisites

- Rust `1.70.0+` (for building/running from source)
- `git`
- `docker` (if using Docker steps)
- `kubectl` (if using Kubernetes steps)
- `aws` CLI credentials/profile (if using ECR)

### Option 1: Install from Cargo

```bash
cargo install apiforge
```

### Option 2: Download release archives

```bash
# Linux (x86_64 / amd64)
curl -L https://github.com/PrazwalR/Apiforge/releases/latest/download/apiforge-linux-amd64.tar.gz -o apiforge.tar.gz
tar -xzf apiforge.tar.gz
chmod +x apiforge
sudo mv apiforge /usr/local/bin/

# Linux (arm64)
curl -L https://github.com/PrazwalR/Apiforge/releases/latest/download/apiforge-linux-arm64.tar.gz -o apiforge.tar.gz
tar -xzf apiforge.tar.gz
chmod +x apiforge
sudo mv apiforge /usr/local/bin/

# macOS (Apple Silicon)
curl -L https://github.com/PrazwalR/Apiforge/releases/latest/download/apiforge-darwin-arm64.tar.gz -o apiforge.tar.gz
tar -xzf apiforge.tar.gz
chmod +x apiforge
sudo mv apiforge /usr/local/bin/

# macOS (Intel / amd64)
curl -L https://github.com/PrazwalR/Apiforge/releases/latest/download/apiforge-darwin-amd64.tar.gz -o apiforge.tar.gz
tar -xzf apiforge.tar.gz
chmod +x apiforge
sudo mv apiforge /usr/local/bin/
```

Windows artifact is published as `apiforge-windows-amd64.zip`.

### Option 3: Build from source

```bash
git clone https://github.com/PrazwalR/Apiforge.git
cd Apiforge
cargo build --release --locked
./target/release/apiforge --version
```

---

## Quick start

### 1. Initialize config

```bash
apiforge init
```

This creates `apiforge.toml` with defaults.

### 2. Validate setup

```bash
apiforge doctor
```

### 3. Preview a release (no side effects)

```bash
apiforge release patch --dry-run
```

### 4. Execute release

```bash
apiforge release patch
```

### 5. Inspect history and status

```bash
apiforge history --limit 20
apiforge status
```

---

## CLI reference

Global flags:

- `--config <path>`: config file path (default: `apiforge.toml`)
- `--debug`: enable debug logs (`APIFORGE_DEBUG=true` also works)

### `apiforge init`

Initializes a new config file.

```bash
apiforge init [--name my-service] [--force]
```

### `apiforge doctor`

Checks:

- required tools (`git`, `docker`, `kubectl`, `aws`)
- config file parse/validation
- repository visibility/basic git status

```bash
apiforge doctor
```

### `apiforge release <major|minor|patch>`

Runs the release pipeline.

```bash
apiforge release patch \
  --dry-run \
  --skip-docker \
  --skip-k8s \
  --skip-github \
  --skip-notify \
  --no-changelog \
  --output json \
  --yes
```

Flags:

| Flag | Meaning |
|---|---|
| `--dry-run` | Simulate pipeline steps without mutating systems |
| `--skip-docker` | Skip Docker build and push steps |
| `--skip-k8s` | Skip Kubernetes update and rollout wait |
| `--skip-github` | Skip GitHub release step |
| `--skip-notify` | Skip post-release notification dispatch |
| `--no-changelog` | Skip changelog step even if enabled in config |
| `--output text|json` | Output mode |
| `-y, --yes` | Skip confirmation prompt |

### `apiforge rollback`

Rolls Kubernetes deployment image back to a target version.

```bash
apiforge rollback --to v1.2.3
apiforge rollback --to v1.2.3 --dry-run
```

### `apiforge history`

Reads audit records from `.apiforge/audit`.

```bash
apiforge history --limit 50 --filter success --output text
apiforge history --output json
```

### `apiforge status`

Shows project metadata, git HEAD/tag, and Kubernetes deployment image/replica state.

```bash
apiforge status
```

---

## Release pipeline behavior

When you run `apiforge release <bump>`, step order is:

1. `git-preflight`
2. `version-bump`
3. `changelog` *(if enabled and not skipped)*
4. `git-commit`
5. `git-tag`
6. `git-push`
7. `docker-build` *(if not skipped)*
8. `docker-push` *(if not skipped)*
9. `k8s-update` *(if not skipped)*
10. `k8s-rollout` *(if not skipped)*
11. `github-release` *(if configured and not skipped)*
12. `health-check` *(if configured)*

On success, Apiforge can send notification(s) and records a release audit entry.

---

## Rollback semantics

Automatic rollback is triggered when a step fails after prior steps succeeded. Rollback runs in **reverse order** for completed steps.

| Step | Rollback behavior |
|---|---|
| `version-bump` | Restores original version-file content captured before mutation |
| `changelog` | Restores `CHANGELOG.md` from git checkout |
| `git-commit` | Soft reset to parent commit (changes remain staged) |
| `git-tag` | Deletes created tag |
| `git-push` | Deletes remote/local tag; intentionally does **not** force-rewrite shared commit history |
| `github-release` | Deletes created GitHub release when possible |
| docker/k8s/health | Step-specific best-effort behavior or no-op if not applicable |

Important design choice: on git-push rollback, commit history is preserved and only release marker tags are removed.

---

## Configuration reference (`apiforge.toml`)

### Full example

```toml
[project]
name = "my-api"
language = "rust" # rust | node | python | go | java

[git]
main_branch = "main"
tag_format = "v{version}"
changelog = true
commit_message = "chore: release v{{ version }}"
remote = "origin"
require_clean = true
require_main_branch = true
fetch_timeout_secs = 60
push_timeout_secs = 120
operation_timeout_secs = 30

[docker]
registry = "aws_ecr" # aws_ecr | docker_hub | ghcr | custom
repository = "my-api"
dockerfile = "Dockerfile"
context = "."
tags = ["{version}", "{major}.{minor}", "latest", "{git_sha}"]
# build_args = { APP_ENV = "production" }

[kubernetes]
context = "production"
namespace = "default"
deployment = "my-api"
manifest_path = "k8s/deployment.yaml"
image_field = ".spec.template.spec.containers[0].image"
rollout_timeout = 300
min_ready_percent = 100

[aws]
region = "us-east-1"
# profile = "prod"

[github]
repository = "org/repo"
token = "${GITHUB_TOKEN}"
create_release = true
prerelease = false
draft = false

[notifications.slack]
webhook_url = "${SLACK_WEBHOOK_URL}"
message = "{{ status_emoji }} Release {{ version }} of {{ project }}: {{ status }}"
notify_on = "both" # success | failure | both

# Optional generic webhook payload
# [notifications.webhook]
# url = "https://hooks.example.com/release"
# method = "POST"
# headers = { "Authorization" = "Bearer ${WEBHOOK_TOKEN}" }
# body = "{\"project\":\"{{ project }}\",\"version\":\"{{ version }}\",\"status\":\"{{ status }}\"}"

[health_check]
url = "https://api.example.com/health"
method = "GET" # GET | POST | HEAD | PUT
expected_status = 200
# expected_body_field = "/status"
# expected_body_value = "ok"
timeout = 60
interval = 5
```

### Field details

#### `[project]`

| Key | Type | Required | Notes |
|---|---|---|---|
| `name` | string | yes | Displayed in output/messages |
| `language` | enum | yes | Determines version file (`Cargo.toml`, `package.json`, `pyproject.toml`, `go.mod`, `pom.xml`) |

#### `[git]`

| Key | Type | Default | Notes |
|---|---|---|---|
| `main_branch` | string | none | Expected release branch |
| `tag_format` | string | none | Must include `{version}` |
| `changelog` | bool | `true` | Enable changelog step |
| `commit_message` | string | none | Supports `{{ version }}` / `{{ project }}` |
| `remote` | string | `origin` | Target remote |
| `require_clean` | bool | `true` | Require no unstaged/uncommitted changes |
| `require_main_branch` | bool | `true` | Require release from `main_branch` |
| `fetch_timeout_secs` | u64 | `60` | Timeout for fetch-like operations |
| `push_timeout_secs` | u64 | `120` | Timeout for push operations |
| `operation_timeout_secs` | u64 | `30` | Timeout for other git operations |

#### `[docker]`

| Key | Type | Default | Notes |
|---|---|---|---|
| `registry` | enum | none | `aws_ecr`, `docker_hub`, `ghcr`, `custom` |
| `repository` | string | none | Required non-empty |
| `dockerfile` | string | `Dockerfile` | Relative to `context` |
| `context` | string | `.` | Build context path |
| `tags` | array<string> | none | At least one tag pattern required |
| `build_args` | table | none | Optional build args |

Docker tag placeholders supported by validation/runtime:

- `{version}`
- `{major}`
- `{minor}`
- `{patch}`
- `{git_sha}`
- `{git_sha_full}`

#### `[kubernetes]`

| Key | Type | Default | Notes |
|---|---|---|---|
| `context` | string | none | kube context name |
| `namespace` | string | none | Required non-empty |
| `deployment` | string | none | Deployment to patch |
| `manifest_path` | string | none | Maintained for manifest-oriented workflows |
| `image_field` | string | none | JSON pointer-like selector for image path |
| `rollout_timeout` | u64 | `300` | Max seconds for rollout wait |
| `min_ready_percent` | u8 | `100` | Must be `0..=100` |

#### `[aws]`

| Key | Type | Required | Notes |
|---|---|---|---|
| `region` | string | yes for ECR | Required when `docker.registry = "aws_ecr"` |
| `profile` | string | no | Optional AWS profile |

#### `[github]` *(optional)*

| Key | Type | Default | Notes |
|---|---|---|---|
| `repository` | string | none | `owner/repo` |
| `token` | string | none | GitHub token |
| `create_release` | bool | `true` | Kept for compatibility |
| `prerelease` | bool | `false` | GitHub prerelease flag |
| `draft` | bool | `false` | GitHub draft flag |

#### `[notifications]` *(optional)*

Slack:

| Key | Type | Default |
|---|---|---|
| `webhook_url` | string | none |
| `message` | string | none |
| `notify_on` | enum | `both` |

Webhook:

| Key | Type | Default |
|---|---|---|
| `url` | string | none |
| `method` | string | `POST` |
| `headers` | table | none |
| `body` | string | none |

#### `[health_check]` *(optional)*

| Key | Type | Default | Notes |
|---|---|---|---|
| `url` | string | none | Required if section present |
| `method` | enum | `GET` | `GET`, `POST`, `HEAD`, `PUT` |
| `expected_status` | u16 | `200` | Expected HTTP status |
| `expected_body_field` | string | none | JSON pointer path (e.g. `/status`) |
| `expected_body_value` | string | none | Compared against resolved response field |
| `timeout` | u64 | `60` | Total check window |
| `interval` | u64 | `5` | Retry interval, must be `> 0` |

---

## Template variables

Apiforge uses templates in multiple places. Available keys depend on context:

### Commit message templates (`git.commit_message`)

- `{{ version }}`
- `{{ project }}`

### Docker tag templates (`docker.tags`)

- `{version}`, `{major}`, `{minor}`, `{patch}`, `{git_sha}`, `{git_sha_full}`

### Notification templates (message/body)

Commonly provided:

- `{{ version }}`
- `{{ project }}`
- `{{ status }}`
- `{{ status_emoji }}`

### Health-check templates (`health_check.url`, `expected_body_value`)

- `{{ version }}`
- `{{ project }}`

---

## CI/CD integration

### GitHub Actions (example)

```yaml
name: Release via Apiforge

on:
  workflow_dispatch:
    inputs:
      bump:
        description: "Version bump type"
        required: true
        default: "patch"
        type: choice
        options: [patch, minor, major]

jobs:
  release:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Apiforge
        run: |
          curl -L https://github.com/PrazwalR/Apiforge/releases/latest/download/apiforge-linux-amd64.tar.gz -o apiforge.tar.gz
          tar -xzf apiforge.tar.gz
          chmod +x apiforge
          sudo mv apiforge /usr/local/bin/

      - name: Run release
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: apiforge release ${{ inputs.bump }} --yes
```

---

## Security and reliability model

### Built-in protections

- Config validation before release execution.
- Timeout wrappers around network-prone git operations.
- Automatic rollback orchestration for completed steps.
- Sanitization of sensitive data in rendered/logged error messages.
- Audit log persistence under `.apiforge/audit`.

### Audit storage

- Location: `.apiforge/audit`
- Retention: bounded record count
- Supports compaction and retry-aware writes

### Vulnerability scanning

Use:

```bash
cargo audit
```

If advisories are intentionally suppressed due transitive ecosystem constraints, they are documented in `.cargo/audit.toml`.

---

## Developer guide

### Repository structure

```text
src/
  cli.rs                 # CLI definition
  config.rs              # Config model + validation
  orchestrator/          # Pipeline execution + rollback orchestration
  steps/                 # Concrete step implementations
    git/
    docker/
    kubernetes/
    github/
    health/
  integrations/          # Service clients (git, docker, k8s, aws, github)
  audit/                 # Release history store
  output/                # CLI output rendering
  utils/                 # Helpers (semver/template/retry/sanitize/version)
```

### Local quality gates

```bash
cargo fmt --all -- --check
cargo test --all-features --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo build --release --locked
cargo doc --no-deps --locked
cargo bench --no-run --locked
cargo audit
```

---

## Troubleshooting

### `git.tag_format must contain {version}`

Your `[git].tag_format` is invalid. Use a format like:

```toml
tag_format = "v{version}"
```

### Health-check never succeeds

Check:

1. endpoint URL and network reachability
2. method (`GET`/`POST`/`HEAD`/`PUT`)
3. expected status code
4. optional JSON pointer/value match
5. timeout/interval values

### ECR or AWS auth issues

Verify:

- correct `aws.region`
- IAM credentials/profile
- ability to call STS/ECR

### Kubernetes rollout timeout

Check deployment events and image pull/access:

```bash
kubectl -n <namespace> describe deploy <name>
kubectl -n <namespace> get pods
kubectl -n <namespace> logs <pod>
```

---

## Known limitations

- `apiforge rollback` currently requires explicit `--to <version>`; automatic rollback-target detection is not implemented yet.
- Git push rollback intentionally avoids force-rewriting remote commit history; it removes release tags instead.

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

---

## License

MIT — see [LICENSE](LICENSE).
