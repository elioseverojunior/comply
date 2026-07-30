# syntax=docker/dockerfile:1

# SPDX-FileCopyrightText: COMPLY contributors
#
# SPDX-License-Identifier: MIT OR Apache-2.0

# Multi-stage Dockerfile for multi-architecture builds (linux/amd64, linux/arm64)

# Dependency stage
ARG IMAGE_TAG=1-slim@sha256:5c6f46a6e4472ab1ca7ba7d494e6677f2f219ebc02f32025d3986f057635ec9c
ARG APP_VERSION=0.1.0

FROM --platform=$BUILDPLATFORM docker.io/library/rust:${IMAGE_TAG} AS dependencies

SHELL ["/bin/bash", "-eou", "pipefail", "-c"]

ARG IMAGE_TAG
ARG APP_VERSION
ARG TARGETPLATFORM
ARG BUILDPLATFORM

ENV BUILDPLATFORM=$BUILDPLATFORM
ENV TARGETPLATFORM=$TARGETPLATFORM
ENV BUILD_PATH=/usr/src/app

# 1. Install system packages and cross-compilation toolchains
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked\
 --mount=type=cache,target=/var/lib/apt,sharing=locked <<EOF
  #!/bin/bash
  set -exou pipefail

  apt-get update
  apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    ca-certificates \
    gcc \
    g++ \
    cmake \
    curl \
    build-essential \
    clang \
    mold

  case "$TARGETPLATFORM" in
    "linux/arm64")
      dpkg --add-architecture arm64
      apt-get update
      apt-get install -y --no-install-recommends \
        gcc-aarch64-linux-gnu \
        g++-aarch64-linux-gnu \
        libc6-dev-arm64-cross \
        libssl-dev:arm64
      ;;
    "linux/amd64")
      echo "Native amd64 platform, no cross-compilation tools needed."
      ;;
    *)
      echo "Unsupported target platform: $TARGETPLATFORM"
      exit 1
      ;;
  esac
EOF

# 2. Add correct Rust targets based on platform
RUN <<EOF
  #!/bin/bash
  set -exou pipefail
  case "$TARGETPLATFORM" in
    "linux/arm64")
      rustup target add aarch64-unknown-linux-gnu
      ;;
    "linux/amd64")
      rustup target add x86_64-unknown-linux-gnu
      ;;
  esac
EOF

WORKDIR ${BUILD_PATH}

# 3. Copy manifests and global build script
COPY Cargo.toml Cargo.lock build.rs ./
COPY crates/comply/Cargo.toml crates/comply/
COPY crates/comply-cli/Cargo.toml crates/comply-cli/
COPY .cargo/config.toml ./.cargo/config.toml

# 4. Generate boilerplate and build dependencies caching both target architectures correctly
RUN --mount=type=cache,target=/usr/local/cargo/registry\
 --mount=type=cache,target=/usr/local/cargo/git\
 --mount=type=cache,target=${BUILD_PATH}/target,sharing=locked <<EOF
  #!/bin/bash
  set -exou pipefail

  # Create minimal rust source placeholders
  mkdir -p crates/comply/src crates/comply-cli/src src/native
  touch crates/comply/src/lib.rs
  echo 'fn main() { println!("===> Preparing Cargo Dependencies! <==="); }' > crates/comply-cli/src/main.rs

  # Mock the C native file so cc/build.rs doesn't crash during dependency compilation
  echo 'void comply_native_dummy() {}' > src/native/comply_native.c

  # Build targeting the exact architecture matching the multi-arch flow
  case "$TARGETPLATFORM" in
    "linux/arm64")
      export CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
      cargo build --workspace --all-targets --all-features --target aarch64-unknown-linux-gnu
      ;;
    "linux/amd64")
      # Mold needs clang flag inside cargo to match your .cargo/config.toml
      cargo build --workspace --all-targets --all-features --target x86_64-unknown-linux-gnu
      ;;
  esac
EOF

# Build stage
FROM dependencies AS builder

ARG TARGETPLATFORM
ARG BUILDPLATFORM
ARG APP_VERSION

ENV BUILDPLATFORM=$BUILDPLATFORM
ENV TARGETPLATFORM=$TARGETPLATFORM
ENV BUILD_PATH=/usr/src/app
ENV RUSTUP_PROFILE=minimal
ENV APP_VERSION=${APP_VERSION}

