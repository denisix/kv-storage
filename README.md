# KV Storage

High-performance key-value storage system built in Rust with HTTP/2, content deduplication, atomic writes, and Zstd compression.

## Features

- **HTTP/2 (h2c/h2)** - Binary framing and multiplexing over plaintext or TLS
- **TLS/SSL Support** - HTTPS with optional certificate fingerprint pinning
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

# Run the server (HTTP/2 cleartext mode)
cargo run --release

# Store a value (server is HTTP/2 only — use --http2-prior-knowledge for h2c)
curl --http2-prior-knowledge -X PUT http://localhost:3000/mykey \
  -H "Authorization: Bearer your-secret-token" \
  -d "Hello, World!"

# Retrieve it
curl --http2-prior-knowledge http://localhost:3000/mykey \
  -H "Authorization: Bearer your-secret-token"
```

For HTTPS mode, set `SSL_CERT` and `SSL_KEY` (see [TLS/SSL](#tlsssl) below).

## Docker

```bash
# Run the official image (TOKEN is required)
docker run -d --name kv-storage -p 3000:3000 \
  -e TOKEN=your-secret-token \
  -v kv-data:/data \
  ghcr.io/denisix/kv-storage:latest

# With TLS support (runs HTTP + HTTPS servers)
docker run -d --name kv-storage \
  -p 3000:3000 \
  -p 3443:3443 \
  -e TOKEN=your-secret-token \
  -e SSL_CERT=/certs/cert.pem \
  -e SSL_KEY=/certs/key.pem \
  -v kv-data:/data \
  -v ./certs:/certs:ro \
  ghcr.io/denisix/kv-storage:latest

# Or use docker compose
TOKEN=your-secret-token docker compose up -d
```

To build locally instead:

```bash
docker build -t kv-storage:latest .
```

## Key Names

Keys can be any valid URI path. The server stores the key exactly as it appears in the URI (no percent-decoding). Any UTF-8 string can be used as a key through the client libraries, which handle percent-encoding automatically for HTTP transport.

```bash
# Simple key
curl --http2-prior-knowledge -X PUT http://localhost:3000/mykey ...

# Key with slashes, dots, colons
curl --http2-prior-knowledge -X PUT http://localhost:3000/path/to/file.txt ...
curl --http2-prior-knowledge -X PUT http://localhost:3000/user:123 ...

# Key with spaces (percent-encoded as %20)
curl --http2-prior-knowledge -X PUT http://localhost:3000/my%20key ...

# Key with special characters
curl --http2-prior-knowledge -X PUT http://localhost:3000/key%23with%23hash ...
```

When using the client libraries, encoding is handled transparently:

```typescript
// Node.js — spaces, unicode, etc. are encoded automatically
await client.put('my key', 'value');
await client.put('path/to/file.txt', data);
await client.put('ключ', data);
```

```rust
// Rust — same automatic encoding
client.put("my key", b"value").await?;
client.put("path/to/file.txt", &data).await?;
client.put("ключ", &data).await?;
```

Key constraints:
- Cannot be empty
- Max 256 KB
- No control characters (except tab)

## API Reference

All endpoints require `Authorization: Bearer <TOKEN>` and HTTP/2. Use `--http2-prior-knowledge` for plaintext h2c connections, or standard HTTPS with curl's native HTTP/2 support.

### PUT /{key}

Store a value. Returns the xxHash3-128 hash.

```bash
# Plaintext (h2c)
curl --http2-prior-knowledge -X PUT http://localhost:3000/mykey \
  -H "Authorization: Bearer TOKEN" \
  --data-binary @file.bin

# HTTPS
curl -X PUT https://localhost:3000/mykey \
  -H "Authorization: Bearer TOKEN" \
  --data-binary @file.bin
```

**Response**: `201 Created` (new object) or `200 OK` (deduplicated)

**Headers**: `X-Hash`, `X-Hash-Algorithm`, `X-Deduplicated`

### GET /{key}

Retrieve a value.

```bash
# Plaintext (h2c)
curl --http2-prior-knowledge http://localhost:3000/mykey \
  -H "Authorization: Bearer TOKEN" -o output.bin

