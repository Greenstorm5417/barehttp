#!/usr/bin/env python3
"""Minimal stdlib HTTP server for barehttp interop."""

from __future__ import annotations

import gzip
import http.server
import socketserver


PLAIN = b"hello"
GZIP_BODY = b"hello-gzip"


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt: str, *args) -> None:  # noqa: A003
        pass

    def _send(
        self,
        code: int,
        body: bytes,
        *,
        headers: list[tuple[str, str]] | None = None,
        chunked: bool = False,
        http10: bool = False,
        close: bool = False,
    ) -> None:
        if http10:
            self.protocol_version = "HTTP/1.0"
        self.send_response(code)
        self.send_header("Content-Type", "text/plain")
        for k, v in headers or []:
            self.send_header(k, v)
        if close or http10:
            self.send_header("Connection", "close")
        if chunked and not http10:
            self.send_header("Transfer-Encoding", "chunked")
            self.end_headers()
            # Two chunks + terminator.
            mid = max(1, len(body) // 2)
            for part in (body[:mid], body[mid:]):
                self.wfile.write(f"{len(part):x}\r\n".encode("ascii"))
                self.wfile.write(part)
                self.wfile.write(b"\r\n")
            self.wfile.write(b"0\r\n\r\n")
        else:
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        if http10:
            self.protocol_version = "HTTP/1.1"

    def do_GET(self) -> None:  # noqa: N802
        path = self.path.split("?", 1)[0]
        if path == "/plain":
            self._send(200, PLAIN)
        elif path == "/chunked":
            self._send(200, PLAIN, chunked=True)
        elif path == "/gzip":
            compressed = gzip.compress(GZIP_BODY)
            self._send(200, compressed, headers=[("Content-Encoding", "gzip")])
        elif path == "/headers":
            self._send(200, b"ok", headers=[("X-Interop-Server", "python")])
        elif path == "/status/404":
            self._send(404, b"missing")
        elif path == "/close":
            self._send(200, b"bye", close=True)
        elif path == "/http10":
            self._send(200, b"http10-ish", http10=True)
        else:
            self._send(404, b"missing")


class ReusableTCPServer(socketserver.TCPServer):
    allow_reuse_address = True


if __name__ == "__main__":
    with ReusableTCPServer(("0.0.0.0", 8080), Handler) as httpd:
        httpd.serve_forever()
