#!/usr/bin/env bash
set -euo pipefail

# Checks that the staged hosted build is an unreleased build.

if [ -n "${FIELDBOOK_GIT_SHA+x}" ]; then
  echo "FIELDBOOK_GIT_SHA is set; the hosted build must be an unreleased build" >&2
  exit 1
fi

if [ ! -f "out/hosted/index.html" ]; then
  echo "out/hosted/index.html is missing" >&2
  exit 1
fi

if ! grep -q -i -F 'unreleased build' out/hosted/index.html; then
  echo "out/hosted/index.html does not say unreleased build" >&2
  exit 1
fi

echo "unreleased build: ok"
exit 0
