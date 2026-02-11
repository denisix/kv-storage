# kv-storage-client

A modern, high-performance Node.js client for the [kv-storage](https://github.com/yourusername/kv-storage) HTTP/2 key-value storage server.

# kv-storage-client

A modern, high-performance Node.js client for the [kv-storage](https://github.com/yourusername/kv-storage) HTTP/2 key-value storage server.

## Features

- 🚀 **Native HTTP/2** - Uses Node.js built-in `http2` module for optimal performance
- 📦 **Full TypeScript Support** - Fully typed for excellent developer experience
- 🔐 **Authentication** - Secure token-based authentication
- ⚡ **Fast Operations** - Optimized for high-throughput read/write workloads
- 🔄 **Batch Operations** - Atomic multi-operation support
- 📊 **Metrics** - Built-in Prometheus metrics support
- 🔍 **Pagination** - Efficient key listing with pagination
- 🎯 **Zero Runtime Dependencies** - Uses only Node.js built-in modules

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
await client.put('user:123', JSON.stringify({ name: 'John', age: 30 }));

// Retrieve a value
const value = await client.get('user:123');
if (value) {
  const user = JSON.parse(value);
  console.log(user.name); // "John"
}

// Clean up when done
client.close();
```

## API Reference

### Constructor Options

```typescript
interface KVStorageOptions {
  endpoint?: string;              // Server URL (default: http://localhost:3000)
  token: string;                  // Authentication token (required)
  timeout?: number;               // Request timeout in ms (default: 30000)
  maxConcurrentStreams?: number;  // Max HTTP/2 concurrent streams (default: 100)
  sessionTimeout?: number;        // Session idle timeout in ms (default: 60000)
  rejectUnauthorized?: boolean;   // TLS verification (default: true)
}
```

### Methods

#### `put(key, value)`

Store a value with a key.

```typescript
// Text value
await client.put('my-key', 'my-value');

// Binary value (Buffer)
const data = Buffer.from([0, 1, 2, 3]);
await client.put('binary-key', data);

// Returns: { hash: string, hash_algorithm: string, deduplicated: boolean }
```

#### `get(key, encoding?)`

Retrieve a value by key.

```typescript
// Get as text (UTF-8)
const text = await client.get('my-key', 'utf-8');

// Get as binary
const binary = await client.get('binary-key', 'binary');

// Returns: string | Buffer | null
```

#### `delete(key)`

Delete a key.

```typescript
const deleted = await client.delete('my-key');
// Returns: boolean
```

#### `head(key)`

Get metadata about a key without retrieving the value.

```typescript
const info = await client.head('my-key');
// Returns: { 'content-length': string, 'x-refs': string, 'x-content-sha256': string } | null
```

#### `list(options?)`

List all keys with pagination.

```typescript
// Get first 100 keys
const result = await client.list();

// Get next page
const page2 = await client.list({ offset: 100, limit: 50 });

// Returns: { keys: KeyInfo[], total: number }
```

#### `listAll(pageSize?)`

Async iterator for listing all keys with automatic pagination.

```typescript
for await (const keys of client.listAll(100)) {
  for (const key of keys) {
    console.log(key.key, key.size);
  }
}
```

#### `batch(operations)`

Execute multiple operations atomically.

```typescript
const results = await client.batch([
  { op: 'put', key: 'user:1', value: '{"name":"John"}' },
  { op: 'put', key: 'user:2', value: '{"name":"Jane"}' },
  { op: 'get', key: 'user:1' },
  { op: 'delete', key: 'old-key' }
]);

// Returns: { results: BatchResult[] }
```

#### `metrics()`

Get Prometheus metrics from the server.

```typescript
const metrics = await client.metrics();
// Returns: string (Prometheus text format)
```

#### `healthCheck()`

Check if the server is accessible.

```typescript
const healthy = await client.healthCheck();
// Returns: boolean
```

#### `close()`

Close the HTTP/2 session and cleanup resources.

```typescript
client.close();
```

## Advanced Usage

### HTTP/2 Connection Pooling

The client maintains a persistent HTTP/2 connection with configurable limits:

```typescript
const client = new KVStorage({
  endpoint: 'https://kv-storage.example.com',
  token: process.env.KV_TOKEN,
  maxConcurrentStreams: 200,  // Allow up to 200 concurrent requests
  sessionTimeout: 120000,      // Keep session alive for 2 minutes idle
});

// Client automatically handles session reuse
const promises = [];
for (let i = 0; i < 1000; i++) {
  promises.push(client.put(`key:${i}`, `value:${i}`));
}
await Promise.all(promises);

// Clean up when done
client.close();
```

### Binary Data

```typescript
import { readFile } from 'fs/promises';

// Store binary data
const imageBuffer = await readFile('image.png');
await client.put('images:logo', imageBuffer);

// Retrieve binary data
const retrieved = await client.get('images:logo', 'binary');
if (Buffer.isBuffer(retrieved)) {
  await writeFile('logo.png', retrieved);
}
```

### Batch Operations

```typescript
// Atomic multi-operation
const results = await client.batch([
  { op: 'put', key: 'cache:user:1', value: userData },
  { op: 'put', key: 'cache:user:2', value: userData },
  { op: 'put', key: 'cache:user:3', value: userData },
]);

for (const result of results.results) {
  if (result.error) {
    console.error(`Failed: ${result.error}`);
  } else {
    console.log(`${result.op} on ${result.key} succeeded`);
  }
}
```

### Pagination

```typescript
// Process all keys efficiently
let offset = 0;
const limit = 100;

while (true) {
  const { keys, total } = await client.list({ offset, limit });

  for (const key of keys) {
    await processKey(key);
  }

  if (keys.length < limit) break;
  offset += limit;
}

// Or use the iterator
for await (const keys of client.listAll(100)) {
  for (const key of keys) {
    await processKey(key);
  }
}
```

### Error Handling

```typescript
try {
  await client.put('my-key', 'my-value');
} catch (error) {
  if (error instanceof Error) {
    if (error.message.includes('Unauthorized')) {
      console.error('Invalid token');
    } else if (error.message.includes('timeout')) {
      console.error('Request timed out');
    } else if (error.message.includes('ECONNREFUSED')) {
      console.error('Cannot connect to server');
    } else {
      console.error('Error:', error.message);
    }
  }
}
```

## HTTP/2 Benefits

Using the native `http2` module provides:

- **Multiplexing** - Multiple concurrent requests over a single connection
- **Header Compression** - Reduced bandwidth usage
- **Server Push** - Future support for server-sent updates
- **Stream Priorities** - Priority-based request handling
- **Connection Reuse** - Persistent connections with automatic management

## Performance Tips

1. **Increase `maxConcurrentStreams`** for high-throughput scenarios
2. **Use batch operations** - Combine multiple operations into a single request
3. **Set appropriate `sessionTimeout`** - Balance between resource usage and latency
4. **Call `close()` when done** - Properly cleanup HTTP/2 sessions

## Testing

```bash
# Start the kv-storage server
TOKEN=test-token cargo run

# Run tests in another terminal
npm test
```

## License

MIT

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

