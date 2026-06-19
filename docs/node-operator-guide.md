# Node Operator Guide — Chronos Keeper Engine

This guide covers everything you need to run a Chronos Keeper Engine node:
Docker deployment, environment variable configuration, health-check monitoring,
and troubleshooting common issues.

---

## Table of Contents

1. [Requirements](#requirements)
2. [Quick start with Docker](#quick-start-with-docker)
3. [Environment variables](#environment-variables)
4. [Optional TOML config file](#optional-toml-config-file)
5. [Health-check endpoint](#health-check-endpoint)
6. [Metrics and structured logs](#metrics-and-structured-logs)
7. [Troubleshooting](#troubleshooting)

---

## Requirements

| Item | Details |
|------|---------|
| Docker | ≥ 24 (or any OCI-compatible runtime) |
| Stellar keypair | A funded Stellar account with ≥ `MIN_KEEPER_STAKE` XLM to register |
| Soroban RPC URL | Testnet: `https://soroban-testnet.stellar.org`; Mainnet: your provider |
| Anchor contract address | The deployed Chronos Anchor contract ID (C…) |

---

## Quick start with Docker

```bash
docker run -d \
  --name chronos-engine \
  --restart unless-stopped \
  -e KEEPER_KEYPAIR="SXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX" \
  -e RPC_ENDPOINT_URL="https://soroban-testnet.stellar.org" \
  -e ANCHOR_CONTRACT_ADDRESS="CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX" \
  -p 8080:8080 \
  ghcr.io/YOUR_GITHUB_USERNAME/chronos-engine:latest
```

### With a config file

Mount `engine.toml` instead of passing every environment variable:

```bash
docker run -d \
  --name chronos-engine \
  -v /path/to/engine.toml:/engine.toml:ro \
  -e CONFIG_FILE=/engine.toml \
  -p 8080:8080 \
  ghcr.io/YOUR_GITHUB_USERNAME/chronos-engine:latest
```

---

## Environment variables

Environment variables take **highest precedence** — they always override
values in the config file.

### Required

| Variable | Description |
|----------|-------------|
| `KEEPER_KEYPAIR` | Stellar secret seed (starts with `S`, 56 characters). Keep this secret. |
| `RPC_ENDPOINT_URL` | Full URL of the Soroban JSON-RPC endpoint. |
| `ANCHOR_CONTRACT_ADDRESS` | Strkey-encoded contract address of the deployed Anchor (starts with `C`). |

The engine exits immediately with a non-zero code and a descriptive error
message if any required variable is missing or empty.

### Optional (with defaults)

| Variable | Default | Allowed range | Description |
|----------|---------|---------------|-------------|
| `POLL_INTERVAL_SECS` | `2` | 1–5 | How often to poll for new ledgers (seconds). |
| `TASK_QUERY_TIMEOUT_SECS` | `10` | 1–10 | Maximum time for the task-discovery RPC call (seconds). |
| `CONFIRMATION_TIMEOUT_SECS` | `60` | 1–60 | Maximum time to wait for on-chain confirmation (seconds). |
| `RETRY_BACKOFF_INITIAL_SECS` | `1` | ≥ 1 | Starting backoff for transient errors (seconds). |
| `RETRY_BACKOFF_MAX_SECS` | `60` | ≤ 60 | Maximum backoff cap (seconds). |
| `MAX_RETRIES` | `5` | ≥ 0 | Maximum retry attempts per task on transient errors. |
| `HEALTH_PORT` | `8080` | 1–65535 | TCP port for the health-check HTTP server. |
| `CONFIG_FILE` | *(none)* | valid path | Path to an optional TOML config file. |
| `RUST_LOG` | `info` | tracing filter | Log level filter. E.g. `debug`, `the_engine=debug`. |

---

## Optional TOML config file

Create `engine.toml` (or any path, set `CONFIG_FILE` env var to point to it):

```toml
# engine.toml — all fields optional; env vars override these values

keeper_keypair          = "SXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"
rpc_endpoint_url        = "https://soroban-testnet.stellar.org"
anchor_contract_address = "CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"

poll_interval_secs          = 2
task_query_timeout_secs     = 10
confirmation_timeout_secs   = 60
retry_backoff_initial_secs  = 1
retry_backoff_max_secs      = 60
max_retries                 = 5
health_port                 = 8080
```

**Never commit a config file containing your secret key.**  
Use environment variables or a secrets manager in production.

---

## Health-check endpoint

`GET /health` returns a JSON body:

```json
{
  "current_ledger": 12345678,
  "last_execution_timestamp": 1718000000,
  "in_flight_count": 0
}
```

| Field | Type | Description |
|-------|------|-------------|
| `current_ledger` | `u32` | Latest ledger sequence observed by the poller. |
| `last_execution_timestamp` | `i64` or `null` | Unix timestamp (seconds) of the last confirmed execution; `null` if none yet. |
| `in_flight_count` | `usize` | Number of tasks currently being processed. |

HTTP status is always `200 OK` while the process is alive. Use the response
body to determine whether the node is actually tracking the chain.

### Liveness vs readiness

- **Liveness**: HTTP `200` from `/health`.
- **Readiness**: `current_ledger > 0` (the poller has synced at least once).

### Docker HEALTHCHECK

The image ships with:
```
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD wget -qO- http://localhost:8080/health || exit 1
```

---

## Metrics and structured logs

All log output is JSON (`tracing` + `tracing-subscriber` JSON layer).

Example log line:
```json
{
  "timestamp": "2026-05-15T12:34:56.789Z",
  "level": "INFO",
  "target": "the_engine::execution_pipeline",
  "message": "task execution confirmed",
  "task_id": "a3f1...",
  "keeper": "GXXXX...",
  "ledger": 12345700,
  "tx_hash": "deadbeef...",
  "role": "Designated"
}
```

Pipe logs to your preferred aggregator (Datadog, Grafana Loki, CloudWatch,
etc.) via Docker logging drivers.

### Log levels

| Level | Meaning |
|-------|---------|
| `INFO` | Normal operation: startup, ledger advances, confirmed executions |
| `WARN` | Recoverable issues: transient RPC errors, missed executions, lagged broadcast |
| `ERROR` | Non-recoverable failures: max retries reached, contract rejection |

Set `RUST_LOG=the_engine=debug` for verbose per-request logging.

---

## Troubleshooting

### Engine exits immediately on startup

Check the error message. The most common causes:

| Error message | Fix |
|---------------|-----|
| `KEEPER_KEYPAIR: required env var is missing` | Set the `KEEPER_KEYPAIR` env var. |
| `RPC_ENDPOINT_URL: required env var is missing` | Set `RPC_ENDPOINT_URL`. |
| `ANCHOR_CONTRACT_ADDRESS: required env var is missing` | Set `ANCHOR_CONTRACT_ADDRESS`. |
| `POLL_INTERVAL_SECS: must be between 1 and 5` | Use a value between 1 and 5. |
| `config_file: No such file or directory` | Check the `CONFIG_FILE` path. |

### `current_ledger` stuck at 0

The poller cannot reach the RPC endpoint.

- Verify `RPC_ENDPOINT_URL` is reachable from the container.
- Check for network policy or firewall rules blocking outbound HTTPS.
- Inspect logs for `WARN transient RPC error` messages.

### Execution not happening

1. Confirm the Anchor contract is initialised and funded with XLM for rewards.
2. Confirm this node's keeper address is registered on the contract.
3. Confirm tasks have been provisioned with `provision_task`.
4. Check that `next_allowed_execution ≤ current_ledger` for at least one task.
5. Watch for `WARN missed-execution` — this means the window has already passed
   more than 50 ledgers ago and the engine is in post-grace mode.

### SLA violation events in logs

```
WARN missed-execution: designated keeper did not execute in time
  task_id=a3f1... ledgers_elapsed=55 next_allowed=12345600 current_ledger=12345655
```

This means the designated keeper (your node or another) failed to execute
within the 50-ledger grace period. The engine will attempt post-grace execution
(no slash penalty). To avoid SLA violations:

- Reduce `POLL_INTERVAL_SECS` to `1`.
- Ensure the RPC endpoint has low latency.
- Monitor `in_flight_count` — a stuck in-flight task blocks re-discovery.

### High `in_flight_count`

Tasks remain in-flight until confirmation or timeout. If `in_flight_count`
grows without bound, the RPC endpoint may be unhealthy or `CONFIRMATION_TIMEOUT_SECS`
is too short for network conditions. Increase it up to the maximum of 60 seconds.
