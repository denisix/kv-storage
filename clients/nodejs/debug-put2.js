import http2 from 'node:http2';

const TEST_ENDPOINT = 'http://127.0.0.1:3000';
const TEST_TOKEN = 'test-token';

const client = http2.connect(TEST_ENDPOINT);

const req = client.request({
  ':method': 'PUT',
  ':path': '/test-key',
  'authorization': `Bearer ${TEST_TOKEN}`,
  'content-type': 'application/octet-stream',
});

let responseStatusCode = 0;
let responseHeaders = {};
const chunks = [];

// Set up event handlers BEFORE writing/ending
req.on('response', (headers, flags) => {
  console.log('Response headers:', headers);
  responseStatusCode = parseInt(headers[':status'], 10);
  responseHeaders = headers;
});

req.on('data', (chunk) => {
  console.log('Data chunk received, length:', chunk.length);
  chunks.push(chunk);
});

req.on('end', () => {
  console.log('Request ended');
  console.log('Status code:', responseStatusCode);
  const body = Buffer.concat(chunks);
  console.log('Body length:', body.length);
  console.log('Body:', body.toString('utf-8'));
  client.close();
});

req.on('error', (err) => {
  console.error('Request error:', err);
  client.close();
});

// Now send the data
req.write('Hello, World!');
req.end();
