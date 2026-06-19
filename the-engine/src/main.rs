//! Chronos Keeper Engine — off-chain daemon entry point.
//!
//! ## Startup sequence
//!
//! 1. Load [`EngineConfig`] from environment variables / TOML file.
//!    Exit non-zero on validation failure.
//! 2. Initialise structured tracing (JSON format).
//! 3. Decode the keeper keypair → derive public address + signing key bytes.
//! 4. Construct shared components: [`RpcClient`], [`InFlightSet`],
//!    task cache, [`ExecutionPipeline`], [`GraceMonitor`],
//!    [`LedgerPoller`], [`TaskDiscovery`], [`HealthServer`].
//! 5. Spawn each component as a Tokio task.
//! 6. Wait for `SIGTERM` / `SIGINT`; gracefully abort all tasks on receipt.
//!
//! ## Required environment variables
//!
//! | Variable | Description |
//! |----------|-------------|
//! | `KEEPER_KEYPAIR` | Stellar secret seed (S…) for this keeper node |
//! | `RPC_ENDPOINT_URL` | Soroban RPC endpoint URL |
//! | `ANCHOR_CONTRACT_ADDRESS` | Deployed Anchor contract address (C…) |
//!
//! See [`config::EngineConfig`] for the full list of optional variables.

use std::collections::HashMap;
use std::process;
use std::sync::{Arc, Mutex};

use tokio::sync::broadcast;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

mod config;
mod errors;
mod execution_pipeline;
mod grace_monitor;
mod health_server;
mod in_flight;
mod ledger_poller;
mod rpc_client;
mod task_discovery;

use config::EngineConfig;
use execution_pipeline::ExecutionPipeline;
use grace_monitor::GraceMonitor;
use health_server::{HealthServer, HealthState};
use in_flight::InFlightSet;
use ledger_poller::LedgerPoller;
use rpc_client::RpcClient;
use task_discovery::TaskDiscovery;

#[tokio::main]
async fn main() {
    // ── 1. Structured JSON logging ────────────────────────────────────────────
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .json()
        .init();

    // ── 2. Load and validate configuration ────────────────────────────────────
    let cfg = match EngineConfig::from_env() {
        Ok(c) => {
            info!(
                rpc          = %c.rpc_endpoint_url,
                anchor       = %c.anchor_contract_address,
                health_port  = c.health_port,
                "configuration loaded"
            );
            c
        }
        Err(e) => {
            error!(error = %e, "configuration validation failed — exiting");
            process::exit(1);
        }
    };

    // ── 3. Decode keeper keypair ──────────────────────────────────────────────
    // derive_keypair exits with a clear error message if the secret is invalid.
    let (keeper_address, signing_key_bytes) = derive_keypair(&cfg.keeper_keypair);
    info!(keeper_address, "keeper identity resolved");

    // ── 4. Construct shared components ────────────────────────────────────────
    let rpc = RpcClient::new(&cfg.rpc_endpoint_url, cfg.task_query_timeout);
    let in_flight = InFlightSet::new();
    let task_cache = Arc::new(Mutex::new(HashMap::new()));

    let pipeline = ExecutionPipeline::new(
        rpc.clone(),
        in_flight.clone(),
        keeper_address.clone(),
        signing_key_bytes,
        cfg.anchor_contract_address.clone(),
        cfg.confirmation_timeout,
        cfg.retry_backoff_initial,
        cfg.retry_backoff_max,
        cfg.max_retries,
    );

    let grace_monitor = GraceMonitor::new(
        keeper_address.clone(),
        in_flight.clone(),
        pipeline.clone(),
    );

    let (ledger_tx, _) = broadcast::channel::<ledger_poller::LedgerEvent>(64);

    let poller = LedgerPoller::new(
        rpc.clone(),
        cfg.poll_interval,
        cfg.retry_backoff_initial,
        cfg.retry_backoff_max,
    );

    let ledger_handle       = poller.ledger_handle();
    let last_confirmed_handle = pipeline.last_confirmed_ts_handle();

    let discovery = TaskDiscovery::new(
        rpc,
        cfg.anchor_contract_address.clone(),
        keeper_address,
        in_flight.clone(),
        pipeline,
        grace_monitor,
        task_cache,
        cfg.task_query_timeout,
    );

    let health_state = HealthState {
        current_ledger:    ledger_handle,
        last_confirmed_ts: last_confirmed_handle,
        in_flight,
    };

    let health_server = HealthServer::new(cfg.health_port, health_state);

    // ── 5. Spawn all components as independent Tokio tasks ────────────────────
    let ledger_tx_for_poller = ledger_tx.clone();
    let poller_task = tokio::spawn(async move {
        poller.run(ledger_tx_for_poller).await;
    });

    let discovery_rx = ledger_tx.subscribe();
    let discovery_task = tokio::spawn(async move {
        discovery.run(discovery_rx).await;
    });

    let health_task = tokio::spawn(async move {
        health_server.run().await;
    });

    info!("Chronos Keeper Engine started — all components running");

    // ── 6. Block until SIGINT / SIGTERM, then shut down cleanly ──────────────
    tokio::signal::ctrl_c()
        .await
        .expect("failed to register ctrl-c signal handler");

    info!("shutdown signal received — aborting tasks");

    poller_task.abort();
    discovery_task.abort();
    health_task.abort();

    info!("engine stopped");
}

// ── Keypair helpers ───────────────────────────────────────────────────────────

/// Decode a Stellar secret seed (S…) into:
/// - the corresponding strkey-encoded public address (G…)
/// - the raw 32-byte Ed25519 signing key scalar
///
/// Exits the process with a descriptive error if the secret is invalid.
fn derive_keypair(secret: &str) -> (String, [u8; 32]) {
    use stellar_strkey::ed25519::{PrivateKey, PublicKey};

    let private_key = PrivateKey::from_string(secret).unwrap_or_else(|e| {
        eprintln!(
            "ERROR: KEEPER_KEYPAIR is not a valid Stellar secret seed (S…): {e}\n\
             Generate one with: stellar keys generate --global my-keeper"
        );
        process::exit(1);
    });

    // Derive public key via Ed25519 scalar multiplication.
    let signing_key     = ed25519_dalek::SigningKey::from_bytes(&private_key.0);
    let public_bytes    = signing_key.verifying_key().to_bytes();
    let public_address  = PublicKey(public_bytes).to_string();

    (public_address, private_key.0)
}
