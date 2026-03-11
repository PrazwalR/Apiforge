# apiforge

Production-grade API release automation CLI. From merged code to healthy pods in production — one command, zero tribal knowledge required.

## Features

- **Single Command Releases**: `apiforge release patch` handles everything
- **Pre-flight Validation**: Catches errors before touching any state
- **Dry-run Support**: Preview every action before executing
- **Instant Rollback**: One command to revert to previous state
- **Full Audit Trail**: Every release tracked and queryable
- **Zero Dependencies**: Single static binary

## Quick Start

```bash
# Install
curl -fsSL https://install.apiforge.dev | sh

# Initialize in your project
cd your-api-project
apiforge init

# Validate environment
apiforge doctor

# Release
apiforge release patch
```

## Commands

- `apiforge init` - Generate configuration for current project
- `apiforge doctor` - Validate environment and dependencies
- `apiforge release <bump>` - Release a new version (major/minor/patch)
- `apiforge rollback` - Roll back to previous release
- `apiforge history` - Show release history
- `apiforge status` - Show current deployment status

## Documentation

See [docs/](./docs/) for detailed documentation.

## License

MIT
