#!/usr/bin/env bash
# Shared by local builds and CI. No repository-wide Rust override.
export NIBE_RUST_VERSION=1.90.0
export NIBE_ZIG_VERSION=0.14.1
export NIBE_ZIGBUILD_VERSION=0.20.1
export NIBE_JUST_VERSION=1.40.0
NIBE_BUILD_TOOLS="${NIBE_BUILD_TOOLS:-${XDG_DATA_HOME:-$HOME/.local/share}/nibe-build-tools}"
export NIBE_BUILD_TOOLS
