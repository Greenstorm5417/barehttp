'use strict';

const http = require('http');
const zlib = require('zlib');

const PLAIN = Buffer.from('hello');
const GZIP_BODY = Buffer.from('hello-gzip');

function writeChunked(res, body) {
  res.writeHead(200, {
    'Content-Type': 'text/plain',
    'Transfer-Encoding': 'chunked',
  });
  const mid = Math.max(1, Math.floor(body.length / 2));
  res.write(body.subarray(0, mid));
  res.write(body.subarray(mid));
  res.end();
}

const server = http.createServer((req, res) => {
  const path = (req.url || '/').split('?', 1)[0];

  if (path === '/plain') {
    res.writeHead(200, { 'Content-Type': 'text/plain', 'Content-Length': PLAIN.length });
    res.end(PLAIN);
    return;
  }
  if (path === '/chunked') {
    writeChunked(res, PLAIN);
    return;
  }
  if (path === '/gzip') {
    const compressed = zlib.gzipSync(GZIP_BODY);
    res.writeHead(200, {
      'Content-Type': 'text/plain',
      'Content-Encoding': 'gzip',
      'Content-Length': compressed.length,
    });
    res.end(compressed);
    return;
  }
  if (path === '/headers') {
    res.writeHead(200, {
      'Content-Type': 'text/plain',
      'X-Interop-Server': 'node',
      'Content-Length': 2,
    });
    res.end('ok');
    return;
  }
  if (path === '/status/404') {
    res.writeHead(404, { 'Content-Type': 'text/plain', 'Content-Length': 7 });
    res.end('missing');
    return;
  }
  if (path === '/close') {
    res.writeHead(200, {
      'Content-Type': 'text/plain',
      Connection: 'close',
      'Content-Length': 3,
    });
    res.end('bye');
    return;
  }
  if (path === '/http10') {
    // Node always responds HTTP/1.1; body remains the probe token.
    res.writeHead(200, { 'Content-Type': 'text/plain', 'Content-Length': 10 });
    res.end('http10-ish');
    return;
  }
  res.writeHead(404, { 'Content-Type': 'text/plain', 'Content-Length': 7 });
  res.end('missing');
});

server.listen(8080, '0.0.0.0');
