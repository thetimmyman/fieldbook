#!/usr/bin/env bash
set -euo pipefail

# Checks that all specified paths on the hosted edition require Cloudflare Access login.
# Usage: check-login-gate.sh BASE_URL PATH...

if [ "$#" -lt 2 ]; then
  echo "usage: check-login-gate.sh BASE_URL PATH..." >&2
  exit 2
fi

BASE_URL="$1"
shift

TMPDIR_GATE=$(mktemp -d)
trap 'rm -rf "$TMPDIR_GATE"' EXIT

FAILED=0

for P; do
  case "$P" in
    /*) TARGET_URL="${BASE_URL%/}${P}" ;;
    *)  TARGET_URL="${BASE_URL%/}/${P}" ;;
  esac

  BODYFILE="${TMPDIR_GATE}/body"
  HEADERFILE="${TMPDIR_GATE}/headers"
  rm -f "$BODYFILE" "$HEADERFILE"

  set +e
  STATUS=$(curl --silent --show-error --max-redirs 0 --max-time 30 --output "$BODYFILE" --dump-header "$HEADERFILE" --write-out '%{http_code}' "$TARGET_URL" 2>/dev/null)
  CURL_EXIT=$?
  set -e

  if [ "$CURL_EXIT" -ne 0 ]; then
    echo "FAIL ${P}: request failed"
    FAILED=1
    continue
  fi

  if [ "$STATUS" != "302" ] && [ "$STATUS" != "401" ] && [ "$STATUS" != "403" ]; then
    echo "FAIL ${P}: status ${STATUS}"
    FAILED=1
    continue
  fi

  LOCATION=$(awk 'BEGIN{IGNORECASE=1} /^location:[[:space:]]*/ {sub(/^location:[[:space:]]*/, ""); sub(/\r$/, ""); print; exit}' "$HEADERFILE")

  if [ "$STATUS" = "302" ] && [ -z "$LOCATION" ]; then
    echo "FAIL ${P}: redirect without a Location"
    FAILED=1
    continue
  fi

  if [ -n "$LOCATION" ]; then
    if [[ ! "$LOCATION" =~ ^https://([A-Za-z0-9-]+\.)*cloudflareaccess\.com(/|$) ]]; then
      echo "FAIL ${P}: Location is not on the login host"
      FAILED=1
      continue
    fi
  fi

  if grep -a -i -F -q "fieldbook" "$BODYFILE" || grep -a -i -F -q "Content-Security-Policy" "$BODYFILE"; then
    echo "FAIL ${P}: body carries edition content"
    FAILED=1
    continue
  fi

  if [ "$STATUS" = "302" ]; then
    echo "ok ${P}: 302 to the login"
  else
    echo "ok ${P}: ${STATUS} status"
  fi
done

if [ "$FAILED" -ne 0 ]; then
  exit 1
fi
exit 0
