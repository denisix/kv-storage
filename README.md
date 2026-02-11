# KV Storage - High-Performance Key-Value Store

A high-performance key-value storage system built in Rust with HTTP/2 support, content deduplication, and atomic operations.

## Features

- **HTTP/2 Protocol** - Efficient binary streaming and multiplexing
- **Content Deduplication** - Identical objects stored only once using SHA-256 content addressing
- **Atomic Writes** - Sled's ACID transactions prevent race conditions
- **Compression** - Zstd compression for efficient storage
- **Token-based Authentication** - Simple Bearer token authorization
- **Prometheus Metrics** - Built-in metrics endpoint
- **Batch Operations** - Execute multiple operations atomically
- **Key Listing** - Paginated key enumeration

## Quick Start

```bash
# Set your authentication token
export TOKEN="your-secret-token"

# Run the server
cargo run --release

# In another terminal, test it
curl -X PUT http://localhost:3000/mykey \
  -H "Authorization: Bearer your-secret-token" \
  -d "Hello, World!"

curl http://localhost:3000/mykey \
  -H "Authorization: Bearer your-secret-token"
```

## API Reference

### PUT /{key}
Store a value.

```bash
curl -X PUT http://localhost:3000/mykey \
  -H "Authorization: Bearer TOKEN" \
  -H "Content-Type: application/octet-stream" \
  --data-binary @file.bin
```

Returns: `201 Created` with SHA-256 hash

### GET /{key}
Retrieve a value.

```bash
curl http://localhost:3000/mykey \
  -H "Authorization: Bearer TOKEN" \
  -o output.bin
```

### DELETE /{key}
Delete a key.

```bash
curl -X DELETE http://localhost:3000/mykey \
  -H "Authorization: Bearer TOKEN"
```

### HEAD /{key}
Get metadata without body.

```bash
curl -I http://localhost:3000/mykey \
  -H "Authorization: Bearer TOKEN"
```

Headers:
- `X-Content-Sha256` - Content hash
- `X-Refs` - Reference count
- `X-Created-At` - Creation timestamp
- `Content-Length` - Original size

### GET /keys?offset=N&limit=M
List all keys with pagination.

```bash
curl "http://localhost:3000/keys?limit=10" \
  -H "Authorization: Bearer TOKEN"
```

### POST /batch
Execute multiple operations atomically.

```bash
curl -X POST http://localhost:3000/batch \
  -H "Authorization: Bearer TOKEN" \
  -H "Content-Type: application/json" \
  -d '[
    {"op": "put", "key": "key1", "value": "value1"},
    {"op": "put", "key": "key2", "value": "value2"},
    {"op": "get", "key": "key1"}
  ]'
```

### GET /metrics
Prometheus metrics.

```bash
curl http://localhost:3000/metrics \
  -H "Authorization: Bearer TOKEN"
```

Metrics:
- `kv_storage_keys_total` - Total number of keys
- `kv_storage_objects_total` - Total unique objects (after deduplication)
- `kv_storage_bytes_total` - Total storage bytes
- `kv_storage_ops_total{operation="put|get|delete"}` - Operation counters
- `kv_storage_dedup_hits_total` - Deduplication hits

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `TOKEN` | *required* | Authentication token |
| `DB_PATH` | `./kv_db` | Database storage path |
| `BIND_ADDR` | `0.0.0.0:3000` | Server bind address |
| `COMPRESSION_LEVEL` | `1` | Zstd compression level (1-21) |

## Running Tests

```bash
# Start server in one terminal
TOKEN=test-token cargo run --release

# Run tests in another
TOKEN=test-token cargo test -- --ignored
```

## Storage Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Sled Database                             │
├─────────────────────────────────────────────────────────────────┤
│  Tree: "keys"     ┌───────────────────────────────────────────┐ │
│                   │ key → {hash: [u8; 32], size: u64, refs}    │ │
│                   └───────────────────────────────────────────┘ │
│                                                                   │
│  Tree: "objects"  ┌───────────────────────────────────────────┐ │
│                   │ hash → compressed binary data               │ │
│                   └───────────────────────────────────────────┘ │
│                                                                   │
│  Tree: "refs"     ┌───────────────────────────────────────────┐ │
│                   │ hash → Set<key>  (for reverse lookup)      │ │
│                   └───────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

## License

MIT
