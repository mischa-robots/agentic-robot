#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Michael Schaefer <https://github.com/mischa-robots/agentic-robot>
# Setup script for installing agentic-robot on a Jetson Nano.
#
# Run this on the Jetson Nano after cloning the repo:
#   sudo ./scripts/setup-jetson.sh

set -euo pipefail

echo "=== Agentic Robot — Jetson Nano Setup ==="
echo ""

# Check we're on a Jetson
if [[ ! -f /etc/nv_tegra_release ]] && [[ ! -d /usr/src/jetson_multimedia_api ]]; then
    echo "WARNING: This doesn't appear to be a Jetson device."
    echo "Continuing anyway..."
fi

echo "1/5 — Installing system dependencies..."
apt-get update -qq
apt-get install -y -qq \
    libclang-dev \
    libopencv-dev \
    pkg-config \
    build-essential \
    i2c-tools \
    curl

echo "2/5 — Checking Rust installation..."
if ! command -v rustup &>/dev/null; then
    echo "Installing Rust via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
else
    echo "Rust already installed: $(rustc --version)"
    rustup update stable
fi

echo "3/5 — Verifying I2C access for PCA9685..."
if ! groups | grep -q i2c; then
    echo "Adding user to i2c group..."
    usermod -aG i2c "$USER"
    echo "NOTE: You may need to log out and back in for i2c access."
fi

if command -v i2cdetect &>/dev/null; then
    echo "I2C devices on bus 1:"
    i2cdetect -y 1 2>/dev/null || echo "(i2cdetect failed — check permissions)"
fi

echo "4/5 — Building agentic-robot..."
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_DIR"
cargo build --release --features hardware

echo "5/5 — Installing binary..."
cp target/release/agentic-robot /usr/local/bin/
chmod +x /usr/local/bin/agentic-robot

echo ""
echo "=== Setup complete ==="
echo ""
echo "Usage:"
echo "  agentic-robot daemon              # start the daemon"
echo "  agentic-robot daemon --port 8080  # custom web dashboard port"
echo "  agentic-robot status              # check if daemon is running"
echo "  agentic-robot capture             # capture a stereo frame"
echo ""
echo "Web dashboard: http://$(hostname -I | awk '{print $1}'):8080"
