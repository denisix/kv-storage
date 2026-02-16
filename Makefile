.PHONY: all build dev test release clean run docker-build docker-run help test-tls

# Default target
all: build

# Build in debug mode
build:
	@echo "Building KV Storage..."
	cargo build

# Run in development mode with hot reload
dev:
	@echo "Running in development mode..."
	cargo run

# Run tests (unit tests only - integration tests require running server)
test:
	@echo "Running unit tests..."
	cargo test --lib

# Run tests with output
test-verbose:
	@echo "Running tests (verbose)..."
	cargo test -- --nocapture

# Run only unit tests
test-unit:
	@echo "Running unit tests..."
	cargo test --lib

# Run integration tests (requires server to be running)
test-integration:
	@echo "Running integration tests..."
	cargo test --test integration_test

# Run TLS integration tests (generates self-signed cert)
test-tls:
	@echo "Running TLS integration tests..."
	cargo test --test tls_integration_test

# Run Rust client tests (requires running server)
test-rust-client:
	@echo "Running Rust client tests..."
	cd clients/rust && cargo test --tests

# Run Node.js client tests (requires running server)
test-node-client:
	@echo "Running Node.js client tests..."
	cd clients/nodejs && npm test

# Run all client tests (requires running server)
test-clients: test-rust-client test-node-client

# Run all tests including clients (requires running server)
test-all: test test-integration test-tls test-clients

# Build release binary
release:
	@echo "Building release..."
	cargo build --release

# Run release binary
run: release
	@echo "Running release binary..."
	./target/release/kv-storage

# Run with custom token
run-dev:
	@echo "Starting server with TOKEN=test-token..."
	TOKEN=test-token DB_PATH=./dev_db BIND_ADDR=127.0.0.1:3000 cargo run

# Clean build artifacts
clean:
	@echo "Cleaning..."
	cargo clean
	rm -rf ./kv_db ./dev_db

# Check code without building
check:
	@echo "Checking code..."
	cargo check

# Format code
fmt:
	@echo "Formatting code..."
	cargo fmt

# Lint code
clippy:
	@echo "Running Clippy..."
	cargo clippy -- -D warnings

# Update dependencies
update:
	@echo "Updating dependencies..."
	cargo update

# Build documentation
docs:
	@echo "Building documentation..."
	cargo doc --no-deps --open

# Run with example configuration
example:
	@echo "Starting with example config..."
	./scripts/start-example.sh

# Benchmark tests (clean op/s summary)
bench:
	@echo "Running benchmarks (op/s summary)..."
	@./scripts/bench-summary.sh

# Benchmark tests (full detailed report)
bench-full:
	@echo "Running benchmarks (full report)..."
	cargo bench --bench kv_bench

# Show benchmark results from last run
bench-report:
	@echo "=== Benchmark Results Summary ==="
	@echo ""
	@if [ -d target/criterion ]; then \
		find target/criterion -name "report.json" -exec sh -c ' \
			b=$$(basename $$(dirname "$$1")); \
			t=$$(jq -r ".[].throughput.value // .[].mean.point_estimate // empty" "$$1" 2>/dev/null); \
			if [ -n "$$t" ]; then \
				printf "  %-30s %s\n" "$$b:" "$$t"; \
			fi \
		' _ {} \;; \
	else \
		echo "No benchmark results found. Run 'make bench' first."; \
	fi

# Show database stats
stats:
	@echo "Database statistics..."
	@ls -lh ./kv_db 2>/dev/null || echo "No database found at ./kv_db"

# Install locally
install: release
	@echo "Installing to /usr/local/bin..."
	sudo cp ./target/release/kv-storage /usr/local/bin/kv-storage

# Uninstall
uninstall:
	@echo "Uninstalling from /usr/local/bin..."
	sudo rm -f /usr/local/bin/kv-storage

# Docker targets
docker-build:
	@echo "Building Docker image..."
	docker build -t kv-storage:latest .

docker-run:
	@echo "Running Docker container..."
	docker run -d --name kv-storage -p 3000:3000 \
		-e TOKEN=docker-token \
		-v kv-data:/data \
		kv-storage:latest

docker-stop:
	@echo "Stopping Docker container..."
	docker stop kv-storage || true
	docker rm kv-storage || true

docker-logs:
	docker logs -f kv-storage

docker-shell:
	docker exec -it kv-storage sh

docker-clean:
	@echo "Cleaning Docker resources..."
	docker stop kv-storage || true
	docker rm kv-storage || true
	docker rmi kv-storage:latest || true
	docker volume rm kv-data || true

# Help target
help:
	@echo "KV Storage - Makefile targets:"
	@echo ""
	@echo "  build          - Build in debug mode"
	@echo "  dev            - Run in development mode"
	@echo "  test           - Run unit tests only (integration tests require running server)"
	@echo "  test-verbose   - Run all tests with output"
	@echo "  test-unit      - Run only unit tests"
	@echo "  test-integration - Run integration tests"
	@echo "  test-tls       - Run TLS integration tests (generates self-signed cert)"
	@echo "  test-rust-client - Run Rust client tests (requires running server)"
	@echo "  test-node-client - Run Node.js client tests (requires running server)"
	@echo "  test-clients   - Run all client tests (requires running server)"
	@echo "  test-all       - Run all tests including clients (requires running server)"
	@echo "  release        - Build release binary"
	@echo "  run            - Build and run release binary"
	@echo "  run-dev        - Run with test-token on port 3000"
	@echo "  clean          - Clean build artifacts and database"
	@echo "  check          - Check code without building"
	@echo "  fmt            - Format code"
	@echo "  clippy         - Run Clippy linter"
	@echo "  update         - Update dependencies"
	@echo "  docs           - Build and open documentation"
	@echo "  bench          - Run benchmarks (op/s summary)"
	@echo "  bench-full     - Run benchmarks (full detailed report)"
	@echo "  bench-report   - Show results from last run"
	@echo "  stats          - Show database statistics"
	@echo "  install        - Install to /usr/local/bin"
	@echo "  uninstall      - Uninstall from /usr/local/bin"
	@echo "  docker-build   - Build Docker image"
	@echo "  docker-run     - Run Docker container (detached)"
	@echo "  docker-stop    - Stop and remove Docker container"
	@echo "  docker-logs    - Show Docker container logs"
	@echo "  docker-shell   - Open shell in running container"
	@echo "  docker-clean   - Remove all Docker resources"
	@echo "  help           - Show this help message"
