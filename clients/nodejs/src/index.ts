/**
 * Fast KV Storage Client for Node.js
 *
 * A modern HTTP/2 client for the kv-storage server using native node:http2
 */

import http2 from 'node:http2';
import { URL } from 'node:url';
import type { ClientSessionOptions, SecureClientSessionOptions, IncomingHttpHeaders, OutgoingHttpHeaders } from 'node:http2';

interface PutResponse {
  hash: string;
  hash_algorithm: string;
  deduplicated: boolean;
}

interface KeyInfo {
  key: string;
  size: number;
  hash: string;
  hash_algorithm: string;
  refs: number;
  created_at: number;
}

interface ListResponse {
  keys: KeyInfo[];
  total: number;
}

type BatchOp =
  | { op: 'put'; key: string; value: string | ArrayBuffer }
  | { op: 'get'; key: string }
  | { op: 'delete'; key: string };

// Server returns nested objects with operation names as keys:
// { "put": { key, hash, created } } or { "error": { key, error } }
type BatchResult =
  | { put: { key: string; hash: string; created: boolean } }
  | { get: { key: string; value?: string; found: boolean } }
  | { delete: { key: string; deleted: boolean } }
  | { error: { key: string; error: string } };

interface BatchResponse {
  results: BatchResult[];
}

interface HeadInfo {
  'content-length': string;
  'x-refs': string;
  'x-content-sha256': string;
}

/**
 * Configuration options for the KV Storage client
 */
export interface KVStorageOptions {
  /**
   * Server endpoint URL (default: http://localhost:3000)
   */
  endpoint?: string;
  /**
   * Authentication token
   */
  token: string;
  /**
   * Request timeout in milliseconds (default: 30000)
   */
  timeout?: number;
  /**
   * Maximum concurrent streams per session (default: 100)
   */
  maxConcurrentStreams?: number;
  /**
   * Session timeout in milliseconds (default: 60000)
   */
  sessionTimeout?: number;
  /**
   * Enable TLS verification (default: true)
   */
  rejectUnauthorized?: boolean;
}

interface RequestResult {
  statusCode: number;
  headers: IncomingHttpHeaders;
  body: Buffer;
}

/**
 * Internal HTTP/2 session wrapper
 */
class HTTP2Session {
  private client: http2.ClientHttp2Session;
  private url: URL;
  private lastUsed: number;
  private sessionTimeout: number;
  private idleTimer: ReturnType<typeof setInterval>;

  constructor(
    authority: string,
    options: {
      maxConcurrentStreams?: number;
      rejectUnauthorized?: boolean;
      sessionTimeout?: number;
    }
  ) {
    this.url = new URL(authority);
    this.sessionTimeout = options.sessionTimeout || 60000;
    this.lastUsed = Date.now();

    const isHttps = this.url.protocol === 'https:';

    if (isHttps) {
      const tlsOptions: SecureClientSessionOptions = {};
      if (options.rejectUnauthorized !== undefined) {
        tlsOptions.rejectUnauthorized = options.rejectUnauthorized;
      }
      this.client = http2.connect(authority, tlsOptions);
    } else {
      this.client = http2.connect(authority);
    }

    this.client.on('error', (_err: Error) => {
      // Session error - will be handled at request level
    });

    // Set up timeout to close idle sessions
    this.idleTimer = setInterval(() => {
      if (Date.now() - this.lastUsed > this.sessionTimeout) {
        this.close();
      }
    }, this.sessionTimeout / 2);
    this.idleTimer.unref();
  }

  request(
    method: string,
    path: string,
    headers: Record<string, string>,
    body: Buffer | undefined,
    timeout: number
  ): Promise<RequestResult> {
    return new Promise((resolve, reject) => {
      this.lastUsed = Date.now();

      const reqHeaders: OutgoingHttpHeaders = {
        ':method': method,
        ':path': path,
        ...headers,
      };

      const req = this.client.request(reqHeaders);

      let responseHeaders: IncomingHttpHeaders = {};
      let statusCode = 200;
      const chunks: Buffer[] = [];

      // Set up event handlers in the correct order
      req.on('response', (headers: IncomingHttpHeaders, _flags: number) => {
        responseHeaders = headers;
        statusCode = parseInt((headers[':status'] as string) || '200', 10);
      });

      req.on('data', (chunk: Buffer) => {
        chunks.push(chunk);
      });

      req.on('end', () => {
        clearTimeout(timeoutId);
        resolve({
          statusCode,
          headers: responseHeaders,
          body: Buffer.concat(chunks),
        });
      });

      const timeoutId = setTimeout(() => {
        req.close();
        reject(new Error(`Request timeout after ${timeout}ms`));
      }, timeout);

      req.on('error', (err: Error) => {
        clearTimeout(timeoutId);
        reject(err);
      });

      // Send the request body
      if (body) {
        req.end(body);
      } else {
        req.end();
      }
    });
  }

  close(): void {
    clearInterval(this.idleTimer);
    this.client.destroy();
  }

  isClosed(): boolean {
    return this.client.destroyed;
  }
}

