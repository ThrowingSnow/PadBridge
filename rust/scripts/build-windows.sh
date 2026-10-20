#!/bin/bash
# Cross-builds rust/target/x86_64-pc-windows-gnu/release/padbridge.exe from a
# Mac or Linux box (needs `rustup target add x86_64-pc-windows-gnu` and mingw-w64).
set -euo pipefail
cd "$(dirname "$0")/.."

# Keep the home directory (user name) out of paths compiled into the binary.
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$HOME=~"
cargo build --release --target x86_64-pc-windows-gnu -p padbridge
echo "target/x86_64-pc-windows-gnu/release/padbridge.exe"
