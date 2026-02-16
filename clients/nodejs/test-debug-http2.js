import http2 from 'node:http2';
import { URL } from 'node:url';

const TEST_ENDPOINT = 'https://127.0.0.1:8443';
const TEST_TOKEN = 'test-token';

console.log('Test 1: Direct HTTP2');

const url = new URL(TEST_ENDPOINT);
const client = http2.connect(TEST_ENDPOINT, { rejectUnauthorized: false });

client.on('error', (err) => console.error('Session error:', err));
client.on('connect', () => console.log('Connected!'));

const req = client.request({
  ':method': 'PUT',
  ':path': '/test:1',
  'authorization': `Bearer ${TEST_TOKEN}`,
});

let responseHeaders = null;
let statusCode = null;
const chunks = [];

req.on('response', (headers) => {
  console.log('Response event!');
  responseHeaders = headers;
  statusCode = parseInt(headers[':status'] || '200', 10);
});

req.on('data', (chunk) => {
  console.log('Data event, length:', chunk.length);
  chunks.push(chunk);
});

req.on('end', () => {
  console.log('End event');
  console.log('Status:', statusCode);
  console.log('Headers:', responseHeaders);
  console.log('Body length:', Buffer.concat(chunks).length);
  client.close();
});

req.on('error', (err) => {
  console.error('Request error:', err);
  client.close();
});

req.write('Hello, World!');
req.end();  // This should work
