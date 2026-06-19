# Chronos Keeper Network

[![CI](https://github.com/AlienScroll78/soroban-cron/actions/workflows/ci.yml/badge.svg)](https://github.com/AlienScroll78/soroban-cron/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

A decentralised, crypto-economically secured keeper network that trustlessly
automates time-sensitive Drips funding-split distributions on the
[Stellar / Soroban](https://stellar.org/soroban) blockchain.

No centralised cron jobs. No manual gas payments. Just staked keepers competing
to execute tasks on time — or lose a fraction of their stake.

---

## Why Chronos?

Drips Network relies on periodic on-chain calls to trigger funding-split
distributions. Without automation, those distributions stall. Chronos solves
this with a **permissionless keeper network backed by Proof-of-Stake**:

- Keepers stake XLM to participate.
- Each task is assigned a *designated keeper* who earns a reward for on-time
  execution.
- If the designated keeper misses their window, any other registered keeper
  can step in during a 50-ledger **grace period**, trigger a **5 % slash** on
  the designated keeper, and earn the slash reward on top of the base reward.
- After the grace period any registered keeper can execute — keeping the
  network resilient even in adversarial conditions.

---

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│  Stellar Network (Soroban)                              │
│                                                         │
│   ┌────────────────────────────────────────────────┐   │
│   │  The Anchor  (the-anchor WASM)                 │   │
│   │  • Keeper registry & staking                   │   │
│   │  • Task provisioning (admin-gated)             │   │
│   │  • execute_drip_split (window-arbitrated)      │   │
│   │  • Slashing & treasury accounting              │   │
│   └──────────────────┬─────────────────────────────┘   │
│                      │ cross-contract call              │
│   ┌──────────────────▼─────────────────────────────┐   │
│   │  Drip List Contract  (target)                  │   │
│   │  distribute_wave_splits()                      │   │
│   └────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
          ▲ JSON-RPC (sendTransaction / getLedgerEntries)
          │
┌─────────┴───────────────────────────────────────────────┐
│  The Engine  (the-engine Tokio daemon)                  │
│                                                         │
│  LedgerPoller ──► TaskDiscovery ──► ExecutionPipeline   │
│                        │                                │
│                   GraceMonitor                          │
│                                                         │
│  HealthServer  GET /health  →  { ledger, ts, inflight } │
└─────────────────────────────────────────────────────────┘
```

### Crate layout

| Crate | Role |
|-------|------|
| `chronos-types` | Shared `Keeper` and `ExecutionTask` types; `no_std` + feature-gated `soroban-sdk` |
| `the-anchor` | Soroban WASM contract — registry, tasks, execution, slashing, events |
| `the-engine` | Off-chain Tokio daemon — polling, discovery, execution, health API |
| `tests/integration` | Soroban sandbox integration tests |

---

## Quick-start

### Prerequisites

| Tool | Version | Install |
|------|---------|---------|
| Rust | ≥ 1.78 | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| WASM target | — | `rustup target add wasm32-unknown-unknown` |
| Stellar CLI | ≥ 0.9 | [docs.stellar.org/tools/stellar-cli](https://docs.stellar.org/tools/stellar-cli) |
| Docker | ≥ 24 | [docs.docker.com](https://docs.docker.com/get-docker/) |

### Build

```bash
# Clone
git clone https://github.com/AlienScroll78/soroban-cron.git
cd soroban-cron

# Build everything (native)
cargo build --workspace

# Build the Anchor WASM artifact
cargo build --target wasm32-unknown-unknown --release --package the-anchor

# The WASM file is at:
# target/wasm32-unknown-unknown/release/the_anchor.wasm
```

### Run tests

```bash
# All unit + integration tests
cargo test --workspace

# Anchor tests only
cargo test --package the-anchor

# Engine tests only
cargo test --package the-engine
```

### Run the engine locally (development)

```bash
export KEEPER_KEYPAIR="SCZANGBA4XLMSEGEBHR5QKOH6X6EN73OOXNL7WI736DKCKDKB2XCJXE"
export RPC_ENDPOINT_URL="https://soroban-testnet.stellar.org"
export ANCHOR_CONTRACT_ADDRESS="<deployed-anchor-contract-id>"
export RUST_LOG=info

cargo run --package the-engine
```

The engine will:
1. Validate configuration and exit with a descriptive error if anything is missing.
2. Start polling the Stellar RPC every 2 seconds (default).
3. Discover executable tasks and dispatch transactions.
4. Expose `GET http://localhost:8080/health`.

### Deploy with Docker

```bash
# Build image
docker build -t chronos-engine:latest .

# Run
docker run -d \
  -e KEEPER_KEYPAIR="S..." \
  -e RPC_ENDPOINT_URL="https://soroban-testnet.stellar.org" \
  -e ANCHOR_CONTRACT_ADDRESS="C..." \
  -p 8080:8080 \
  --name chronos-engine \
  chronos-engine:latest

# Check health
curl http://localhost:8080/health
```

---

## Deploy the Anchor contract

```bash
# Upload and deploy using Stellar CLI
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/the_anchor.wasm \
  --source <admin-secret-key> \
  --network testnet

# Initialise (replace placeholders)
stellar contract invoke \
  --id <contract-id> \
  --source <admin-secret-key> \
  --network testnet \
  -- initialize \
  --admin <admin-address> \
  --native_token <native-token-address> \
  --min_keeper_stake 10000000
```

---

## Configuration reference

See [`docs/node-operator-guide.md`](docs/node-operator-guide.md) for the full
list of environment variables and Docker deployment instructions.

---

## Documentation

| Document | Contents |
|----------|----------|
| [`docs/node-operator-guide.md`](docs/node-operator-guide.md) | Docker deployment, env vars, health endpoint, troubleshooting |
| [`docs/protocol-spec.md`](docs/protocol-spec.md) | SLA rules, slashing formula, grace period, reward distribution |

---

## Contributing

This project participates in the **Stellar Wave Program** on
[Drips Wave](https://www.drips.network/wave/stellar). Browse open issues
labelled `Stellar Wave` to find tasks available for contribution.

To contribute:
1. Fork the repository and create a branch from `main`.
2. Make your changes and ensure all checks pass:
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
3. Open a pull request with a clear description of the change.

Issues are tagged by complexity matching the Drips Wave points system:
- `complexity: trivial` — 100 pts
- `complexity: medium` — 150 pts  
- `complexity: high` — 200 pts

---

## License

MIT — see [LICENSE](LICENSE).
