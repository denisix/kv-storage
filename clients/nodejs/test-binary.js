import { KVStorage } from './dist/index.js';

const client = new KVStorage({
  endpoint: 'http://127.0.0.1:3001',
  token: 'test-token',
  timeout: 5000,
});

(async () => {
  try {
    // First, clean up
    try {
      await client.delete('test:binary');
    } catch {}

    console.log('Testing binary data...');

    const binary = new Uint8Array([0, 1, 2, 3, 4, 255]);
    console.log('Sending binary data:', binary);

    await client.put('test:binary', binary);
    console.log('PUT successful');

    const retrieved = await client.get('test:binary', 'binary');
    console.log('GET result:', retrieved);
    console.log('Buffer?', Buffer.isBuffer(retrieved));
    console.log('Length?', retrieved?.length);

    if (!Buffer.isBuffer(retrieved)) {
      throw new Error('Expected Buffer but got ' + typeof retrieved);
    }
    if (retrieved.length !== 6) {
      throw new Error(`Expected 6 bytes but got ${retrieved.length}`);
    }

    await client.delete('test:binary');
    console.log('Test passed!');
  } catch (error) {
    console.error('Error:', error);
  } finally {
    client.close();
  }
})();
