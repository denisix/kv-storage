import http2 from 'node:http2';

const TEST_ENDPOINT = 'http://127.0.0.1:3000';
const TEST_TOKEN = 'test-token';

class HTTP2Session {
  constructor(authority) {
    this.client = http2.connect(authority);
    this.client.on('error', (_err) => {
      console.log('Session error:', _err);
    });
  }

  request(method, path, headers, body, timeout) {
    return new Promise((resolve, reject) => {
      const reqHeaders = {
        ':method': method,
        ':path': path,
        ...headers,
      };

      const req = this.client.request(reqHeaders);

      let responseHeaders = {};
      let statusCode = 200;
      const chunks = [];

      console.log('Setting up response handler...');

      req.on('response', (headers, _flags) => {
        console.log('Response event fired!');
        responseHeaders = headers;
        statusCode = parseInt(headers[':status'] || '200', 10);
        console.log('Status:', statusCode);
      });

      req.on('data', (chunk) => {
        console.log('Data event, chunk length:', chunk.length);
        chunks.push(chunk);
      });

      req.on('end', () => {
        console.log('End event fired');
        clearTimeout(timeoutId);
        resolve({
          statusCode,
          headers: responseHeaders,
          body: Buffer.concat(chunks),
        });
      });

      const timeoutId = setTimeout(() => {
        console.log('Timeout fired');
        req.close();
        reject(new Error(`Request timeout after ${timeout}ms`));
      }, timeout);

      req.on('error', (err) => {
        console.log('Request error:', err);
        clearTimeout(timeoutId);
        reject(err);
      });

      console.log('Sending request body...');
      if (body) {
        req.end(body);
      } else {
        req.end();
      }
    });
  }

  close() {
    this.client.destroy();
  }
}

const session = new HTTP2Session(TEST_ENDPOINT);

(async () => {
  try {
    const result = await session.request(
      'PUT',
      '/test-key',
      { authorization: `Bearer ${TEST_TOKEN}` },
      Buffer.from('Hello, World!', 'utf-8'),
      5000
    );
    console.log('Result:', result);
    console.log('Body:', result.body.toString());
  } catch (err) {
    console.error('Error:', err);
  } finally {
    session.close();
  }
})();
