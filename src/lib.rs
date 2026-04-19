// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Michael Schaefer <https://github.com/mischa-robots/agentic-robot>

//! # Agentic Robot
//!
//! An autonomous robot controlled by an AI agent (GitHub Copilot CLI).
//!
//! This binary provides:
//! - Stereo camera capture from two CSI cameras
//! - Motor control via the `robot-control` crate
//! - A web dashboard for observation and emergency stop
//! - History storage of frames, reasoning, and commands
//!
//! ## Architecture
//!
//! The system has three layers:
//! 1. **Hardware Layer** (this binary) — camera + motor + web server
//! 2. **Intelligence Layer** (Copilot CLI) — visual reasoning + decisions
//! 3. **Observer Layer** (web dashboard) — browser UI for monitoring

pub mod camera;
pub mod config;
pub mod daemon;
pub mod error;
pub mod history;
pub mod ipc;
pub mod motor;
pub mod safety;
pub mod server;
