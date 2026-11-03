#!/usr/bin/env sh
# Format check, lint (host + macOS targets if installed), tests, release build.
set -eu
cd "$(dirname "$0")/.."
cargo fmt --check
cargo clippy --all-targets -- -D warnings
for t in aarch64-apple-darwin x86_64-apple-darwin; do
    if rustup target list --installed 2>/dev/null | grep -qx "$t"; then
        echo "== clippy for $t (type-check only; linking needs a macOS SDK)"
        cargo clippy --target "$t" -- -D warnings
    fi
done
cargo test
cargo build --release
