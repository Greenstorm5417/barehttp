#!/bin/sh
printf 'Status: 404 Not Found\r\n'
printf 'Content-Type: text/plain\r\n'
printf 'Content-Length: 7\r\n'
printf '\r\n'
printf 'missing'