# HTTPS
curl https://localhost:3000/mykey \
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
| `PORT` | `3000` | HTTP/2 cleartext (h2c) port |
| `SSL_PORT` | `3443` | HTTPS port (only when SSL_CERT/SSL_KEY set) |
| `HOST` | `0.0.0.0` | Host to bind servers to |
| `BIND_ADDR` | `0.0.0.0:3000` | Legacy: host:port (PORT extracts from here if set) |
| `COMPRESSION_LEVEL` | `1` | Zstd level: 0 = off, 1-9 = compression |
| `SSL_CERT` | *unset* | Path to PEM certificate file (enables HTTPS) |
| `SSL_KEY` | *unset* | Path to PEM private key file (enables HTTPS) |
| `KV_CACHE_CAPACITY` | `1073741824` | Sled cache size in bytes (1GB) |
| `KV_FLUSH_INTERVAL_MS` | `1000` | Sled flush interval in ms |

## TLS/SSL

The server supports running both HTTP (h2c) and HTTPS (h2) simultaneously. When `SSL_CERT` and `SSL_KEY` are set, the HTTPS server starts on `SSL_PORT` alongside the HTTP server on `PORT`.

### HTTP Only (default)

```bash
export TOKEN="your-secret-token"
cargo run --release
# Listens on http://0.0.0.0:3000
```

### HTTP + HTTPS (both servers)

```bash
export TOKEN="your-secret-token"
export SSL_CERT="/path/to/cert.pem"
export SSL_KEY="/path/to/key.pem"
export SSL_PORT="443"  # Optional, defaults to 3443
cargo run --release
# Listens on http://0.0.0.0:3000 (h2c)
# AND https://0.0.0.0:443 (h2)
```

### Generating a Self-Signed Certificate

For testing, generate a self-signed certificate:

```bash
openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365 -nodes -subj "/CN=localhost"
```

### Using curl with HTTPS

```bash
# With a trusted certificate
curl -X PUT https://localhost:3000/mykey \
  -H "Authorization: Bearer TOKEN" \
  --data-binary @file.bin

# With a self-signed certificate (skip verification)
curl -X PUT https://localhost:3000/mykey \
  -H "Authorization: Bearer TOKEN" \
  --data-binary @file.bin \
  -k

# With certificate fingerprint pinning (SHA-256)
curl -X PUT https://localhost:3000/mykey \
  -H "Authorization: Bearer TOKEN" \
  --data-binary @file.bin \
  --pinnedpubkey "sha256//$(openssl x509 -in cert.pem -noout -fingerprint -sha256 | cut -d= -f2 | tr -d :)"
```

### Certificate Fingerprint Pinning

For enhanced security, clients can pin the server's certificate by its SHA-256 fingerprint. This protects against man-in-the-middle attacks even with self-signed certificates.

**Get the certificate fingerprint:**

```bash
# Using openssl
openssl x509 -in cert.pem -noout -fingerprint -sha256

# Or convert to the format needed by clients (lowercase hex, no colons)
openssl x509 -in cert.pem -noout -fingerprint -sha256 | cut -d= -f2 | tr -d : | tr '[:upper:]' '[:lower:]'
```

**Rust client with fingerprint pinning:**

```rust
use kv_storage_client::{Client, ClientConfig};

let client = Client::with_config(ClientConfig {
    endpoint: "https://localhost:3000".to_string(),
    token: "your-token".to_string(),
    ssl_fingerprint: Some("abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".to_string()),
    ..Default::default()
})?;
```

**Node.js client with fingerprint pinning:**

```typescript
import { KVStorage } from '@kv-storage/client';

const client = new KVStorage({
  endpoint: 'https://localhost:3000',
  token: 'your-token',
  sslFingerprint: 'AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89'
});
```

## Development

```bash
make build             # Debug build
make test              # Unit tests
make test-integration  # Integration tests (requires running server)
make test-tls          # TLS integration tests (generates self-signed cert)
make clippy            # Lint
make bench             # Criterion benchmarks
make run-dev           # Dev server with TOKEN=test-token
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

- **Rust** - `clients/rust/` - Async HTTP/2 client with full API coverage, automatic key encoding, TLS support, and certificate fingerprint pinning
- **Node.js** - `clients/nodejs/` - TypeScript/JavaScript HTTP/2 client with automatic key encoding, TLS support, and certificate fingerprint pinning

## License

MIT
