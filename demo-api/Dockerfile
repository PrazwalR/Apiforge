FROM rust:1.70-slim as builder
WORKDIR /app
COPY Cargo.toml .
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/demo-api /usr/local/bin/demo-api
CMD ["demo-api"]
