// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Michael Schaefer <https://github.com/mischa-robots/agentic-robot>

//! Web server for the observation dashboard.
//!
//! Provides REST API endpoints and serves the static dashboard UI.
//! The dashboard shows live frames, decision logs, history, and
//! an emergency STOP button.

use actix_web::{web, HttpResponse, Responder};
use serde::Deserialize;

use crate::daemon::DaemonState;

/// GET /api/frame — serve the latest captured frame as JPEG.
pub async fn get_frame(state: web::Data<DaemonState>) -> impl Responder {
    let frame_data = state.latest_frame.lock().await;
    match frame_data.as_ref() {
        Some(data) => HttpResponse::Ok()
            .content_type("image/jpeg")
            .body(data.clone()),
        None => HttpResponse::NotFound().body("no frame captured yet"),
    }
}

/// GET /api/status — return robot status as JSON.
pub async fn get_status(state: web::Data<DaemonState>) -> impl Responder {
    let status = state.get_status().await;
    HttpResponse::Ok().json(status)
}

/// Query parameters for history pagination.
#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    #[serde(default = "default_count")]
    pub count: usize,
}

fn default_count() -> usize {
    20
}

/// GET /api/history — return recent history entries as JSON.
pub async fn get_history(
    state: web::Data<DaemonState>,
    query: web::Query<HistoryQuery>,
) -> impl Responder {
    let history = state.history.lock().await;
    let entries = history.recent(query.count);
    HttpResponse::Ok().json(entries)
}

/// POST /api/stop — emergency stop all motors.
pub async fn post_stop(state: web::Data<DaemonState>) -> impl Responder {
    if let Some(motor) = &state.motor_controller {
        if let Err(e) = motor.stop().await {
            return HttpResponse::InternalServerError()
                .body(format!("failed to stop: {e}"));
        }
    }
    state.watchdog_handle.ping();
    HttpResponse::Ok().body("stopped")
}

/// GET /api/history/{id}/frame — serve a specific history frame.
pub async fn get_history_frame(
    state: web::Data<DaemonState>,
    path: web::Path<String>,
) -> impl Responder {
    let id = path.into_inner();
    let history = state.history.lock().await;
    let entries = history.recent(1000); // TODO: direct lookup by ID

    if let Some(entry) = entries.iter().find(|e| e.id == id) {
        if let Some(frame_path) = &entry.frame_path {
            match std::fs::read(frame_path) {
                Ok(data) => {
                    return HttpResponse::Ok()
                        .content_type("image/jpeg")
                        .body(data);
                }
                Err(_) => return HttpResponse::NotFound().body("frame file not found"),
            }
        }
    }

    HttpResponse::NotFound().body("entry not found")
}

/// Configure all API routes.
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api")
            .route("/frame", web::get().to(get_frame))
            .route("/status", web::get().to(get_status))
            .route("/history", web::get().to(get_history))
            .route("/history/{id}/frame", web::get().to(get_history_frame))
            .route("/stop", web::post().to(post_stop)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_history_count_is_20() {
        assert_eq!(default_count(), 20);
    }
}
