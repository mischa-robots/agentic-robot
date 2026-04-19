# Architecture

## System Overview

The Agentic Robot is a three-layer autonomous system where an AI agent (GitHub Copilot CLI)
controls a physical robot through a Rust hardware abstraction layer, while a human observer
monitors everything through a web dashboard.

```
┌────────────────────────────────────────────────────────────────────────┐
│  OBSERVER (Your Computer — Browser)                                    │
│                                                                        │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │  Web Dashboard (http://<robot-ip>:8080)                          │  │
│  │  ┌────────────┐ ┌──────────────┐ ┌─────────┐ ┌───────────────┐  │  │
│  │  │ Live Frame │ │ Decision Log │ │ History │ │ 🛑 STOP Button│  │  │
│  │  └────────────┘ └──────────────┘ └─────────┘ └───────────────┘  │  │
│  └──────────────────────────────────────────────────────────────────┘  │
│                              ▲ HTTP/WebSocket                           │
└──────────────────────────────┼─────────────────────────────────────────┘
                               │
┌──────────────────────────────┼─────────────────────────────────────────┐
│  JETSON NANO (Robot)         │                                         │
│                              │                                         │
│  ┌───────────────────────────┼────────────────────────────────────┐    │
│  │  Daemon Process (agentic-robot daemon)                         │    │
│  │                                                                │    │
│  │    ┌──────────┐  ┌──────────────┐  ┌──────────┐  ┌──────────┐  │    │
│  │    │ Web      │  │ IPC Listener │  │ Watchdog │  │ History  │  │    │
│  │    │ Server   │  │ (Unix sock)  │  │ Timer    │  │ Storage  │  │    │
│  │    │ (actix)  │  │              │  │ (5s)     │  │ (disk)   │  │    │
│  │    └──────────┘  └──────┬───────┘  └────┬─────┘  └──────────┘  │    │
│  │                        │               │                       │    │
│  │  ┌─────────────────────┼───────────────┼───────────────────┐   │    │
│  │  │  Motor Controller   │               │                   │   │    │
│  │  │  (robot-control)    ◄───────────────┘ (auto-stop)       │   │    │
│  │  │  - Smooth ramping   │                                   │   │    │
│  │  │  - Speed limiting   │                                   │   │    │
│  │  └─────────┬───────────┘                                   │   │    │
│  │            │                                                │   │    │
│  │  ┌─────────▼───────────┐  ┌────────────────────────────┐   │   │    │
│  │  │  PCA9685 (I2C)      │  │  Camera Capture (OpenCV)   │   │   │    │
│  │  │  Motor Driver Board │  │  GStreamer CSI Pipeline     │   │   │    │
│  │  └─────────┬───────────┘  └────────────┬───────────────┘   │   │    │
│  └────────────┼────────────────────────────┼───────────────────┘   │    │
│               │                            │                       │    │
│  ┌────────────▼────────┐    ┌──────────────▼────────────────┐     │    │
│  │  DC Motors (L / R)  │    │  CSI Cameras (Left / Right)   │     │    │
│  └─────────────────────┘    └───────────────────────────────┘     │    │
│                                                                    │    │
│  ┌─────────────────────────────────────────────────────────────┐  │    │
│  │  Copilot CLI (Intelligence Layer)                           │  │    │
│  │                                                             │  │    │
│  │  1. agentic-robot capture  → stereo frame (JPEG)           │  │    │
│  │  2. view frame.jpg         → analyze scene                 │  │    │
│  │  3. agentic-robot log "..."→ record reasoning              │  │    │
│  │  4. agentic-robot drive L R→ execute decision              │  │    │
│  │  5. repeat                                                  │  │    │
│  │                                                             │  │    │
│  │  Connected via: Unix socket (/tmp/agentic-robot.sock)       │  │    │
│  └─────────────────────────────────────────────────────────────┘  │    │
└────────────────────────────────────────────────────────────────────┘    │
```

## Data Flow

### Autonomous Loop (1 cycle ≈ 1-3 seconds)

