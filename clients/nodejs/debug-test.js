import { KVStorage } from './dist/index.js';

const client = new KVStorage({
  endpoint: 'http://127.0.0.1:3000',
  token: 'test-token',
  timeout: 5000,
});

try {
  console.log('Testing PUT...');
  const result = await client.put('test-key', 'Hello, World!');
  console.log('PUT result:', result);

  console.log('Testing GET...');
  const value = await client.get('test-key');
  console.log('GET value:', value);

  console.log('Testing DELETE...');
  const deleted = await client.delete('test-key');
  console.log('DELETE result:', deleted);
} catch (error) {
  console.error('Error:', error);
} finally {
  client.close();
}
