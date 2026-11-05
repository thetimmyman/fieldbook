# Fieldbook developer recipes. Run `just` to list them.
# Bootstrap: `cargo install --locked just@1.58.0`, then `just setup`.
# Recipes still to come: site, browser, edition, release and new-card. Each arrives with its card.

set shell := ["bash", "-euo", "pipefail", "-c"]

# List the recipes.
default:
    @just --list --unsorted

# Install the pinned tools (.config/tools.toml) and enable the git hooks. Safe to repeat.
setup:
    cargo xtask setup

# Check the installed tools against their pins. CI checks exact versions.
doctor:
    cargo xtask doctor

# Everything a pull request must pass, in the order of AD-07. CI runs this too.
check:
    # 1. Formatting
    cargo fmt --all --check
    # 2. Lints, including the crate-layering and I/O bans in clippy.toml, for every build shape
    cargo clippy --workspace --all-targets --locked -- -D warnings
    cargo clippy --workspace --all-targets --locked --all-features -- -D warnings
    cargo clippy -p fieldbook --all-targets --locked --no-default-features -- -D warnings
    # 3. Tests (the metadata test and the negative controls included) for every build shape, then doc tests
    # The offline Windows graph check needs its locked crates even on a Linux runner.
    cargo fetch --locked
    cargo nextest run --workspace --locked
    cargo nextest run -p fieldbook --locked --all-features
    cargo nextest run -p fieldbook --locked --no-default-features
    cargo test --workspace --doc --locked
    # 4. Dependency policy: bans, licences, sources, with default features and with all of them
    cargo deny --locked check bans licenses sources
    cargo deny --locked --all-features check bans licenses sources
    # 5. Spelling
    typos
    # 6. Secrets in the commits not yet on main
    just _gitleaks
    # 7. Check the Corpus at the commit date under --strict
    cargo run -q --locked -p fieldbook -- check --strict --today "$(git log -1 --format=%cs)"
    # 8. Regenerate the generated docs and diff
    cargo xtask docs --check
    # 9. Published compatibility
    cargo xtask compat --base release-baseline.toml

# Resolve Cargo.lock with the 14-day cooldown (AD-06). The pinned nightly only resolves; it never builds.
lock:
    #!/usr/bin/env bash
    set -euo pipefail
    nightly="$(sed -n 's/^nightly *= *"\(.*\)".*/\1/p' .config/tools.toml)"
    rustup toolchain install "$nightly" --profile minimal
    cargo "+$nightly" generate-lockfile -Zunstable-options \
      --publish-time "$(date -u -d '14 days ago' +%Y-%m-%dT%H:%M:%SZ)"

# gitleaks over the commits that are not on main yet. CI sets FIELDBOOK_BASE to the commit
# to start after (all zeros means scan everything); locally the default is origin/main, then main.
_gitleaks:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! git rev-parse --verify --quiet HEAD >/dev/null; then
        echo "gitleaks: no commits yet; the pre-commit hook scans what is staged"
        exit 0
    fi
    base="${FIELDBOOK_BASE:-}"
    if [ -z "$base" ]; then
        base="$(git rev-parse --verify --quiet origin/main || git rev-parse --verify --quiet main || true)"
        if [ -n "$base" ] && [ "$base" = "$(git rev-parse HEAD)" ]; then
            echo "gitleaks: no commits beyond main"
            exit 0
        fi
    elif [ "${base//0/}" = "" ]; then
        base=""
    fi
    gitleaks git --no-banner --redact --log-opts="${base:+$base..}HEAD" .
