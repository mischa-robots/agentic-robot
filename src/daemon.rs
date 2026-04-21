// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Michael Schaefer <https://github.com/mischa-robots/agentic-robot>

//! Daemon process — the persistent runtime that manages hardware and state.
//!
//! The daemon:
//! - Holds the motor controller (smooth ramping stays active)
//! - Manages camera capture
//! - Runs the web server for the dashboard
//! - Listens on a Unix socket for CLI commands
//! - Runs the watchdog safety timer

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Utc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use crate::camera::{self, CameraCapture, GStreamerCapture};
use crate::config::{self, DaemonConfig, SOCKET_PATH};
use crate::history::{DiskHistoryStore, HistoryStore};
use crate::ipc::{DaemonCommand, DaemonResponse, RobotStatus};
use crate::motor::MotorController;
use crate::safety::{Watchdog, WatchdogHandle};

/// Shared daemon state accessible from web handlers and IPC.
pub struct DaemonState {
    pub motor_controller: Option<MotorController>,
    pub latest_frame: Mutex<Option<Vec<u8>>>,
    pub history: Mutex<Box<dyn HistoryStore>>,
    pub watchdog_handle: WatchdogHandle,
    pub config: DaemonConfig,
    pub started_at: chrono::DateTime<Utc>,
    pub last_command_at: Mutex<Option<chrono::DateTime<Utc>>>,
    pub last_capture_at: Mutex<Option<chrono::DateTime<Utc>>>,
}

impl DaemonState {
    pub async fn get_status(&self) -> RobotStatus {
        let last_cmd = self.last_command_at.lock().await;
        let last_cap = self.last_capture_at.lock().await;
        let history = self.history.lock().await;

        RobotStatus {
            running: true,
            max_speed: self.config.max_speed,
            watchdog_timeout_secs: self.config.watchdog_timeout_secs,
            last_command_at: last_cmd.map(|t| t.to_rfc3339()),
            last_capture_at: last_cap.map(|t| t.to_rfc3339()),
            history_entries: history.entry_count(),
            uptime_secs: (Utc::now() - self.started_at).num_seconds() as u64,
        }
    }
}

/// Run the daemon with the given configuration.
pub async fn run(config: DaemonConfig) -> Result<()> {
    info!("starting agentic-robot daemon");

    // Initialize motor controller
    let motor_controller = match crate::motor::create_hardware_driver(
        &config.i2c_bus,
        config.i2c_addr,
    ) {
        Ok(driver) => {
            match MotorController::new(driver, config.left_factor, config.right_factor, config.max_speed) {
                Ok(ctrl) => Some(ctrl),
                Err(e) => {
                    warn!(%e, "failed to create motor controller — running without motors");
                    None
                }
            }
        }
        Err(e) => {
            warn!(%e, "failed to create motor driver — running without motors");
            None
        }
    };

    // Initialize watchdog
    let watchdog = Watchdog::new(Duration::from_secs(config.watchdog_timeout_secs));
    let watchdog_handle = watchdog.activity_handle();

    // Initialize history store
    let history: Box<dyn HistoryStore> = Box::new(DiskHistoryStore::new(1000));

    // Build shared state
    let state = Arc::new(DaemonState {
        motor_controller,
        latest_frame: Mutex::new(None),
        history: Mutex::new(history),
        watchdog_handle: watchdog_handle.clone(),
        config: config.clone(),
        started_at: Utc::now(),
        last_command_at: Mutex::new(None),
        last_capture_at: Mutex::new(None),
    });

    // Initialize camera
    let camera: Arc<Mutex<Box<dyn CameraCapture>>> = Arc::new(Mutex::new(Box::new(
        GStreamerCapture::new(640, 480, config.swap_cameras),
    )));

    // Spawn watchdog task
    let state_for_watchdog = Arc::clone(&state);
    tokio::spawn(async move {
        watchdog
            .run(move || {
                if state_for_watchdog.motor_controller.is_some() {
                    warn!("watchdog triggered — stopping motors");
                    // Spawn an async task to perform the actual stop
                    let state = Arc::clone(&state_for_watchdog);
                    let handle = tokio::runtime::Handle::current();
                    handle.spawn(async move {
                        if let Some(motor) = &state.motor_controller {
                            if let Err(e) = motor.stop().await {
                                error!("watchdog stop failed: {e}");
                            }
                        }
                    });
                }
            })
            .await;
    });

    // Clean up old socket
    let _ = std::fs::remove_file(SOCKET_PATH);

    // Spawn IPC listener
    let state_for_ipc = Arc::clone(&state);
    let camera_for_ipc = Arc::clone(&camera);
    tokio::spawn(async move {
        if let Err(e) = run_ipc_listener(state_for_ipc, camera_for_ipc).await {
            error!(%e, "IPC listener failed");
        }
    });

    // Start web server
    let state_for_web = Arc::clone(&state);
    let bind_addr = format!("{}:{}", config.host, config.port);
    info!(%bind_addr, "starting web server");

    let static_dir = std::env::current_dir()
        .unwrap_or_default()
        .join("static");

    actix_web::HttpServer::new(move || {
        let app = actix_web::App::new()
            .app_data(actix_web::web::Data::from(Arc::clone(&state_for_web)))
            .configure(crate::server::configure_routes);

        // Serve static files if directory exists
        if static_dir.exists() {
            app.service(
                actix_files::Files::new("/", static_dir.clone())
                    .index_file("index.html"),
            )
        } else {
            app.route("/", actix_web::web::get().to(|| async {
                actix_web::HttpResponse::Ok()
                    .content_type("text/html")
                    .body(include_str!("../static/index.html"))
            }))
        }
    })
    .bind(&bind_addr)
    .context(format!("failed to bind to {bind_addr}"))?
    .run()
    .await
    .context("web server error")?;

    // Cleanup
    let _ = std::fs::remove_file(SOCKET_PATH);
    if let Some(motor) = &state.motor_controller {
        motor.shutdown().await;
    }

    Ok(())
}

