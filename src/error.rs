// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Michael Schaefer <https://github.com/mischa-robots/agentic-robot>

//! Application error types.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("camera error: {0}")]
    Camera(String),

    #[error("motor error: {0}")]
    Motor(#[from] robot_control::RobotError),

    #[error("IPC error: {0}")]
    Ipc(String),

    #[error("daemon not running")]
    DaemonNotRunning,

    #[error("watchdog timeout — motors stopped")]
    WatchdogTimeout,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}
