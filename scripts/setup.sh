#!/usr/bin/env bash
# scripts/setup.sh - install system deps needed to build rust_solver
#
# rust_solver depends on rust_poker 0.1.5 (see Cargo.toml). rust_poker 0.1.5
# wraps a C hand_indexer library and its build script needs:
#   - cmake
#   - libclang (for bindgen)
#   - a C compiler (gcc/clang)
#
# Run with: sudo ./scripts/setup.sh
# or:       ./scripts/setup.sh  (will prompt for sudo if not root)

set -euo pipefail

if [ "$(id -u)" -ne 0 ]; then
    echo "Re-running with sudo..."
    exec sudo "$0" "$@"
fi

echo "Installing build dependencies for rust_solver..."

apt-get update -qq
apt-get install -y --no-install-recommends \
    cmake \
    libclang-dev \
    build-essential \
    pkg-config

echo
echo "Verifying..."
cmake --version | head -1
which clang
ls /usr/lib/x86_64-linux-gnu/libclang*.so* 2>/dev/null | head -3 || \
    ls /usr/lib/llvm-*/lib/libclang*.so* 2>/dev/null | head -3 || \
    echo "libclang not found in standard locations"

echo
echo "Done. Run 'cargo build --release' to build the solver."
