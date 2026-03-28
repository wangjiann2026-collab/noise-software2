//! WebSocket endpoint for real-time job-progress streaming.
//!
//! ## Route
//! `GET /ws/jobs/:job_id`
//!
//! Upgrades to a WebSocket connection and streams [`JobEvent`] messages as
//! newline-delimited JSON text frames until the job reaches a terminal state
//! (`completed` or `failed`) or the client disconnects.
//!
//! ## Race-condition handling
//! The handler subscribes to the broadcast channel **before** inspecting the
//! current job status.  This guarantees no events are missed:
//! - If the job is already complete, the final status is sent immediately.
//! - If the job is in-progress, events are forwarded as they arrive.
//!
//! ## Authentication
//! The route is placed under the `authenticated` router group (JWT required).

use axum::{
    extract::{Path, State, WebSocketUpgrade, ws::{Message, WebSocket}},
    response::Response,
    http::StatusCode,
    Json,
};
use tokio::sync::broadcast::error::RecvError;

use crate::state::{AppState, JobEvent};

/// Upgrade the connection to a WebSocket that streams job progress.
pub async fn ws_job_progress(
    State(state): State<AppState>,
    Path(job_id): Path<u64>,
    ws: WebSocketUpgrade,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    if job_id == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Job 0 is reserved" })),
        ));
    }

    // Subscribe first, then read current status — avoids the TOCTOU race.
    let mut rx = state.event_tx.subscribe();

    let current_status = state.jobs.lock()
        .map(|g| g.get(&job_id).map(|r| (r.status.clone(), r.calc_result_id, r.error.clone())))
        .ok()
        .flatten();

    // If the job is already in a terminal state, we can skip the broadcast loop.
    let already_done = current_status.as_ref().map_or(false, |(s, _, _)| {
        s == "completed" || s == "failed"
    });

    if current_status.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("Job {job_id} not found") })),
        ));
    }

    let upgrade = ws.on_upgrade(move |socket| async move {
        if already_done {
            // Emit a synthetic terminal event from the stored record.
            let synthetic = match current_status.as_ref() {
                Some((status, Some(id), _)) if status == "completed" => {
                    Some(JobEvent::Completed { job_id, calc_result_id: *id })
                }
                Some((_, _, Some(err))) => {
                    Some(JobEvent::Failed { job_id, error: err.clone() })
                }
                _ => None,
            };
            handle_socket_already_done(socket, synthetic).await;
        } else {
            handle_socket(socket, job_id, rx).await;
        }
    });

    Ok(upgrade)
}

/// Send one terminal event then close.
async fn handle_socket_already_done(
    mut socket: WebSocket,
    event: Option<JobEvent>,
) {
    if let Some(ev) = event {
        if let Ok(json) = serde_json::to_string(&ev) {
            let _ = socket.send(Message::Text(json.into())).await;
        }
    }
    let _ = socket.close().await;
}

/// Forward broadcast events that belong to `job_id` until terminal or disconnect.
async fn handle_socket(
    mut socket: WebSocket,
    job_id: u64,
    mut rx: tokio::sync::broadcast::Receiver<JobEvent>,
) {
    loop {
        match rx.recv().await {
            Ok(event) if event.job_id() == job_id => {
                let is_terminal = matches!(
                    event,
                    JobEvent::Completed { .. } | JobEvent::Failed { .. }
                );
                match serde_json::to_string(&event) {
                    Ok(json) => {
                        if socket.send(Message::Text(json.into())).await.is_err() {
                            break; // client disconnected
                        }
                    }
                    Err(_) => break,
                }
                if is_terminal {
                    break;
                }
            }
            Ok(_) => {
                // Event for a different job — skip.
            }
            Err(RecvError::Lagged(n)) => {
                // Some events were dropped due to a slow consumer.
                tracing::warn!(job_id, dropped = n, "WebSocket consumer lagged");
                continue;
            }
            Err(RecvError::Closed) => break,
        }
    }
    let _ = socket.close().await;
}
