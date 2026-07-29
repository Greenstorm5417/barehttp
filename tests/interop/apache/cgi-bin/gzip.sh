#!/bin/sh
# Serve a precompressed body from the static volume.
# Do not stash gzip bytes in a shell variable: ash/bash strip NULs and corrupt the stream.
GZ="/usr/local/apache2/htdocs/hello-gzip.gz"
LEN="$(wc -c < "$GZ" | tr -d ' ')"
printf 'Content-Type: text/plain\r\n'
printf 'Content-Encoding: gzip\r\n'
printf 'Content-Length: %s\r\n' "$LEN"
printf '\r\n'
cat "$GZ"
