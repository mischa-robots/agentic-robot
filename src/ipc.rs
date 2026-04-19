// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Michael Schaefer <https://github.com/mischa-robots/agentic-robot>

//! IPC protocol for communication between CLI commands and the daemon.
//!
//! Uses a Unix domain socket with JSON-encoded messages.

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use crate::config::SOCKET_PATH;
use crate::error::AppError;

/// Commands sent from CLI to daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum DaemonCommand {
    /// Capture a stereo frame.
    Capture {
        output_path: Option<String>,
    },
    /// Drive the motors.
    Drive {
        left: f32,
        right: f32,
    },
    /// Stop all motors.
    Stop,
    /// Get status.
    Status,
    /// Log a reasoning message.
    Log {
        message: String,
    },
}

/// Status information from the daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RobotStatus {
    pub running: bool,
    pub max_speed: f32,
    pub watchdog_timeout_secs: u64,
    pub last_command_at: Option<String>,
    pub last_capture_at: Option<String>,
    pub history_entries: u64,
    pub uptime_secs: u64,
}

/// Responses sent from daemon to CLI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonResponse {
    /// Success with no extra data.
    Ok,
    /// A captured frame path.
    Frame { path: String },
    /// Status information.
    Status(RobotStatus),
    /// An error occurred.
    Error { message: String },
}

/// Send a command to the daemon and receive a response.
pub async fn send_command(command: DaemonCommand) -> Result<DaemonResponse, AppError> {
    let stream = UnixStream::connect(SOCKET_PATH)
        .await
        .map_err(|_| AppError::DaemonNotRunning)?;

    let (reader, mut writer) = stream.into_split();

    // Send command as a single JSON line
    let mut msg = serde_json::to_string(&command)
        .map_err(|e| AppError::Ipc(format!("serialize failed: {e}")))?;
    msg.push('\n');
    writer
        .write_all(msg.as_bytes())
        .await
        .map_err(|e| AppError::Ipc(format!("write failed: {e}")))?;

    // Read response
    let mut buf_reader = BufReader::new(reader);
    let mut response_line = String::new();
    buf_reader
        .read_line(&mut response_line)
        .await
        .map_err(|e| AppError::Ipc(format!("read failed: {e}")))?;

    serde_json::from_str(&response_line)
        .map_err(|e| AppError::Ipc(format!("deserialize failed: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_serialization_roundtrip() {
        let cmd = DaemonCommand::Drive {
            left: 0.5,
            right: -0.3,
        };
        let json = serde_json::to_string(&cmd).unwrap();
        let deserialized: DaemonCommand = serde_json::from_str(&json).unwrap();

        match deserialized {
            DaemonCommand::Drive { left, right } => {
                assert!((left - 0.5).abs() < f32::EPSILON);
                assert!((right - (-0.3)).abs() < f32::EPSILON);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn response_serialization_roundtrip() {
        let resp = DaemonResponse::Frame {
            path: "/tmp/frame.jpg".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: DaemonResponse = serde_json::from_str(&json).unwrap();

        match deserialized {
            DaemonResponse::Frame { path } => assert_eq!(path, "/tmp/frame.jpg"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn status_serialization() {
        let status = RobotStatus {
            running: true,
            max_speed: 0.8,
            watchdog_timeout_secs: 5,
            last_command_at: Some("2026-04-19T16:00:00Z".to_string()),
            last_capture_at: None,
            history_entries: 42,
            uptime_secs: 120,
        };
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: RobotStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.history_entries, 42);
        assert!(deserialized.running);
    }

    #[test]
    fn command_tag_format() {
        let cmd = DaemonCommand::Stop;
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains(r#""cmd":"stop"#));
    }
}
