# kv-storage-client

Node.js/TypeScript HTTP/2 client for the [kv-storage](https://github.com/denisix/kv-storage) server.

## Features

- **Native HTTP/2** - Uses Node.js built-in `node:http2` for h2c multiplexing
- **Full TypeScript** - Typed interfaces for all operations
- **Connection Pooling** - Persistent session with automatic reconnect and idle timeout
- **Batch Operations** - Multiple ops in a single request
- **Async Iteration** - `listAll()` generator for paginated key enumeration
- **Zero Runtime Dependencies** - Only Node.js built-in modules

Requires Node.js >= 18.

## Installation

```bash
npm install kv-storage-client
```

## Quick Start

```typescript
import { KVStorage } from 'kv-storage-client';

const client = new KVStorage({
  endpoint: 'http://localhost:3000',
  token: 'your-secret-token'
});

// Store a value
const { hash } = await client.put('user:123', JSON.stringify({ name: 'John' }));
console.log(hash); // xxHash3-128 hex string

// Retrieve a value
const value = await client.get('user:123');
if (value) {
  console.log(JSON.parse(value));
}

// Clean up
client.close();
```

## API Reference

### Constructor

```typescript
const client = new KVStorage({
  endpoint: 'http://localhost:3000', // Server URL (default)
  token: 'secret',                   // Required
  timeout: 30000,                    // Request timeout ms (default)
  maxConcurrentStreams: 100,         // HTTP/2 streams (default)
  sessionTimeout: 60000,            // Idle session timeout ms (default)
  rejectUnauthorized: true,          // TLS verification (default)
  sslFingerprint: 'AB:CD:EF:...',    // Certificate pinning (optional)
});
```

### `put(key, value)` -> `PutResponse`

Store a value. Accepts `string`, `Buffer`, or `Uint8Array`.

```typescript
const result = await client.put('my-key', 'my-value');
// { hash: string, hash_algorithm: string, deduplicated: boolean }

// Binary data
await client.put('bin-key', Buffer.from([0, 1, 2, 3]));
```

### `get(key, encoding?)` -> `string | Buffer | null`

Retrieve a value. Returns `null` if key doesn't exist.

```typescript
const text = await client.get('my-key');           // UTF-8 string
const buf = await client.get('bin-key', 'binary'); // Buffer
```

### `delete(key)` -> `boolean`

Delete a key. Returns `true` if deleted, `false` if not found.

```typescript
await client.delete('my-key');
```

### `head(key)` -> `HeadInfo | null`

Get metadata without the value body.

```typescript
const info = await client.head('my-key');
// { 'content-length': string, 'x-refs': string, 'x-hash': string }
```

### `list(options?)` -> `ListResponse`

Paginated key listing.

```typescript
const { keys, total } = await client.list({ offset: 0, limit: 50 });
// keys: [{ key, size, hash, hash_algorithm, refs, created_at }]
```

### `listAll(pageSize?)` -> `AsyncGenerator<KeyInfo[]>`

Async iterator for automatic pagination.

```typescript
for await (const keys of client.listAll(100)) {
  for (const key of keys) {
    console.log(key.key, key.size);
  }
}
```

### `batch(operations)` -> `BatchResponse`

Multiple operations in a single request.

```typescript
const { results } = await client.batch([
  { op: 'put', key: 'k1', value: 'v1' },
  { op: 'get', key: 'k1' },
  { op: 'delete', key: 'old' }
]);
```

Each result is a tagged object: `{ put: {...} }`, `{ get: {...} }`, `{ delete: {...} }`, or `{ error: {...} }`.

### `metrics()` -> `string`

Prometheus-format metrics text.

### `healthCheck()` -> `boolean`

Returns `true` if the server is reachable.

### `close()`

Close the HTTP/2 session and release resources. Always call this when done.

## Advanced Usage

### High-Throughput Writes

```typescript
const client = new KVStorage({
  endpoint: 'http://localhost:3000',
  token: process.env.KV_TOKEN,
  maxConcurrentStreams: 200,
});

// HTTP/2 multiplexing allows parallel requests on one connection
const promises = Array.from({ length: 1000 }, (_, i) =>
  client.put(`key:${i}`, `value:${i}`)
);
await Promise.all(promises);

client.close();
```

### TLS/SSL with Certificate Pinning

For HTTPS endpoints, you can enable certificate fingerprint pinning to protect against
man-in-the-middle attacks:

```typescript
const client = new KVStorage({
  endpoint: 'https://localhost:3000',
  token: 'secret',
  sslFingerprint: 'AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89'
});
```

#### Getting the Server Certificate Fingerprint

From your server's certificate file:

```bash
# Get SHA-256 fingerprint (with colons)
openssl x509 -in cert.pem -noout -fingerprint -sha256


# Convert to lowercase hex without colons (optional format)
openssl x509 -in cert.pem -noout -fingerprint -sha256 | cut -d= -f2 | tr -d : | tr '[:upper:]' '[:lower:]'
```

Or directly from a running HTTPS server:

```bash
# Get fingerprint from running server (requires --insecure for self-signed)
openssl s_client -connect localhost:3000 -servername localhost 2>/dev/null | \
  openssl x509 -noout -fingerprint -sha256
```

The `sslFingerprint` option accepts either format:
- With colons: `'AB:CD:EF:01:23...'` (standard openssl output)
- Without colons: `'abcdef0123456789...'` (clean hex string)

When set, the client will:
1. Skip standard CA certificate verification
2. Verify that the server's certificate SHA-256 fingerprint matches exactly
3. Reject connections with mismatched certificates

This is especially useful for self-signed certificates or environments where
you want to pin to a specific certificate rather than trusting a CA.

### Binary Files

```typescript
import { readFile, writeFile } from 'fs/promises';

await client.put('images:logo', await readFile('logo.png'));

const data = await client.get('images:logo', 'binary');
if (Buffer.isBuffer(data)) {
  await writeFile('logo-copy.png', data);
}
```

### Error Handling

```typescript
try {
  await client.put('my-key', 'value');
} catch (error) {
  if (error.message.includes('Unauthorized')) {
    // Invalid token
  } else if (error.message.includes('timeout')) {
    // Request timed out
  } else if (error.message.includes('ECONNREFUSED')) {
    // Server unreachable
  }
}
```

## Development

```bash
# Start the kv-storage server (HTTP/2 only)
TOKEN=test-token cargo run --release

# Build
npm run build

# Run tests (requires running server)
npm test

# Run example
npm run example
```

## License

MIT
