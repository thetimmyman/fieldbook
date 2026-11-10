#!/usr/bin/env bash
set -euo pipefail

# Checks the working tree for deployment identifier values.

for name in FIELDBOOK_HOST CLOUDFLARE_ACCOUNT_ID; do
  value="${!name-}"
  if [ -z "$value" ]; then
    echo "${name}: missing" >&2
    exit 1
  fi

  status=0
  grep -r -F -i -q --exclude-dir=.git -e "$value" . || status=$?
  if [ "$status" -eq 0 ]; then
    echo "an identifier value appears in the tree: ${name}" >&2
    exit 1
  elif [ "$status" -eq 1 ]; then
    :
  else
    echo "the search failed for: ${name}" >&2
    exit 1
  fi
done

echo "identifiers: none in the tree"
exit 0
