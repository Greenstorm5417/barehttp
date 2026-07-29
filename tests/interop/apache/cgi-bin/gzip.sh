#!/bin/sh
BODY='hello-gzip'
GZ="$(printf '%s' "$BODY" | gzip -c)"
LEN="$(printf '%s' "$GZ" | wc -c | tr -d ' ')"
printf 'Content-Type: text/plain\r\n'
printf 'Content-Encoding: gzip\r\n'
printf 'Content-Length: %s\r\n' "$LEN"
printf '\r\n'
printf '%s' "$GZ"
