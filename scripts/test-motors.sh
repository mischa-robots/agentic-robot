#!/usr/bin/env bash

# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Michael Schaefer <https://github.com/mischa-robots/agentic-robot>
#
# Motor test script — moves robot in a simple pattern to verify wiring.
#
# Pattern: forward 1s → backward 1s → spin right 1s → spin left 1s → stop
#
# Prerequisites:
#   1. The daemon must be running: agentic-robot daemon
#   2. The robot should be on a flat surface with clearance around it
#
# If motors spin the wrong way, adjust polarity with:
#   agentic-robot daemon --left-factor -1.0 --right-factor 1.0
#
# Usage:
#   ./scripts/test-motors.sh              # default speed 0.6
#   ./scripts/test-motors.sh 0.7          # custom speed

set -euo pipefail

# Resolve binary: prefer PATH, fall back to release/debug build in project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
if command -v agentic-robot &>/dev/null; then
    BINARY="agentic-robot"
elif [[ -x "$PROJECT_ROOT/target/release/agentic-robot" ]]; then
    BINARY="$PROJECT_ROOT/target/release/agentic-robot"
elif [[ -x "$PROJECT_ROOT/target/debug/agentic-robot" ]]; then
    BINARY="$PROJECT_ROOT/target/debug/agentic-robot"
else
    echo "ERROR: agentic-robot binary not found. Build with: cargo build --release"
    exit 1
fi
SPEED="${1:-0.6}"
DURATION=1

echo "=== Motor Test ==="
echo "Speed: $SPEED | Duration per step: ${DURATION}s"
echo ""

# Verify daemon is running
if ! "$BINARY" status &>/dev/null; then
    echo "ERROR: daemon is not running. Start it with: $BINARY daemon"
    exit 1
fi

echo "Starting in 3 seconds — make sure the robot has clearance!"
sleep 3

echo "1/4 — Forward..."
"$BINARY" drive "$SPEED" "$SPEED"
sleep "$DURATION"

echo "2/4 — Backward..."
"$BINARY" drive "-$SPEED" "-$SPEED"
sleep "$DURATION"

echo "3/4 — Spin right..."
"$BINARY" drive "$SPEED" "-$SPEED"
sleep "$DURATION"

echo "4/4 — Spin left..."
"$BINARY" drive "-$SPEED" "$SPEED"
sleep "$DURATION"

echo "Stopping..."
"$BINARY" stop

echo ""
echo "=== Test complete ==="
echo ""
echo "Expected behavior:"
echo "  1. Robot moved forward"
echo "  2. Robot moved backward"
echo "  3. Robot spun clockwise (right)"
echo "  4. Robot spun counter-clockwise (left)"
echo ""
echo "If any direction was wrong, adjust motor factors:"
echo "  Left reversed:  agentic-robot daemon --left-factor -1.0"
echo "  Right reversed: agentic-robot daemon --right-factor -1.0"
echo "  Both reversed:  agentic-robot daemon --left-factor -1.0 --right-factor -1.0"
