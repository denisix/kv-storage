import { KVStorage } from './dist/index.js';

const TEST_ENDPOINT = 'https://127.0.0.1:8443';
const TEST_TOKEN = 'test-token';

console.log('Test 1: First request');
const client1 = new KVStorage({ endpoint: TEST_ENDPOINT, token: TEST_TOKEN, timeout: 5000, rejectUnauthorized: false });
try {
  const result = await client1.put('test:1', 'Hello, World!');
  console.log('  PUT result hash:', result.hash);
  const value = await client1.get('test:1');
  console.log('  GET value:', value);
} catch (error) {
  console.error('  Error:', error.message);
}
client1.close();

await new Promise(resolve => setTimeout(resolve, 2000));

console.log('Test 2: Second request');
const client2 = new KVStorage({ endpoint: TEST_ENDPOINT, token: TEST_TOKEN, timeout: 5000, rejectUnauthorized: false });
try {
  const result = await client2.put('test:2', 'Hello, World!');
  console.log('  PUT result hash:', result.hash);
  const value = await client2.get('test:2');
  console.log('  GET value:', value);
} catch (error) {
  console.error('  Error:', error.message);
}
client2.close();
