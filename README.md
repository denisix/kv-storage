# KV Storage

High-performance key-value storage system built in Rust with HTTP/2, content deduplication, atomic writes, and Zstd compression.

## Features

- **HTTP/2 (h2c)** - Binary framing and multiplexing over plaintext
- **Content Deduplication** - Identical data stored once via xxHash3-128 content addressing
- **Atomic Writes** - Sled ACID transactions prevent race conditions
- **Zstd Compression** - Transparent compression with smart thresholds (skip <512B, inline <=64KB, async >64KB)
- **Security** - Constant-time token comparison, memory zeroing for credentials
- **Prometheus Metrics** - Built-in `/metrics` endpoint
- **Batch Operations** - Multiple ops in a single request
- **Paginated Key Listing** - Offset/limit enumeration

## Quick Start

```bash
# Set your authentication token
export TOKEN="your-secret-token"

# Run the server
cargo run --release

# Store a value (server is HTTP/2 only — use --http2-prior-knowledge for h2c)
curl --http2-prior-knowledge -X PUT http://localhost:3000/mykey \
  -H "Authorization: Bearer your-secret-token" \
  -d "Hello, World!"

# Retrieve it
curl --http2-prior-knowledge http://localhost:3000/mykey \
  -H "Authorization: Bearer your-secret-token"
```

## Docker

```bash
# Build
docker build -t kv-storage:latest .

# Run (TOKEN is required)
docker run -d --name kv-storage -p 3000:3000 \
  -e TOKEN=your-secret-token \
  -v kv-data:/data \
  kv-storage:latest

# Or use docker compose
TOKEN=your-secret-token docker compose up -d
```

## API Reference

All endpoints require `Authorization: Bearer <TOKEN>` and HTTP/2 prior knowledge (`--http2-prior-knowledge` with curl).

### PUT /{key}

Store a value. Returns the xxHash3-128 hash.

```bash
curl --http2-prior-knowledge -X PUT http://localhost:3000/mykey \
  -H "Authorization: Bearer TOKEN" \
  --data-binary @file.bin
```

**Response**: `201 Created` (new object) or `200 OK` (deduplicated)

**Headers**: `X-Hash`, `X-Hash-Algorithm`, `X-Deduplicated`

### GET /{key}

Retrieve a value.

```bash
curl --http2-prior-knowledge http://localhost:3000/mykey \
  -H "Authorization: Bearer TOKEN" -o output.bin
```

### DELETE /{key}

Delete a key. Object is garbage-collected when no keys reference it.

```bash
curl --http2-prior-knowledge -X DELETE http://localhost:3000/mykey \
  -H "Authorization: Bearer TOKEN"
```

**Response**: `204 No Content`

### HEAD /{key}

Metadata without body.

```bash
curl --http2-prior-knowledge -I http://localhost:3000/mykey \
  -H "Authorization: Bearer TOKEN"
```

**Headers**: `X-Hash`, `X-Refs`, `X-Created-At`, `Content-Length`

### GET /keys?offset=N&limit=M

Paginated key listing. Default limit: 100, max: 1000.

```bash
curl --http2-prior-knowledge "http://localhost:3000/keys?limit=10" \
  -H "Authorization: Bearer TOKEN"
```

### POST /batch

Multiple operations in a single request.

```bash
curl --http2-prior-knowledge -X POST http://localhost:3000/batch \
  -H "Authorization: Bearer TOKEN" \
  -H "Content-Type: application/json" \
  -d '[
    {"op": "put", "key": "k1", "value": "v1"},
    {"op": "get", "key": "k1"},
    {"op": "delete", "key": "k1"}
  ]'
```

### GET /metrics

Prometheus-format metrics.

```bash
curl --http2-prior-knowledge http://localhost:3000/metrics \
  -H "Authorization: Bearer TOKEN"
```

Exported metrics:
- `kv_storage_keys_total` - Number of keys (gauge)
- `kv_storage_objects_total` - Unique objects after dedup (gauge)
- `kv_storage_bytes_total` - Storage bytes (gauge)
- `kv_storage_ops_total{operation="put|get|delete"}` - Op counters
- `kv_storage_dedup_hits_total` - Dedup hits counter

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `TOKEN` | *required* | Authentication token |
| `DB_PATH` | `./kv_db` | Database storage path |
| `BIND_ADDR` | `0.0.0.0:3000` | Server bind address |
| `COMPRESSION_LEVEL` | `1` | Zstd level (clamped to 1-3) |
| `KV_CACHE_CAPACITY` | `1073741824` | Sled cache size in bytes (1GB) |
| `KV_FLUSH_INTERVAL_MS` | `100` | Sled flush interval in ms |

## Development

```bash
make build          # Debug build
make test           # Unit tests (41 tests)
make test-integration  # Integration tests (requires running server)
make clippy         # Lint
make bench          # Criterion benchmarks
make run-dev        # Dev server with TOKEN=test-token
```

## Storage Architecture

```
Sled Database (3 trees)
├── keys:    key (string)  -> KeyMeta {hash: [u8; 16], size, refs, created_at}
├── objects: hash (16B)    -> compressed binary data
└── refs:    hash + key    -> "1" (reverse lookup for GC)
```

Deduplication: multiple keys can point to the same object hash. Objects are garbage-collected when the last referencing key is deleted.

## Client Libraries

- **Rust** - `clients/rust/` - Async HTTP/2 client with full API coverage
- **Node.js** - `clients/nodejs/` - TypeScript/JavaScript HTTP/2 client

## License

MIT
