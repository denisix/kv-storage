# Multi-stage Dockerfile for kv-storage
# Stage 1: Build
FROM rust:alpine AS builder

# Install build dependencies
RUN apk add --no-cache musl-dev

WORKDIR /build

# Copy manifests first to leverage Docker cache
COPY Cargo.toml Cargo.lock ./

# Create dummy sources to cache dependencies
RUN mkdir -p src benches && \
    echo "fn main() {}" > src/main.rs && \
    echo "" > src/lib.rs && \
    echo "fn main() {}" > benches/kv_bench.rs && \
    cargo fetch --locked

# Build dependencies (cached layer)
RUN cargo build --release && rm -rf src

# Copy source and build application
COPY src ./src
RUN mkdir -p benches && echo "fn main() {}" > benches/kv_bench.rs && \
    find src -name '*.rs' -exec touch {} + && \
    cargo build --release --locked

# Stage 2: Minimal runtime
FROM alpine

LABEL org.opencontainers.image.source="https://github.com/denisix/kv-storage"
LABEL org.opencontainers.image.description="KV Storage: High-performance key-value storage with HTTP/2, deduplication, and Zstd compression"
LABEL org.opencontainers.image.licenses=MIT

# curl for healthcheck (HTTP/2 h2c required)
RUN apk add --no-cache curl

# Create user and copy binary with correct ownership in single layer
RUN adduser -D -u 10001 kvuser && mkdir -p /data && chown kvuser:kvuser /data

COPY --from=builder --chown=kvuser:kvuser /build/target/release/kv-storage /kv-storage

USER kvuser
WORKDIR /data

ENV DB_PATH=/data/kv_db \
    BIND_ADDR=0.0.0.0:3000 \
    COMPRESSION_LEVEL=1 \
    KV_CACHE_CAPACITY=1073741824 \
    KV_FLUSH_INTERVAL_MS=1000

EXPOSE 3000

# Healthcheck uses /keys endpoint (read-only, no auth required for health status)
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD curl -sf --http2-prior-knowledge -H "Authorization: Bearer ${TOKEN}" http://localhost:3000/keys?limit=1 -o /dev/null || exit 1

ENTRYPOINT ["/kv-storage"]
