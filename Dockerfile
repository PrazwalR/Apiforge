# Apiforge CLI Dockerfile
# Multi-stage build for minimal image size

FROM rust:1.70-slim as builder

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy manifests first for dependency caching
COPY Cargo.toml Cargo.lock ./

# Create dummy main.rs to build dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs

# Build dependencies only (cached layer)
RUN cargo build --release && rm -rf src

# Copy actual source
COPY src ./src

# Touch main.rs to ensure rebuild
RUN touch src/main.rs

# Build the actual binary
RUN cargo build --release

# Runtime image
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    git \
    openssh-client \
    && rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /app/target/release/apiforge /usr/local/bin/apiforge

# Create non-root user for security
RUN useradd -m -s /bin/bash apiforge
USER apiforge
WORKDIR /home/apiforge

# Default entrypoint
ENTRYPOINT ["apiforge"]
CMD ["--help"]