/// Run the Unix socket IPC listener.
async fn run_ipc_listener(
    state: Arc<DaemonState>,
    camera: Arc<Mutex<Box<dyn CameraCapture>>>,
) -> Result<()> {
    let listener = UnixListener::bind(SOCKET_PATH)
        .context("failed to bind IPC socket")?;

    info!(path = SOCKET_PATH, "IPC listener started");

    loop {
        let (stream, _) = listener.accept().await?;
        let state = Arc::clone(&state);
        let camera = Arc::clone(&camera);

        tokio::spawn(async move {
            let (reader, mut writer) = stream.into_split();
            let mut buf_reader = BufReader::new(reader);
            let mut line = String::new();

            if buf_reader.read_line(&mut line).await.is_err() {
                return;
            }

            let response = match serde_json::from_str::<DaemonCommand>(&line) {
                Ok(cmd) => handle_command(cmd, &state, &camera).await,
                Err(e) => DaemonResponse::Error {
                    message: format!("invalid command: {e}"),
                },
            };

            let mut resp_json = serde_json::to_string(&response).unwrap_or_default();
            resp_json.push('\n');
            let _ = writer.write_all(resp_json.as_bytes()).await;
        });
    }
}

/// Handle a single daemon command.
async fn handle_command(
    cmd: DaemonCommand,
    state: &DaemonState,
    camera: &Arc<Mutex<Box<dyn CameraCapture>>>,
) -> DaemonResponse {
    // Ping watchdog on any command
    state.watchdog_handle.ping();
    *state.last_command_at.lock().await = Some(Utc::now());

    match cmd {
        DaemonCommand::Capture { output_path } => {
            handle_capture(state, camera, output_path).await
        }
        DaemonCommand::Drive { left, right } => {
            handle_drive(state, left, right).await
        }
        DaemonCommand::Stop => handle_stop(state).await,
        DaemonCommand::Status => {
            let status = state.get_status().await;
            DaemonResponse::Status(status)
        }
        DaemonCommand::Log { message } => {
            let mut history = state.history.lock().await;
            if let Err(e) = history.append_reasoning(&message) {
                return DaemonResponse::Error {
                    message: format!("log failed: {e}"),
                };
            }
            DaemonResponse::Ok
        }
    }
}

async fn handle_capture(
    state: &DaemonState,
    camera: &Arc<Mutex<Box<dyn CameraCapture>>>,
    output_path: Option<String>,
) -> DaemonResponse {
    let mut cam = camera.lock().await;
    match cam.capture() {
        Ok(frame) => {
            // Determine save path
            let path = output_path.unwrap_or_else(|| {
                let data_dir = config::data_dir();
                let frames_dir = data_dir.join("frames");
                let filename = format!("{}.jpg", Utc::now().format("%Y-%m-%dT%H-%M-%S"));
                frames_dir.join(filename).to_string_lossy().to_string()
            });

            let path_buf = std::path::PathBuf::from(&path);
            match camera::save_frame(&frame, &path_buf) {
                Ok(_) => {
                    // Update latest frame for web dashboard
                    *state.latest_frame.lock().await = Some(frame.jpeg_data);
                    *state.last_capture_at.lock().await = Some(Utc::now());

                    // Create history entry
                    let mut history = state.history.lock().await;
                    let _ = history.create_entry(&path);

                    DaemonResponse::Frame { path }
                }
                Err(e) => DaemonResponse::Error {
                    message: format!("save failed: {e}"),
                },
            }
        }
        Err(e) => DaemonResponse::Error {
            message: format!("capture failed: {e}"),
        },
    }
}

async fn handle_drive(state: &DaemonState, left: f32, right: f32) -> DaemonResponse {
    match &state.motor_controller {
        Some(motor) => match motor.drive(left, right).await {
            Ok(()) => {
                let mut history = state.history.lock().await;
                let _ = history.record_command(left, right);
                DaemonResponse::Ok
            }
            Err(e) => DaemonResponse::Error {
                message: format!("drive failed: {e}"),
            },
        },
        None => DaemonResponse::Error {
            message: "no motor controller available".to_string(),
        },
    }
}

async fn handle_stop(state: &DaemonState) -> DaemonResponse {
    match &state.motor_controller {
        Some(motor) => match motor.stop().await {
            Ok(()) => DaemonResponse::Ok,
            Err(e) => DaemonResponse::Error {
                message: format!("stop failed: {e}"),
            },
        },
        None => DaemonResponse::Ok, // No motors, nothing to stop
    }
}
