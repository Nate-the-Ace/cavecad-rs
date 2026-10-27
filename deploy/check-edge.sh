#!/usr/bin/env bash
# Checks the edge after deployment: a forged identity header never reaches CaveCAD, and the desktop API host
# refuses requests without a token. TinyAuth tells browsers from scripts by User-Agent, so send a browser one.
set -u
UA='Mozilla/5.0 (Macintosh; Intel Mac OS X 15_0) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15'
bad=0
code=$(curl -s -o /dev/null -w '%{http_code} %{redirect_url}' -A "$UA" -H 'Remote-Email: nathan@example.com' -H 'X-Cave-Gate: tinyauth' https://cave.schonegg.stream/api/me)
case "$code" in 302\ https://auth.schonegg.stream/*) ;; *) echo "FAIL forged header on cave.: got $code"; bad=1 ;; esac
code=$(curl -s -o /dev/null -w '%{http_code}' -H 'Remote-Email: nathan@example.com' -H 'X-Cave-Gate: tinyauth' https://cave-api.schonegg.stream/api/me)
[ "$code" = 401 ] || { echo "FAIL cave-api. without a token: got $code"; bad=1; }
code=$(curl -s -o /dev/null -w '%{http_code}' -A "$UA" https://cave.schonegg.stream/)
[ "$code" = 302 ] || { echo "FAIL the app is reachable without signing in: got $code"; bad=1; }
[ $bad = 0 ] && echo 'edge: all ok'
exit $bad
