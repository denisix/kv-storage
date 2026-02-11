# Multi-stage Dockerfile for kv-storage
# Stage 1: Build
FROM rust:1.91-alpine AS builder

# Install build dependencies
RUN apk add --no-cache musl-dev

WORKDIR /build

# Copy manifests and lockfile first to leverage Docker cache
COPY Cargo.toml Cargo.lock ./

# Create dummy sources to build dependencies
RUN mkdir -p src benches && \
    echo "fn main() {}" > src/main.rs && \
    echo "" > src/lib.rs && \
    echo "fn main() {}" > benches/kv_bench.rs

# Build dependencies (this layer will be cached if dependencies don't change)
RUN cargo build --release && \
    rm -rf src benches

# Copy actual source code and recreate dummy bench (benches/ excluded via .dockerignore)
COPY src ./src
RUN mkdir -p benches && echo "fn main() {}" > benches/kv_bench.rs

# Touch all source files so cargo detects changes vs cached dummy build
RUN find src -name '*.rs' -exec touch {} +

# Build the actual application
RUN cargo build --release

# Stage 2: Minimal Alpine runtime (musl-linked binary needs musl)
FROM alpine:3.21

# curl for healthcheck (server is HTTP/2 only, wget doesn't support h2c)
RUN apk add --no-cache curl

# Copy binary from builder
COPY --from=builder /build/target/release/kv-storage /kv-storage

# Create non-root user and data directory with correct ownership
RUN adduser -D -u 10001 kvuser && \
    mkdir -p /data && chown kvuser:kvuser /data

USER kvuser

WORKDIR /data

# Environment variables (TOKEN intentionally has no default — must be set at runtime)
ENV DB_PATH=/data/kv_db \
    BIND_ADDR=0.0.0.0:3000 \
    COMPRESSION_LEVEL=1 \
    KV_CACHE_CAPACITY=1073741824 \
    KV_FLUSH_INTERVAL_MS=100

EXPOSE 3000

# Server is HTTP/2 only — use h2c and pass auth token from env
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD curl -sf --http2-prior-knowledge -H "Authorization: Bearer $TOKEN" http://localhost:3000/metrics > /dev/null || exit 1

ENTRYPOINT ["/kv-storage"]
