#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Michael Schaefer <https://github.com/mischa-robots/agentic-robot>
# Autonomous robot loop — launches Copilot CLI with the robot prompt.
#
# Usage:
#   ./scripts/autonomous.sh            # start the autonomous loop
#   ./scripts/autonomous.sh --dry-run  # print the prompt without executing
#
# Prerequisites:
#   1. The daemon must be running:  agentic-robot daemon
#   2. GitHub Copilot CLI must be installed and authenticated
#   3. The agentic-robot binary must be in PATH

set -euo pipefail

BINARY="agentic-robot"

# Verify daemon is running
if ! "$BINARY" status &>/dev/null; then
    echo "ERROR: agentic-robot daemon is not running."
    echo "Start it with:  $BINARY daemon"
    exit 1
fi

PROMPT='You are controlling an autonomous robot via CLI commands on a Jetson Nano.

Available commands:
  agentic-robot capture   — capture a stereo frame (left+right cameras), returns path to JPEG
  agentic-robot drive L R — drive motors (L=left, R=right, range -1.0 to 1.0)
  agentic-robot stop      — emergency stop all motors
  agentic-robot status    — check robot status (running, speed, uptime)
  agentic-robot log "msg" — record your reasoning (visible on web dashboard)
  agentic-robot look      — shortcut for capture

Your autonomous loop:
1. Capture a frame with `agentic-robot capture`
2. View the captured image to understand the scene
3. Log your reasoning with `agentic-robot log "what you see and plan to do"`
4. Drive with `agentic-robot drive <left> <right>` (use speeds 0.5-0.8, NEVER below 0.5)
5. Wait briefly, then repeat from step 1

Safety rules:
- Speed range: 0.5 to 0.8 (forward) or -0.5 to -0.8 (backward). Below 0.5 burns motors!
- If you see an obstacle within ~30cm, STOP immediately
- If anything looks wrong or uncertain, STOP first, then investigate
- The watchdog will auto-stop motors if you take more than 5 seconds between commands
- A human is watching via the web dashboard at http://localhost:8080

Start your first cycle now. Capture a frame and describe what you see.'

if [[ "${1:-}" == "--dry-run" ]]; then
    echo "=== Prompt that would be sent to Copilot CLI ==="
    echo "$PROMPT"
    exit 0
fi

echo "Starting autonomous robot loop..."
echo "Web dashboard: http://$(hostname -I | awk '{print $1}'):8080"
echo "Press Ctrl+C to stop"
echo ""

# Launch Copilot CLI with the robot control prompt
exec copilot-cli "$PROMPT"
