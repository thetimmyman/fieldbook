#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "$0")/.."
mode=${1:-setup}
[[ $# -le 1 && $mode =~ ^(setup|--check|--verify)$ ]] || { echo 'usage: scripts/agent-bootstrap.sh [--check|--verify]' >&2; exit 2; }
command -v cargo >/dev/null || { echo 'missing cargo and the pinned Rust toolchain' >&2; exit 1; }
if [[ $mode != --check ]]; then
    cargo xtask setup
fi
cargo xtask doctor
if [[ $mode == --verify ]]; then just check; fi
