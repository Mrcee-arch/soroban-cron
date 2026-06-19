//! HTTP health-check server (Axum).
//!
//! Exposes a single `GET /health` endpoint that returns a JSON
//! [`HealthResponse`] indicating the node's current status.
//!
//! ## Response schema
//!
//! ```json
//! {
//!   "current_ledger": 12345678,
//!   "last_execution_timestamp": 1718000000,
//!   "in_flight_count": 0
//! }
//! ```
//!
//! `last_execution_timestamp` is a Unix timestamp (seconds) or `null` when no
//! execution has been confirmed yet.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};
use std::sync::Arc;

use axum::{extract::State, routing::get, Json, Router};
use serde::Serialize;
use tracing::info;

use crate::in_flight::InFlightSet;

// ── Response type ─────────────────────────────────────────────────────────────

/// JSON body returned by `GET /health`.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    /// The latest ledger sequence observed by the [`LedgerPoller`].
    pub current_ledger: u32,
    /// Unix timestamp (seconds) of the last successfully confirmed execution.
    /// `null` if no execution has been confirmed since the node started.
    pub last_execution_timestamp: Option<i64>,
    /// Number of tasks currently being processed by the execution pipeline.
    pub in_flight_count: usize,
}

// ── Shared state ──────────────────────────────────────────────────────────────

/// Read-only handles shared between the HTTP handler and the engine components.
#[derive(Clone, Debug)]
pub struct HealthState {
    /// Shared atomic updated by `LedgerPoller`.
    pub current_ledger: Arc<AtomicU32>,
    /// Shared atomic updated by `ExecutionPipeline`.
    pub last_confirmed_ts: Arc<AtomicI64>,
    /// Shared in-flight set.
    pub in_flight: InFlightSet,
}

// ── Handler ───────────────────────────────────────────────────────────────────

async fn health_handler(State(state): State<HealthState>) -> Json<HealthResponse> {
    let current_ledger = state.current_ledger.load(Ordering::Relaxed);
    let ts_raw = state.last_confirmed_ts.load(Ordering::Relaxed);
    let last_execution_timestamp = if ts_raw == 0 { None } else { Some(ts_raw) };
    let in_flight_count = state.in_flight.len();

    Json(HealthResponse {
        current_ledger,
        last_execution_timestamp,
        in_flight_count,
    })
}

// ── HealthServer ──────────────────────────────────────────────────────────────

/// Wraps the Axum router and bind logic.
pub struct HealthServer {
    port: u16,
    state: HealthState,
}

impl HealthServer {
    /// Construct a new [`HealthServer`].
    pub fn new(port: u16, state: HealthState) -> Self {
        HealthServer { port, state }
    }

    /// Start the server and listen indefinitely.
    ///
    /// Bind the listener on `0.0.0.0:{port}` and serve until the task is
    /// cancelled.
    pub async fn run(self) {
        let app = Router::new()
            .route("/health", get(health_handler))
            .with_state(self.state);

        let addr = SocketAddr::from(([0, 0, 0, 0], self.port));
        info!(port = self.port, "health server listening on {addr}");

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .unwrap_or_else(|e| panic!("failed to bind health server on port {}: {e}", self.port));

        axum::serve(listener, app)
            .await
            .expect("health server exited unexpectedly");
    }
}
