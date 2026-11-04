#!/usr/bin/env bash
set -euo pipefail

base="${1:-}"
if [ -z "$base" ] || [ -z "${base//0/}" ]; then
    printf 'code=true\ncorpus=true\n'
    exit 0
fi

if ! paths="$(git diff --no-renames --name-only "$base" HEAD 2>/dev/null)"; then
    printf 'code=true\ncorpus=true\n'
    exit 0
fi

code=false
corpus=false

while IFS= read -r path; do
    [ -z "$path" ] && continue
    case "$path" in
        corpus/*)
            corpus=true
            ;;
        tests/*|crates/*|xtask/*)
            code=true
            ;;
        docs/*|*.md)
            ;;
        *)
            code=true
            ;;
    esac
done <<< "$paths"

printf 'code=%s\ncorpus=%s\n' "$code" "$corpus"
