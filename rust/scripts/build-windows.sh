#!/bin/bash
# Cross-builds padbridge.exe from a Mac or Linux box and prints where it is.
# Uses a local mingw-w64 when there is one (plus `rustup target add
# x86_64-pc-windows-gnu`), otherwise Docker, so a Linux build machine needs no
# packages installed.
set -euo pipefail
cd "$(dirname "$0")/.."

if command -v x86_64-w64-mingw32-gcc >/dev/null; then
    # Keep the home directory (user name) out of paths compiled into the binary.
    export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$HOME=~"
    cargo build --release --target x86_64-pc-windows-gnu -p padbridge
    OUT=target/x86_64-pc-windows-gnu/release/padbridge.exe
else
    IMAGE=padbridge-build:windows
    docker build -q -t "$IMAGE" - <<'DOCKERFILE' >/dev/null
FROM ubuntu:22.04
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
 && apt-get install -y --no-install-recommends build-essential gcc-mingw-w64-x86-64 binutils-mingw-w64-x86-64 curl ca-certificates \
 && rm -rf /var/lib/apt/lists/*
RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable \
    --target x86_64-pc-windows-gnu
ENV PATH=/root/.cargo/bin:$PATH
DOCKERFILE
    # Made here, so it belongs to this user and not to root in the container.
    mkdir -p target/windows-docker
    docker run --rm \
        -v "$PWD":/src -w /src \
        -v padbridge-cargo-registry:/root/.cargo/registry \
        -e CARGO_TARGET_DIR=/src/target/windows-docker \
        "$IMAGE" \
        bash -c "cargo build --release --target x86_64-pc-windows-gnu -p padbridge \
                 && chown -R $(id -u):$(id -g) target/windows-docker"
    OUT=target/windows-docker/x86_64-pc-windows-gnu/release/padbridge.exe
fi
echo "$OUT"