/**
 * Fast KV Storage Client
 *
 * @example
 * ```ts
 * import { KVStorage } from 'node-kv-storage';
 *
 * const client = new KVStorage({
 *   endpoint: 'http://localhost:3000',
 *   token: 'my-secret-token'
 * });
 *
 * await client.put('my-key', 'my-value');
 * const value = await client.get('my-key');
 * ```
 */
export class KVStorage {
  private readonly endpoint: string;
  private readonly token: string;
  private readonly timeout: number;
  private readonly maxConcurrentStreams: number;
  private readonly rejectUnauthorized: boolean;
  private readonly sessionTimeout: number;
  private session: HTTP2Session | null = null;

  constructor(options: KVStorageOptions) {
    this.endpoint = options.endpoint || 'http://localhost:3000';
    this.token = options.token;
    this.timeout = options.timeout || 30000;
    this.maxConcurrentStreams = options.maxConcurrentStreams || 100;
    this.rejectUnauthorized = options.rejectUnauthorized !== false;
    this.sessionTimeout = options.sessionTimeout || 60000;
  }

  private getSession(): HTTP2Session {
    if (this.session && !this.session.isClosed()) {
      return this.session;
    }

    this.session = new HTTP2Session(this.endpoint, {
      maxConcurrentStreams: this.maxConcurrentStreams,
      rejectUnauthorized: this.rejectUnauthorized,
      sessionTimeout: this.sessionTimeout,
    });

    return this.session;
  }

  private async request(
    path: string,
    options: {
      method: string;
      body?: Buffer;
      headers?: Record<string, string>;
      throwOnNotFound?: boolean;
    }
  ): Promise<RequestResult> {
    const session = this.getSession();

    const headers: Record<string, string> = {
      'authorization': `Bearer ${this.token}`,
      ...options.headers,
    };

    try {
      const result = await session.request(
        options.method,
        path,
        headers,
        options.body,
        this.timeout
      );

      if (result.statusCode === 401) {
        throw new Error('Unauthorized: Invalid token');
      }

      if (result.statusCode === 404) {
        if (options.throwOnNotFound !== false) {
          throw new Error(`Not found: ${path}`);
        }
        // Don't throw for 404 when throwOnNotFound is false
        return result;
      }

      if (result.statusCode >= 400) {
        const text = result.body.toString('utf-8');
        throw new Error(`Server error (${result.statusCode}): ${text}`);
      }

      return result;
    } catch (error) {
      if (error instanceof Error && error.message.includes('ECONNREFUSED')) {
        throw new Error(`Failed to connect to ${this.endpoint}`);
      }
      throw error;
    }
  }

  /**
   * Store a value with a key
   *
   * @param key - The key to store the value under
   * @param value - The value to store (string or binary data)
   * @returns Promise with hash information
   *
   * @example
   * ```ts
   * const result = await client.put('user:123', JSON.stringify({ name: 'John' }));
   * console.log(result.hash); // SHA-256 hash of the stored data
   * ```
   */
  async put(key: string, value: string | Buffer | Uint8Array): Promise<PutResponse> {
    const body =
      typeof value === 'string'
        ? Buffer.from(value, 'utf-8')
        : Buffer.from(value.buffer, value.byteOffset, value.byteLength);

    const result = await this.request(`/${key}`, {
      method: 'PUT',
      body,
      headers: {
        'content-type': 'application/octet-stream',
      },
    });

    const hash = result.body.toString('utf-8').trim();
    const deduplicated = result.headers['x-deduplicated'] === 'true';
    const hashAlgorithm = (result.headers['x-hash-algorithm'] as string) || 'sha256';

    return {
      hash,
      hash_algorithm: hashAlgorithm,
      deduplicated,
    };
  }

  /**
   * Retrieve a value by key
   *
   * @param key - The key to retrieve
   * @param encoding - Return as 'utf-8' text or 'binary' buffer (default: 'utf-8')
   * @returns Promise with the value or null if not found
   *
   * @example
   * ```ts
   * const value = await client.get('user:123');
   * if (value) {
   *   const user = JSON.parse(value);
   * }
   *
   * // Get as binary
   * const binary = await client.get('image:logo', 'binary');
   * ```
   */
  async get(
    key: string,
    encoding: 'utf-8' | 'binary' = 'utf-8'
  ): Promise<string | Buffer | null> {
    const result = await this.request(`/${key}`, {
      method: 'GET',
      throwOnNotFound: false,
    });

    if (result.statusCode === 404) {
      return null;
    }

    if (encoding === 'utf-8') {
      return result.body.toString('utf-8');
    }

    return result.body;
  }

  /**
   * Delete a key
   *
   * @param key - The key to delete
   * @returns Promise with true if deleted, false if not found
   *
   * @example
   * ```ts
   * await client.delete('user:123');
   * ```
   */
  async delete(key: string): Promise<boolean> {
    const result = await this.request(`/${key}`, {
      method: 'DELETE',
      throwOnNotFound: false,
    });

    return result.statusCode === 204;
  }

