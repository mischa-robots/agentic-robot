// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Michael Schaefer <https://github.com/mischa-robots/agentic-robot>

use anyhow::Result;
use clap::{Parser, Subcommand};

mod cli;

/// An autonomous robot controlled by AI agent.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Enable verbose output
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Capture a stereo frame from both CSI cameras
    Capture {
        /// Output path for the captured frame (default: auto-generated in history dir)
        #[arg(short, long)]
        output: Option<std::path::PathBuf>,
    },

    /// Send a drive command to the motors
    Drive {
        /// Left motor speed (-1.0 to 1.0)
        #[arg(allow_negative_numbers = true)]
        left: f32,
        /// Right motor speed (-1.0 to 1.0)
        #[arg(allow_negative_numbers = true)]
        right: f32,
    },

    /// Stop all motors immediately
    Stop,

    /// Get robot status
    Status,

    /// Capture a frame and print its path (convenience for Copilot CLI)
    Look,

    /// Log a reasoning message to the current history entry
    Log {
        /// The reasoning or decision message to log
        message: String,
    },

    /// Start the daemon (web server + motor control + watchdog)
    Daemon {
        /// Host to bind the web server to
        #[arg(long, default_value = "0.0.0.0")]
        host: String,

        /// Port to bind the web server to
        #[arg(long, default_value_t = 8080)]
        port: u16,

        /// I2C bus device path
        #[arg(long, default_value = "/dev/i2c-1")]
        i2c_bus: String,

        /// I2C address of the PCA9685 board
        #[arg(long, default_value_t = 0x60)]
        i2c_addr: u8,

        /// Left motor speed factor (-1.0 to reverse polarity)
        #[arg(long, default_value_t = -1.0, allow_negative_numbers = true)]
        left_factor: f32,

        /// Right motor speed factor (-1.0 to reverse polarity)
        #[arg(long, default_value_t = 1.0, allow_negative_numbers = true)]
        right_factor: f32,

        /// Swap left/right camera sensor mapping
        #[arg(long)]
        swap: bool,

        /// Maximum allowed speed (0.5 to 1.0) for safety
        #[arg(long, default_value_t = 0.8)]
        max_speed: f32,

        /// Watchdog timeout in seconds (stops motors if no command received)
        #[arg(long, default_value_t = 5)]
        watchdog_timeout: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drive_accepts_positive_speeds() {
        let cli = Cli::try_parse_from(["agentic-robot", "drive", "0.6", "0.6"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        match cli.command {
            Command::Drive { left, right } => {
                assert!((left - 0.6).abs() < f32::EPSILON);
                assert!((right - 0.6).abs() < f32::EPSILON);
            }
            _ => panic!("expected Drive command"),
        }
    }

    #[test]
    fn drive_accepts_negative_speeds() {
        let cli = Cli::try_parse_from(["agentic-robot", "drive", "-0.6", "-0.6"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        match cli.command {
            Command::Drive { left, right } => {
                assert!((left - (-0.6)).abs() < f32::EPSILON);
                assert!((right - (-0.6)).abs() < f32::EPSILON);
            }
            _ => panic!("expected Drive command"),
        }
    }

    #[test]
    fn drive_accepts_mixed_speeds() {
        let cli = Cli::try_parse_from(["agentic-robot", "drive", "0.6", "-0.6"]);
        assert!(cli.is_ok());
    }

    #[test]
    fn stop_command_parses() {
        let cli = Cli::try_parse_from(["agentic-robot", "stop"]);
        assert!(cli.is_ok());
        assert!(matches!(cli.unwrap().command, Command::Stop));
    }

    #[test]
    fn capture_command_parses() {
        let cli = Cli::try_parse_from(["agentic-robot", "capture"]);
        assert!(cli.is_ok());
    }

    #[test]
    fn log_command_parses() {
        let cli = Cli::try_parse_from(["agentic-robot", "log", "wall ahead, turning right"]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Command::Log { message } => assert_eq!(message, "wall ahead, turning right"),
            _ => panic!("expected Log command"),
        }
    }

    #[test]
    fn daemon_accepts_negative_left_factor() {
        let cli = Cli::try_parse_from([
            "agentic-robot", "daemon", "--left-factor", "-1.0",
        ]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Command::Daemon { left_factor, .. } => {
                assert!((left_factor - (-1.0)).abs() < f32::EPSILON);
            }
            _ => panic!("expected Daemon command"),
        }
    }

    #[test]
    fn daemon_accepts_negative_right_factor() {
        let cli = Cli::try_parse_from([
            "agentic-robot", "daemon", "--right-factor", "-1.0",
        ]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Command::Daemon { right_factor, .. } => {
                assert!((right_factor - (-1.0)).abs() < f32::EPSILON);
            }
            _ => panic!("expected Daemon command"),
        }
    }

    #[test]
    fn daemon_accepts_both_negative_factors() {
        let cli = Cli::try_parse_from([
            "agentic-robot", "daemon",
            "--left-factor", "-1.0",
            "--right-factor", "-1.0",
        ]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Command::Daemon { left_factor, right_factor, .. } => {
                assert!((left_factor - (-1.0)).abs() < f32::EPSILON);
                assert!((right_factor - (-1.0)).abs() < f32::EPSILON);
            }
            _ => panic!("expected Daemon command"),
        }
    }

    #[test]
    fn daemon_default_left_factor_is_negative() {
        // Default is -1.0 — ensure it round-trips without explicit flag
        let cli = Cli::try_parse_from(["agentic-robot", "daemon"]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Command::Daemon {
                left_factor,
                right_factor,
                swap,
                ..
            } => {
                assert!((left_factor - (-1.0)).abs() < f32::EPSILON);
                assert!((right_factor - 1.0).abs() < f32::EPSILON);
                assert!(!swap);
            }
            _ => panic!("expected Daemon command"),
        }
    }

    #[test]
    fn daemon_accepts_swap_flag() {
        let cli = Cli::try_parse_from(["agentic-robot", "daemon", "--swap"]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Command::Daemon { swap, .. } => assert!(swap),
            _ => panic!("expected Daemon command"),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli_args = Cli::parse();

    agentic_robot::config::init_tracing(cli_args.verbose);

    match cli_args.command {
        Command::Capture { output } => cli::capture(output).await,
        Command::Drive { left, right } => cli::drive(left, right).await,
        Command::Stop => cli::stop().await,
        Command::Status => cli::status().await,
        Command::Look => cli::look().await,
        Command::Log { message } => cli::log_message(&message).await,
        Command::Daemon {
            host,
            port,
            i2c_bus,
            i2c_addr,
            left_factor,
            right_factor,
            swap,
            max_speed,
            watchdog_timeout,
        } => {
            cli::daemon(
                &host,
                port,
                &i2c_bus,
                i2c_addr,
                left_factor,
                right_factor,
                swap,
                max_speed,
                watchdog_timeout,
            )
            .await
        }
    }
}
