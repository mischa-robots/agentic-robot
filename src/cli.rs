// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Michael Schaefer <https://github.com/mischa-robots/agentic-robot>

//! CLI command implementations.
//!
//! Each subcommand connects to the daemon via IPC socket when available,
//! or falls back to direct hardware access.

use anyhow::{Context, Result};
use std::path::PathBuf;

use agentic_robot::ipc::{self, DaemonCommand, DaemonResponse};

pub async fn capture(output: Option<PathBuf>) -> Result<()> {
    let response = ipc::send_command(DaemonCommand::Capture {
        output_path: output.map(|p| p.to_string_lossy().to_string()),
    })
    .await
    .context("failed to send capture command — is the daemon running?")?;

    match response {
        DaemonResponse::Frame { path } => {
            println!("{path}");
            Ok(())
        }
        DaemonResponse::Error { message } => {
            anyhow::bail!("capture failed: {message}");
        }
        _ => anyhow::bail!("unexpected response from daemon"),
    }
}

pub async fn drive(left: f32, right: f32) -> Result<()> {
    let response = ipc::send_command(DaemonCommand::Drive { left, right })
        .await
        .context("failed to send drive command — is the daemon running?")?;

    match response {
        DaemonResponse::Ok => {
            println!("driving: left={left:.2}, right={right:.2}");
            Ok(())
        }
        DaemonResponse::Error { message } => {
            anyhow::bail!("drive failed: {message}");
        }
        _ => anyhow::bail!("unexpected response from daemon"),
    }
}

pub async fn stop() -> Result<()> {
    let response = ipc::send_command(DaemonCommand::Stop)
        .await
        .context("failed to send stop command — is the daemon running?")?;

    match response {
        DaemonResponse::Ok => {
            println!("stopped");
            Ok(())
        }
        DaemonResponse::Error { message } => {
            anyhow::bail!("stop failed: {message}");
        }
        _ => anyhow::bail!("unexpected response from daemon"),
    }
}

pub async fn status() -> Result<()> {
    let response = ipc::send_command(DaemonCommand::Status)
        .await
        .context("failed to send status command — is the daemon running?")?;

    match response {
        DaemonResponse::Status(status) => {
            println!("{}", serde_json::to_string_pretty(&status)?);
            Ok(())
        }
        DaemonResponse::Error { message } => {
            anyhow::bail!("status failed: {message}");
        }
        _ => anyhow::bail!("unexpected response from daemon"),
    }
}

pub async fn look() -> Result<()> {
    capture(None).await
}

pub async fn log_message(message: &str) -> Result<()> {
    let response = ipc::send_command(DaemonCommand::Log {
        message: message.to_string(),
    })
    .await
    .context("failed to send log command — is the daemon running?")?;

    match response {
        DaemonResponse::Ok => Ok(()),
        DaemonResponse::Error { message } => {
            anyhow::bail!("log failed: {message}");
        }
        _ => anyhow::bail!("unexpected response from daemon"),
    }
}

#[expect(clippy::too_many_arguments)]
pub async fn daemon(
    host: &str,
    port: u16,
    i2c_bus: &str,
    i2c_addr: u8,
    left_factor: f32,
    right_factor: f32,
    swap: bool,
    max_speed: f32,
    watchdog_timeout: u64,
) -> Result<()> {
    let config = agentic_robot::config::DaemonConfig {
        host: host.to_string(),
        port,
        i2c_bus: i2c_bus.to_string(),
        i2c_addr,
        left_factor,
        right_factor,
        swap_cameras: swap,
        max_speed,
        watchdog_timeout_secs: watchdog_timeout,
    };

    agentic_robot::daemon::run(config).await
}
