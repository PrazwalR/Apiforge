# Contributing to Apiforge

Thank you for your interest in contributing to Apiforge! This document provides guidelines and information for contributors.

## Getting Started

### Prerequisites

- Rust 1.91 or later
- Docker (for building and testing container features)
- kubectl and a Kubernetes cluster (for testing K8s features)
- AWS CLI (optional, for ECR features)

### Building

```bash
git clone https://github.com/PrazwalR/Apiforge.git
cd Apiforge
cargo build
```

### Running Tests

```bash
cargo test
```

## Development Guidelines

### Code Style

- Follow standard Rust formatting (`cargo fmt`)
- Run `cargo clippy` before committing
- Add documentation comments for public APIs
- Keep functions focused and small

### Commit Messages

Use conventional commit format:
- `feat:` New features
- `fix:` Bug fixes
- `docs:` Documentation changes
- `refactor:` Code refactoring
- `test:` Adding or modifying tests
- `chore:` Maintenance tasks

Example: `feat: add retry logic for AWS operations`

### Pull Requests

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes
4. Run tests and linting
5. Commit with clear messages
6. Push to your fork
7. Open a Pull Request

### PR Checklist

- [ ] Code compiles without warnings
- [ ] All tests pass
- [ ] New features have tests
- [ ] Documentation is updated
- [ ] CHANGELOG is updated (if applicable)

## Architecture Overview

Apiforge uses a step-based architecture:

```
src/
├── steps/           # Individual release steps
│   ├── git/         # Git operations (commit, push, tag)
│   ├── docker/      # Docker build and push
│   ├── kubernetes/  # K8s deployment updates
│   ├── github/      # GitHub releases
│   └── health/      # Health checks
├── integrations/    # External service clients
│   ├── aws.rs       # AWS ECR/STS
│   ├── github.rs    # GitHub API
│   └── kubernetes.rs # Kubernetes client
├── orchestrator/    # Pipeline execution and rollback
├── config.rs        # Configuration parsing
├── error.rs         # Error types
└── utils/           # Shared utilities
```

### Adding a New Step

1. Create a new module in `src/steps/`
2. Implement the `Step` trait:
   - `name()` - Step identifier
   - `description()` - Human-readable description
   - `validate()` - Pre-execution validation
   - `execute()` - Main step logic
   - `dry_run()` - Simulation mode
   - `rollback()` - Undo on failure (optional)

3. Register in `src/steps/mod.rs`

### Error Handling

- Use the custom `Result<T>` type from `src/error.rs`
- Create specific error variants for clear diagnostics
- Wrap external errors with context

## Reporting Issues

When reporting bugs:
- Describe expected vs actual behavior
- Include Apiforge version (`apiforge --version`)
- Provide relevant configuration (redact secrets)
- Include error messages and logs

## Feature Requests

Open an issue with:
- Clear description of the feature
- Use case and motivation
- Proposed implementation (optional)

## Questions?

Open a GitHub Discussion for questions or ideas that aren't bugs or feature requests.

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