  /**
   * Get metadata about a key without retrieving the value
   *
   * @param key - The key to get head info for
   * @returns Promise with head info or null if not found
   *
   * @example
   * ```ts
   * const info = await client.head('user:123');
   * if (info) {
   *   console.log(`Size: ${info['content-length']} bytes`);
   *   console.log(`Refs: ${info['x-refs']}`);
   * }
   * ```
   */
  async head(key: string): Promise<HeadInfo | null> {
    const result = await this.request(`/${key}`, {
      method: 'HEAD',
      throwOnNotFound: false,
    });

    if (result.statusCode === 404) {
      return null;
    }

    return {
      'content-length': (result.headers['content-length'] as string) || '0',
      'x-refs': (result.headers['x-refs'] as string) || '0',
      'x-content-sha256': (result.headers['x-content-sha256'] as string) || '',
    };
  }

  /**
   * List all keys with pagination
   *
   * @param options - List options
   * @returns Promise with array of key information
   *
   * @example
   * ```ts
   * // Get first 100 keys
   * const result = await client.list();
   *
   * // Get next page
   * const page2 = await client.list({ offset: 100, limit: 50 });
   *
   * // Get all keys (pagination helper)
   * for await (const keys of client.listAll()) {
   *   console.log(keys);
   * }
   * ```
   */
  async list(options?: { offset?: number; limit?: number }): Promise<ListResponse> {
    const params = new URLSearchParams();
    if (options?.offset) params.append('offset', options.offset.toString());
    if (options?.limit)
      params.append('limit', Math.min(options.limit, 1000).toString());

    const path = `/keys${params.toString() ? '?' + params.toString() : ''}`;

    const result = await this.request(path, {
      method: 'GET',
      headers: {
        'accept': 'application/json',
      },
    });

    return JSON.parse(result.body.toString('utf-8')) as ListResponse;
  }

  /**
   * Async iterator to list all keys with automatic pagination
   *
   * @param pageSize - Number of keys per page (default: 100)
   * @returns Async generator yielding key arrays
   *
   * @example
   * ```ts
   * for await (const { keys } of client.listAll(100)) {
   *   for (const key of keys) {
   *     console.log(key.key, key.size);
   *   }
   * }
   * ```
   */
  async *listAll(pageSize = 100): AsyncGenerator<KeyInfo[], void, unknown> {
    let offset = 0;
    const limit = pageSize;

    while (true) {
      const result = await this.list({ offset, limit });
      if (result.keys.length === 0) break;

      yield result.keys;

      if (result.keys.length < limit) break;
      offset += limit;
    }
  }

  /**
   * Execute multiple operations atomically
   *
   * @param operations - Array of batch operations
   * @returns Promise with array of results
   *
   * @example
   * ```ts
   * const results = await client.batch([
   *   { op: 'put', key: 'user:1', value: '{"name":"John"}' },
   *   { op: 'put', key: 'user:2', value: '{"name":"Jane"}' },
   *   { op: 'get', key: 'user:1' },
   *   { op: 'delete', key: 'old-key' }
   * ]);
   *
   * for (const result of results.results) {
   *   if (result.error) {
   *     console.error(`Error on ${result.key}: ${result.error}`);
   *   } else {
   *     console.log(`${result.op} on ${result.key} succeeded`);
   *   }
   * }
   * ```
   */
  async batch(operations: BatchOp[]): Promise<BatchResponse> {
    const body = Buffer.from(JSON.stringify(operations), 'utf-8');

    const result = await this.request('/batch', {
      method: 'POST',
      body,
      headers: {
        'content-type': 'application/json',
        'accept': 'application/json',
      },
    });

    return JSON.parse(result.body.toString('utf-8')) as BatchResponse;
  }

  /**
   * Get Prometheus metrics from the server
   *
   * @returns Promise with metrics text
   *
   * @example
   * ```ts
   * const metrics = await client.metrics();
   * console.log(metrics);
   * // kv_storage_keys_total 1523
   * // kv_storage_objects_total 847
   * // ...
   * ```
   */
  async metrics(): Promise<string> {
    const result = await this.request('/metrics', {
      method: 'GET',
    });

    return result.body.toString('utf-8');
  }

  /**
   * Check if the server is accessible
   *
   * @returns Promise with true if server is accessible
   *
   * @example
   * ```ts
   * const isHealthy = await client.healthCheck();
   * if (!isHealthy) {
   *   console.error('Server is not accessible');
   * }
   * ```
   */
  async healthCheck(): Promise<boolean> {
    try {
      const result = await this.request('/', {
        method: 'GET',
        throwOnNotFound: false,
      });
      return result.statusCode !== 503;
    } catch {
      return false;
    }
  }

  /**
   * Close the HTTP/2 session and cleanup resources
   *
   * @example
   * ```ts
   * await client.close();
   * ```
   */
  close(): void {
    if (this.session) {
      this.session.close();
      this.session = null;
    }
  }
}

/**
 * Default export for convenience
 */
export default KVStorage;
