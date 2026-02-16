import http2 from 'node:http2';

const client = http2.connect('http://127.0.0.1:3000');

const req = client.request({
  ':method': 'PUT',
  ':path': '/test-key',
  'authorization': 'Bearer test-token',
});

req.setEncoding('utf8');
let data = '';

req.on('data', (chunk) => {
  data += chunk;
  console.log('Received chunk:', chunk.length);
});

req.on('end', () => {
  console.log('Final data:', data);
  console.log('Data length:', data.length);
  client.close();
});

req.on('error', (err) => {
  console.error('Error:', err);
  client.close();
});

req.write('Hello, World!');
req.end();
