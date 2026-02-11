import { KVStorage } from './dist/index.js';

const client = new KVStorage({
  endpoint: 'http://localhost:3000',
  token: 'test-token'
});

// Store a value
await client.put('user:123', JSON.stringify({ name: 'John', age: 30 }));

// Retrieve a value
const value = await client.get('user:123');
if (value) {
  const user = JSON.parse(value);
  console.log(user.name); // "John"
}

// Delete a value
await client.delete('user:123');
console.log('- done')
