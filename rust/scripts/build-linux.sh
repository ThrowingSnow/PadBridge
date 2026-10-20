#!/bin/bash
# Builds a portable Linux binary inside Ubuntu 20.04 (glibc 2.31), so it runs
# on Ubuntu 20.04 and newer, Debian 11+, Mint, Zorin, Pop!_OS and current
# Arch/Fedora alike. A binary built on a rolling distribution needs that
# distribution's newest glibc and does not start elsewhere. Needs Docker.
#
# Output: target/linux-portable/release/padbridge
set -euo pipefail
cd "$(dirname "$0")/.."

IMAGE=padbridge-build:focal
docker build -q -t "$IMAGE" - <<'DOCKERFILE' >/dev/null
FROM ubuntu:20.04
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
 && apt-get install -y --no-install-recommends build-essential pkg-config libasound2-dev curl ca-certificates \
 && rm -rf /var/lib/apt/lists/*
RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable
ENV PATH=/root/.cargo/bin:$PATH
DOCKERFILE

# The crate cache lives in a volume, so later builds do not download again.
docker run --rm \
    -v "$PWD":/src -w /src \
    -v padbridge-cargo-registry:/root/.cargo/registry \
    -e CARGO_TARGET_DIR=/src/target/linux-portable \
    "$IMAGE" \
    bash -c "cargo build --release -p padbridge && chown -R $(id -u):$(id -g) target/linux-portable"

echo "target/linux-portable/release/padbridge"
