// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Michael Schaefer <https://github.com/mischa-robots/agentic-robot>

//! Application configuration and tracing initialization.

use serde::{Deserialize, Serialize};
use tracing_subscriber::EnvFilter;

/// Socket path for daemon IPC.
pub const SOCKET_PATH: &str = "/tmp/agentic-robot.sock";

/// Data directory for history and frame storage.
///
/// Uses `./history/` relative to the current working directory (the project root).
/// This keeps data on the same drive as the project — important when the project
/// lives on a USB stick to avoid SD card wear on Jetson Nano.
pub fn data_dir() -> std::path::PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("history")
}

/// Configuration for the daemon process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub host: String,
    pub port: u16,
    pub i2c_bus: String,
    pub i2c_addr: u8,
    pub left_factor: f32,
    pub right_factor: f32,
    pub swap_cameras: bool,
    pub max_speed: f32,
    pub watchdog_timeout_secs: u64,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            i2c_bus: "/dev/i2c-1".to_string(),
            i2c_addr: 0x60,
            left_factor: -1.0,
            right_factor: 1.0,
            swap_cameras: false,
            max_speed: 0.8,
            watchdog_timeout_secs: 5,
        }
    }
}

/// Initialize tracing with the given verbosity level.
pub fn init_tracing(verbose: u8) {
    let filter = match verbose {
        0 => "agentic_robot=info",
        1 => "agentic_robot=debug",
        _ => "agentic_robot=trace",
    };

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter)),
        )
        .with_target(false)
        .init();
}
