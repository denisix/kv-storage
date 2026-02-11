# Multi-stage Dockerfile for kv-storage
# Stage 1: Build
FROM rust:1.91-alpine AS builder

# Install build dependencies
RUN apk add --no-cache musl-dev

WORKDIR /build

# Copy manifests first to leverage Docker cache
COPY Cargo.toml ./

# Create a dummy main.rs to build dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    echo "" > src/lib.rs

# Build dependencies (this layer will be cached if dependencies don't change)
RUN cargo build --release && \
    rm -rf src

# Copy actual source code
COPY src ./src

# Build the actual application as a static binary
RUN RUSTFLAGS="-C target-feature=-crt-static" cargo build --release

# Verify it's statically linked
RUN ldd /build/target/release/kv-storage 2>&1 | grep -q "not a dynamic executable" || \
    (echo "Binary is not static!" && exit 1)

# Stage 2: Distroless runtime
FROM gcr.io/distroless/cc-debian12

# Copy binary from builder
COPY --from=builder /build/target/release/kv-storage /kv-storage

# Use nobody user (built into distroless)
USER 65534:65534

# Set working directory
WORKDIR /data

# Environment variables (can be overridden)
ENV TOKEN=changeme \
    DB_PATH=/data/kv_db \
    BIND_ADDR=0.0.0.0:3000 \
    COMPRESSION_LEVEL=1

# Expose port
EXPOSE 3000

# Note: Distroless images have no shell or healthcheck tools
# Health checks should be performed externally (orchestrator level)
# or the application should expose a health endpoint

# Run the application
ENTRYPOINT ["/kv-storage"]