```
Copilot CLI                    Daemon                         Hardware
    │                            │                              │
    │─── capture ───────────────►│                              │
    │                            │─── GStreamer capture ────────►│ CSI cameras
    │                            │◄── left + right frames ──────│
    │                            │── stack horizontally ──►      │
    │                            │── save JPEG ──►               │
    │                            │── update history ──►          │
    │◄── /path/to/frame.jpg ─────│                              │
    │                            │                              │
    │── (analyze image) ──►      │                              │
    │                            │                              │
    │─── log "I see a wall" ────►│                              │
    │                            │── append to history ──►       │
    │◄── ok ─────────────────────│                              │
    │                            │                              │
    │─── drive 0.6 -0.6 ───────►│                              │
    │                            │── apply dead zone + limit ──►│
    │                            │── robot.drive() ─────────────►│ PCA9685
    │                            │── record command ──►           │
    │◄── ok ─────────────────────│                              │
    │                            │                              │
```

### Safety: Watchdog Flow

```
                    Daemon
                      │
    ┌─────────────────┼──────────────────┐
    │ Watchdog Timer  │                  │
    │                 │                  │
    │  [command received] ──► reset      │
    │  [5s elapsed]   ──► STOP motors   │
    │  [WiFi lost]    ──► no commands   │
    │                      ──► timeout  │
    │                      ──► STOP     │
    └────────────────────────────────────┘
```

## Module Structure

| Module | Responsibility |
|--------|---------------|
| `main.rs` | CLI entry point (clap argument parsing) |
| `cli.rs` | Subcommand implementations (connect to daemon) |
| `config.rs` | Configuration, paths, tracing init |
| `error.rs` | Application error types |
| `camera.rs` | `CameraCapture` trait + GStreamer implementation |
| `motor.rs` | Motor controller with dead zone + speed limiting |
| `daemon.rs` | Daemon process (IPC + web server + state) |
| `server.rs` | REST API routes (actix-web) |
| `history.rs` | `HistoryStore` trait + disk implementation |
| `safety.rs` | Watchdog timer + emergency stop |
| `ipc.rs` | Unix socket protocol (JSON over newline) |

## Testability

Every hardware-dependent component has a trait abstraction:

| Trait | Real Implementation | Mock |
|-------|-------------------|------|
| `CameraCapture` | `GStreamerCapture` | `MockCapture` |
| `MotorDriver` | `Pca9685MotorBoard` | `MockMotorDriver` |
| `HistoryStore` | `DiskHistoryStore` | `InMemoryHistoryStore` |
| `Clock` | `SystemClock` | `FakeClock` |

All tests run without hardware:
```bash
cargo test                       # unit + integration tests (no hardware)
cargo test --features camera     # on Jetson: includes camera tests
cargo test --features pca9685    # on Jetson: includes motor hardware tests
cargo test --features hardware   # on Jetson: all hardware tests
```

## IPC Protocol

Communication between CLI and daemon uses a simple JSON-over-Unix-socket protocol:

**Request** (one JSON object per line):
```json
{"cmd": "drive", "left": 0.6, "right": -0.6}
{"cmd": "capture", "output_path": null}
{"cmd": "stop"}
{"cmd": "status"}
{"cmd": "log", "message": "I see an obstacle"}
```

**Response** (one JSON object per line):
```json
{"type": "ok"}
{"type": "frame", "path": "./history/frames/2026-04-19T16-30-00.jpg"}
{"type": "status", "running": true, "max_speed": 0.8, ...}
{"type": "error", "message": "motor driver not available"}
```

## Web Dashboard API

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/` | GET | Dashboard HTML UI |
| `/api/frame` | GET | Latest stereo frame (JPEG) |
| `/api/status` | GET | Robot status (JSON) |
| `/api/history` | GET | Recent history entries (JSON) |
| `/api/history/{id}/frame` | GET | Specific history frame (JPEG) |
| `/api/stop` | POST | Emergency stop |

## History Storage

```
<project-root>/history/         # git-ignored, stored on USB stick
├── frames/                     # Raw captured frames
│   ├── 2026-04-19T16-30-00.jpg
│   └── ...
└── entries/                    # History entries
    ├── 2026-04-19T16-30-00/
    │   └── entry.json          # {timestamp, frame_path, reasoning[], command}
    └── ...
```

Stored in the project directory (not home) to keep all I/O on the USB stick
and avoid SD card wear on the Jetson Nano.

Retention: configurable, default 1000 entries. Oldest entries are pruned automatically.
