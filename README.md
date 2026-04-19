# Agentic Robot

An autonomous robot controlled by GitHub Copilot CLI, running on a Jetson Nano.

## What is this?

This is a Rust application that gives `Github Copilot CLI`, `Claude Code CLI`, `Codex CLI` or any other Agent runtime direct control over a physical robot.
The system captures stereo vision from two CSI cameras, lets Copilot CLI analyze scenes
and make driving decisions, while a web dashboard lets you watch the AI reason in real-time.

**Three layers:**
1. **Hardware Layer** — Rust daemon manages cameras, motors, safety watchdog
2. **Intelligence Layer** — Copilot CLI analyzes images and decides actions
3. **Observer Layer** — Web dashboard at `http://<robot-ip>:8080`

## Hardware Requirements

- **Jetson Nano B01** (4GB) with Ubuntu 22.04 (L4T upgrade)
- **2x CSI cameras** (IMX219) — stereo vision
- **PCA9685 motor driver** via I2C
- **DC motors** (left + right)
- WiFi connection (for Copilot CLI and web dashboard)

## Software Requirements

- Rust 1.85+ (install via `rustup`)
- OpenCV 4.x with GStreamer support (pre-built on Jetson)
- `libclang-dev` (for opencv-rust bindgen)
- `Github Copilot CLI`, `Claude Code CLI`, `Codex CLI` or any other Agent runtime installed and authenticated

## Building

```bash
# On the Jetson Nano:
sudo apt install libclang-dev libopencv-dev pkg-config

# Clone the repo with HTTPS (no Github auth required)
git clone https://github.com/mischa-robots/agentic-robot.git
# OR clone with SSH (Github auth key required)
git clone git@github.com:mischa-robots/agentic-robot.git

# Build with full hardware support (cameras + motors)
cd agentic-robot
cargo build --release --features hardware

# Build with only cameras (no motor board)
cargo build --release --features camera

# Build with only motors (no OpenCV needed)
cargo build --release --features pca9685

# Build without hardware (development/testing with fake hardware)
cargo build --release
```

## Usage

### Start the Daemon

```bash
# Start the persistent daemon (web server + motor control + camera)
agentic-robot daemon

# With custom max speed (0.5–1.0, default 0.8)
agentic-robot daemon --max-speed 0.7

# Custom web server port (default 8080)
agentic-robot daemon --port 9090

# Reverse motor direction if wiring is flipped
agentic-robot daemon --left-factor -1.0 --right-factor 1.0
```

### CLI Commands (used by Copilot CLI)

```bash
# Capture a stereo frame (left + right cameras stacked)
agentic-robot capture
# → prints path to saved JPEG

# Drive the robot (left_speed right_speed, range 0.5–1.0 or -0.5–-1.0)
agentic-robot drive 0.6 0.6

# Emergency stop
agentic-robot stop

# Check robot status
agentic-robot status

# Log a reasoning message (appears in web dashboard)
agentic-robot log "I see a wall ahead, turning right"

# Convenience: capture + print path
agentic-robot look
```

### Web Dashboard

Open `http://<jetson-ip>:8080` in your browser to see:
- **Live stereo frame** (auto-refreshes every 2s)
- **Decision log** (Copilot CLI's reasoning in real-time)
- **History browser** (past frames + decisions)
- **🛑 STOP button** (emergency motor stop)

## Copilot CLI Autonomous Loop

Once the daemon is running, Copilot CLI operates in a loop:

1. `agentic-robot capture` → gets stereo frame path
2. Views/analyzes the image
3. Reasons about obstacles, paths, objects
4. `agentic-robot log "reasoning..."` → records decision
5. `agentic-robot drive <left> <right>` → executes
6. Repeat (~1-3 seconds per cycle)

## Safety Features

- **Watchdog timer** — motors stop automatically if no command received in 5 seconds
- **Dead zone protection** — speeds below 0.5 are treated as stop (prevents motor burn)
- **Max speed limit** — configurable cap (default 0.8) prevents runaway
- **Emergency STOP** — web button or `agentic-robot stop` CLI command
- **Graceful shutdown** — SIGINT/SIGTERM stops all motors
- **WiFi loss** — watchdog triggers stop (no commands = timeout)

## Data Storage

All runtime data is stored in `./history/` (project directory, not home):

```
<project-root>/
└── history/              # git-ignored
    ├── frames/           # Captured stereo frames (JPEG)
    └── entries/          # Decision history
        └── <timestamp>/
            └── entry.json  # {frame_path, reasoning[], command}
```

This keeps data on the USB stick alongside the project — avoiding SD card wear on the Jetson Nano.
Retention: 1000 entries (oldest pruned automatically).

## Testing

```bash
# Run all unit tests (no hardware needed)
cargo test

# Run with verbose output
cargo test -- --nocapture

# Run clippy lints
cargo clippy --all-targets -- -D warnings
```

## Architecture

See [ARCHITECTURE.md](./ARCHITECTURE.md) for detailed system design, data flow diagrams,
module responsibilities, and IPC protocol specification.

## License

MIT

Copyright (c) 2026 Michael Schaefer https://github.com/mischa-robots/agentic-robot
