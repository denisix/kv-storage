#!/usr/bin/env node

/**
 * Standalone test runner for kv-storage-client
 * Works with both node and bun
 */

import { KVStorage } from './index.js';

const TEST_ENDPOINT = process.env.TEST_ENDPOINT || 'http://127.0.0.1:3456';
const TEST_TOKEN = process.env.TEST_TOKEN || 'test-token';
const USE_HTTPS = TEST_ENDPOINT.startsWith('https://');

let passed = 0;
let failed = 0;

function log(msg: string) {
  console.log(msg);
}

function logError(msg: string) {
  console.error(`❌ ${msg}`);
  failed++;
}

function logPass(msg: string) {
  console.log(`✅ ${msg}`);
  passed++;
}

async function runTest(name: string, fn: () => Promise<void>) {
  try {
    await fn();
    logPass(name);
  } catch (error: any) {
    logError(`${name}: ${error.message}`);
  }
}

async function cleanup(client: KVStorage, keys: string[]) {
  for (const key of keys) {
    try {
      await client.delete(key);
    } catch {
      // Ignore cleanup errors
    }
  }
}

async function main() {
  log(`🧪 Testing against: ${TEST_ENDPOINT}`);
  log(`🔑 Using token: ${TEST_TOKEN}\n`);

  // Cleanup function to ensure keys don't exist before tests
  async function ensureClean(client: KVStorage, keys: string[]) {
    for (const key of keys) {
      try {
        await client.delete(key);
      } catch {
        // Ignore if key doesn't exist
      }
    }
  }

  // Test PUT and GET
  await runTest('PUT and GET text data', async () => {
    const client = new KVStorage({
      endpoint: TEST_ENDPOINT,
      token: TEST_TOKEN,
      timeout: 5000,
    });

    await ensureClean(client, ['test:put-get']);

    const result = await client.put('test:put-get', 'Hello, World!');
    if (typeof result.hash !== 'string') {
      throw new Error('Invalid hash result');
    }

    const value = await client.get('test:put-get');
    if (value !== 'Hello, World!') {
      throw new Error(`Expected "Hello, World!" but got "${value}"`);
    }

    await cleanup(client, ['test:put-get']);
    client.close();
  });

  // Test binary data
  await runTest('PUT and GET binary data', async () => {
    const client = new KVStorage({
      endpoint: TEST_ENDPOINT,
      token: TEST_TOKEN,
      timeout: 5000,
    });

    await ensureClean(client, ['test:binary']);

    const binary = new Uint8Array([0, 1, 2, 3, 4, 255]);
    await client.put('test:binary', binary);
    const retrieved = await client.get('test:binary', 'binary');
    if (!Buffer.isBuffer(retrieved)) {
      throw new Error('Expected Buffer but got ' + typeof retrieved);
    }
    if (retrieved.length !== 6) {
      throw new Error(`Expected 6 bytes but got ${retrieved.length}`);
    }

    await cleanup(client, ['test:binary']);
    client.close();
  });

  // Test DELETE
  await runTest('DELETE operation', async () => {
    const client = new KVStorage({
      endpoint: TEST_ENDPOINT,
      token: TEST_TOKEN,
      timeout: 5000,
    });

    await ensureClean(client, ['test:delete']);

    await client.put('test:delete', 'to be deleted');
    const deleted = await client.delete('test:delete');
    if (!deleted) {
      throw new Error('Delete should return true');
    }
    const notFound = await client.get('test:delete');
    if (notFound !== null) {
      throw new Error('Value should be null after delete');
    }

    client.close();
  });

  // Test HEAD
  await runTest('HEAD operation', async () => {
    const client = new KVStorage({
      endpoint: TEST_ENDPOINT,
      token: TEST_TOKEN,
      timeout: 5000,
    });

    await ensureClean(client, ['test:head']);

    await client.put('test:head', 'head test data');
    const info = await client.head('test:head');
    if (!info) {
      throw new Error('HEAD should return info');
    }
    if (info['content-length'] !== '14') {
      throw new Error(`Expected content-length 14 but got ${info['content-length']}`);
    }

    await cleanup(client, ['test:head']);
    client.close();
  });

  // Test LIST
  await runTest('LIST operation', async () => {
    const client = new KVStorage({
      endpoint: TEST_ENDPOINT,
      token: TEST_TOKEN,
      timeout: 5000,
    });

    await ensureClean(client, ['test:list:1', 'test:list:2']);

    await client.put('test:list:1', 'data1');
    await client.put('test:list:2', 'data2');
    const result = await client.list({ limit: 10 });
    if (!result.keys || !Array.isArray(result.keys)) {
      throw new Error('LIST should return keys array');
    }

    await cleanup(client, ['test:list:1', 'test:list:2']);
    client.close();
  });

  // Test UPDATE (PUT existing key)
  await runTest('PUT updates existing key', async () => {
    const client = new KVStorage({
      endpoint: TEST_ENDPOINT,
      token: TEST_TOKEN,
      timeout: 5000,
    });

    await ensureClean(client, ['test:update']);

    const result1 = await client.put('test:update', 'first value');
    if (typeof result1.hash !== 'string') {
      throw new Error('Invalid hash result on first put');
    }

    const value1 = await client.get('test:update');
    if (value1 !== 'first value') {
      throw new Error(`Expected "first value" but got "${value1}"`);
    }

    const result2 = await client.put('test:update', 'second value');
    if (typeof result2.hash !== 'string') {
      throw new Error('Invalid hash result on second put');
    }

    const value2 = await client.get('test:update');
    if (value2 !== 'second value') {
      throw new Error(`Expected "second value" but got "${value2}"`);
    }

    await cleanup(client, ['test:update']);
    client.close();
  });

  // Test BATCH
  await runTest('BATCH operations', async () => {
    const client = new KVStorage({
      endpoint: TEST_ENDPOINT,
      token: TEST_TOKEN,
      timeout: 5000,
    });

    // Clean up first to avoid "Key already exists" errors
    await ensureClean(client, ['test:batch:1', 'test:batch:2']);

    const response = await client.batch([
      { op: 'put', key: 'test:batch:1', value: 'batch1' },
      { op: 'put', key: 'test:batch:2', value: 'batch2' },
      { op: 'get', key: 'test:batch:1' },
    ]);
    if (!response.results || response.results.length !== 3) {
      throw new Error('BATCH should return 3 results');
    }
    const getResult = response.results.find(r => 'get' in r && (r as any).get.key === 'test:batch:1');
    if (!getResult || !(getResult as any).get.found || (getResult as any).get.value !== 'batch1') {
      throw new Error('BATCH GET failed');
    }

    await cleanup(client, ['test:batch:1', 'test:batch:2']);
    client.close();
  });

  // Test keys with spaces
  await runTest('PUT and GET keys with spaces', async () => {
    const client = new KVStorage({
      endpoint: TEST_ENDPOINT,
      token: TEST_TOKEN,
      timeout: 5000,
    });

    const keys = [
      'test key with spaces',
      'test hello world',
      'test path/to/my file.txt',
    ];

    for (const key of keys) {
      await ensureClean(client, [key]);

      const data = `data for ${key}`;
      const result = await client.put(key, data);
      if (typeof result.hash !== 'string' || result.hash.length === 0) {
        throw new Error(`PUT failed for key "${key}": invalid hash`);
      }

      const value = await client.get(key);
      if (value !== data) {
        throw new Error(`GET mismatch for key "${key}": expected "${data}" but got "${value}"`);
      }

      const deleted = await client.delete(key);
      if (!deleted) {
        throw new Error(`DELETE failed for key "${key}"`);
      }
    }

    client.close();
  });

  // Test keys with special characters
  await runTest('PUT and GET keys with special characters', async () => {
    const client = new KVStorage({
      endpoint: TEST_ENDPOINT,
      token: TEST_TOKEN,
      timeout: 5000,
    });

    const keys = [
      'test:colons:here',
      'test.dots.here',
      'test-dashes-here',
      'test_underscores_here',
      'test/slashes/here',
      'test!exclaim',
      'test~tilde',
      'test(parens)',
    ];

    for (const key of keys) {
      await ensureClean(client, [key]);

      const data = `data for ${key}`;
      const result = await client.put(key, data);
      if (typeof result.hash !== 'string') {
        throw new Error(`PUT failed for key "${key}"`);
      }

      const value = await client.get(key);
      if (value !== data) {
        throw new Error(`GET mismatch for key "${key}": expected "${data}" but got "${value}"`);
      }

      await client.delete(key);
    }

    client.close();
  });

  // Test keys with unicode
  await runTest('PUT and GET keys with unicode', async () => {
    const client = new KVStorage({
      endpoint: TEST_ENDPOINT,
      token: TEST_TOKEN,
      timeout: 5000,
    });

    const keys = [
      'test_ключ',
      'test_键',
      'test_مفتاح',
      'test_日本語キー',
    ];

    for (const key of keys) {
      await ensureClean(client, [key]);

      const data = `data for ${key}`;
      const result = await client.put(key, data);
      if (typeof result.hash !== 'string') {
        throw new Error(`PUT failed for key "${key}"`);
      }

      const value = await client.get(key);
      if (value !== data) {
        throw new Error(`GET mismatch for key "${key}": expected "${data}" but got "${value}"`);
      }

      await client.delete(key);
    }

    client.close();
  });

  // Test keys with URI-structural characters (#, ?, %)
  await runTest('PUT and GET keys with # ? % characters', async () => {
    const client = new KVStorage({
      endpoint: TEST_ENDPOINT,
      token: TEST_TOKEN,
      timeout: 5000,
    });

    const keys = [
      'test#hash',
      'test?question',
      'test%percent',
    ];

    for (const key of keys) {
      await ensureClean(client, [key]);

      const data = `data for ${key}`;
      const result = await client.put(key, data);
      if (typeof result.hash !== 'string') {
        throw new Error(`PUT failed for key "${key}"`);
      }

      const value = await client.get(key);
      if (value !== data) {
        throw new Error(`GET mismatch for key "${key}": expected "${data}" but got "${value}"`);
      }

      await client.delete(key);
    }

    client.close();
  });


  // Test sslFingerprint option is accepted with https:// endpoint
  await runTest('sslFingerprint with https:// does not throw at construction', async () => {
    // This should not throw when constructing
    const client = new KVStorage({
      endpoint: 'https://localhost:3000',
      token: TEST_TOKEN,
      sslFingerprint: '0E:2D:B3:49:A3:5B:4A:92:2A:29:2E:7B:82:DF:30:E6:FE:70:2B:1D:2E:3F:7D:28:4B:89:C4:AB:AB:F9:1A:7B',
    });
    // Client is constructed, we don't attempt connection since no TLS server is running
    client.close();
  });

  // Test sslFingerprint pinning with actual connection (requires HTTPS endpoint + TEST_SSL_FINGERPRINT)
  if (USE_HTTPS && process.env.TEST_SSL_FINGERPRINT) {
    await runTest('sslFingerprint pinning succeeds with correct fingerprint', async () => {
      const client = new KVStorage({
        endpoint: TEST_ENDPOINT,
        token: TEST_TOKEN,
        timeout: 5000,
        sslFingerprint: process.env.TEST_SSL_FINGERPRINT,
      });

      const result = await client.put('test:fingerprint', 'pinned');
      if (typeof result.hash !== 'string' || result.hash.length === 0) {
        throw new Error('PUT with fingerprint pinning should succeed');
      }

      const value = await client.get('test:fingerprint');
      if (value !== 'pinned') {
        throw new Error(`Expected "pinned" but got "${value}"`);
      }

      await client.delete('test:fingerprint');
      client.close();
    });

    await runTest('sslFingerprint pinning rejects wrong fingerprint', async () => {
      const client = new KVStorage({
        endpoint: TEST_ENDPOINT,
        token: TEST_TOKEN,
        timeout: 5000,
        sslFingerprint: 'FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF',
      });

      let rejected = false;
      try {
        await client.put('test:bad-fp', 'should-fail');
      } catch (error: any) {
        if (error.message.includes('fingerprint mismatch') || error.message.includes('self-signed') || error.message.includes('canceled')) {
          rejected = true;
        } else {
          // Any connection error is acceptable — wrong fingerprint should cause rejection
          rejected = true;
        }
      }
      client.close();

      if (!rejected) {
        throw new Error('Wrong fingerprint should cause connection rejection');
      }
    });
  }

  // Test rejectUnauthorized is respected without fingerprint
  await runTest('rejectUnauthorized option is accepted', async () => {
    const client = new KVStorage({
      endpoint: 'https://localhost:3000',
      token: TEST_TOKEN,
      rejectUnauthorized: false,
    });
    client.close();
  });

  // Test METRICS
  await runTest('METRICS endpoint', async () => {
    const client = new KVStorage({
      endpoint: TEST_ENDPOINT,
      token: TEST_TOKEN,
      timeout: 5000,
    });

    const metrics = await client.metrics();
    if (typeof metrics !== 'string' || metrics.length === 0) {
      throw new Error('METRICS should return string');
    }
    if (!metrics.includes('kv_storage_')) {
      throw new Error('METRICS should contain kv_storage_');
    }

    client.close();
  });

  // Summary
  log('\n' + '='.repeat(50));
  log(`Tests passed: ${passed}`);
  log(`Tests failed: ${failed}`);
  log('='.repeat(50));

  if (failed > 0) {
    process.exit(1);
  }
}

main().catch((error) => {
  console.error('Fatal error:', error);
  process.exit(1);
});
