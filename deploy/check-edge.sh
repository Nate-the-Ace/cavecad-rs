#!/usr/bin/env bash
# Checks the edge after deployment: signed-out visitors are sent to /signin, forged identity headers mean
# nothing, icons are public, downloads need a session, the desktop API host refuses requests without a token.
set -u
bad=0
code=$(curl -s -o /dev/null -w '%{http_code} %{redirect_url}' https://cavecad.com/)
case "$code" in 302\ https://cavecad.com/signin*) ;; *) echo "FAIL signed-out app: got $code"; bad=1 ;; esac
code=$(curl -s -o /dev/null -w '%{http_code}' -H 'Remote-Email: nathan@example.com' -H 'X-Cave-Gate: x' https://cavecad.com/api/me)
[ "$code" = 401 ] || { echo "FAIL forged header: got $code"; bad=1; }
code=$(curl -s -o /dev/null -w '%{http_code}' https://cavecad.com/apple-touch-icon.png)
[ "$code" = 200 ] || { echo "FAIL icon: got $code"; bad=1; }
code=$(curl -s -o /dev/null -w '%{http_code}' https://cavecad.com/downloads/x)
[ "$code" = 302 ] || { echo "FAIL signed-out download: got $code"; bad=1; }
code=$(curl -s -o /dev/null -w '%{http_code}' https://api.cavecad.com/api/me)
[ "$code" = 401 ] || { echo "FAIL api. without a token: got $code"; bad=1; }
csp=$(curl -sI https://cavecad.com/signin | grep -i '^content-security-policy:')
case "$csp" in *script-src*) case "$csp" in *unsafe-inline*) ;; *) echo "FAIL /signin CSP blocks inline scripts: $csp"; bad=1 ;; esac ;; esac
[ $bad = 0 ] && echo 'edge: all ok'
exit $bad
