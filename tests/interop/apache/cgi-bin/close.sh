#!/bin/sh
printf 'Content-Type: text/plain\r\n'
printf 'Connection: close\r\n'
printf 'Content-Length: 3\r\n'
printf '\r\n'
printf 'bye'
