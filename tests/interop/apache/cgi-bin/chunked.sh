#!/bin/sh
# CGI cannot emit Transfer-Encoding: chunked (Apache frames CGI). Body is still "hello".
printf 'Content-Type: text/plain\r\n'
printf 'Content-Length: 5\r\n'
printf '\r\n'
printf 'hello'