WORKDIR ${BUILD_PATH}

# Copy the actual source code over (invalidating the cache from this point forward)
COPY . .

RUN --mount=type=cache,target=/usr/local/cargo/registry\
 --mount=type=cache,target=/usr/local/cargo/git\
 --mount=type=cache,target=${BUILD_PATH}/target,sharing=locked <<EOF
  #!/bin/bash
  set -exou pipefail

  mkdir -p /app
  rm -f rust-toolchain.toml

  # Clean old binary artifacts to ensure fresh build variables
  rm -rf target/*/release/comply* target/*/release/libcomply* \
         target/*/incremental target/*/build/comply*

  case "$TARGETPLATFORM" in
    "linux/arm64")
      export CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
      export CXX_aarch64_unknown_linux_gnu=aarch64-linux-gnu-g++
      export AR_aarch64_unknown_linux_gnu=aarch64-linux-gnu-ar
      export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
      export PKG_CONFIG_PATH=/usr/lib/aarch64-linux-gnu/pkgconfig
      export PKG_CONFIG_ALLOW_CROSS=1
      export CARGO_BUILD_VERSION=${APP_VERSION}
      export TARGET=aarch64-unknown-linux-gnu
      export CARGO_BUILD_TARGET=aarch64-unknown-linux-gnu

      cargo build --release --target aarch64-unknown-linux-gnu
      cp -v target/aarch64-unknown-linux-gnu/release/comply /app/comply
      ;;

    "linux/amd64")
      export CARGO_BUILD_VERSION=${APP_VERSION}
      export TARGET=x86_64-unknown-linux-gnu
      export CARGO_BUILD_TARGET=x86_64-unknown-linux-gnu

      # Use mold linker for maximum link speed on amd64
      export RUSTFLAGS="-C linker=clang -C link-arg=-fuse-ld=mold ${RUSTFLAGS:-}"

      cargo build --release --target x86_64-unknown-linux-gnu
      cp -v target/x86_64-unknown-linux-gnu/release/comply /app/comply
      ;;
  esac
EOF

# Runtime stage - minimal image with just the binary and runtime deps
ARG IMAGE_TAG=trixie-slim@sha256:020c0d20b9880058cbe785a9db107156c3c75c2ac944a6aa7ab59f2add76a7bd
# Braced `${IMAGE_TAG}`, matching the dependencies stage above: Scorecard's
# pinned-dependencies check resolves the braced form back to the ARG's digest
# but not the bare `$IMAGE_TAG`, so this line alone read as unpinned.
FROM docker.io/library/debian:${IMAGE_TAG} AS runtime

# Install only runtime dependencies (SSL certs, libssl, tini for signal handling)
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked\
 --mount=type=cache,target=/var/lib/apt,sharing=locked <<EOF
  #!/bin/bash
  set -ex pipefail
  apt-get update
  apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    tini
  rm -rf /var/lib/apt/lists/*
EOF

# Create app directory and non-root user/group with fixed UID/GID
RUN groupadd -g 10000 comply &&\
 useradd -u 10000 -g comply -m -s /sbin/nologin comply

WORKDIR /app

# Copy the binary from builder with ownership root:comply
COPY --from=builder --chown=root:comply /app/comply /app/comply

# Security labels for orchestration tools
LABEL org.opencontainers.image.security.no-new-privileges="true" \
      org.opencontainers.image.security.read-only-rootfs="true" \
      org.opencontainers.image.security.capabilities.drop="ALL" \
      org.opencontainers.image.security.run-as-non-root="true" \
      org.opencontainers.image.security.run-as-user="10000" \
      org.opencontainers.image.security.run-as-group="10000"

# Entrypoint wrapper: allows `docker run comply:0.1.0 bash` for debugging
RUN <<EOF
#!/bin/bash
set -ex pipefail

cat > /entrypoint.sh <<SCRIPT
#!/bin/bash
set -ex pipefail

# If first arg is a shell, exec it directly (for debugging)
case "${1:-}" in
  bash|sh|/bin/bash|/bin/sh)
    exec tini -- "$@"
    ;;
esac

# Default: run comply with args
exec tini -- /app/comply "$@"
SCRIPT
chmod +x /entrypoint.sh
chown root:comply /entrypoint.sh
EOF

# Switch to non-root user
USER comply:comply

ENTRYPOINT ["/entrypoint.sh"]
CMD ["--help"]
