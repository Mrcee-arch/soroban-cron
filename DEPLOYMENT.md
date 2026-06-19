# Deployment Guide — Chronos Keeper Network

**Last Updated:** June 19, 2026  
**Status:** Production-Ready

This guide covers publishing to GitHub, deploying the Anchor contract, and launching keeper engine nodes.

---

## Table of Contents

1. [Publish to GitHub](#publish-to-github)
2. [Deploy Anchor Contract](#deploy-anchor-contract)
3. [Run Keeper Engine](#run-keeper-engine)
4. [Submit to Drips Wave](#submit-to-drips-wave)

---

## Publish to GitHub

### Prerequisites

- GitHub account with `AlienScroll78` username
- Git installed locally
- Repository created at `https://github.com/AlienScroll78/soroban-cron`

### Commands

```bash
cd "c:\Users\ROYALTY\Documents\DRIPS FOLDER\soroban-cron"

# Initialise Git repository
git init

# Stage all files
git add .

# Create initial commit
git commit -m "feat: Chronos Keeper Network — Soroban keeper network with economic incentives"

# Add remote origin
git remote add origin https://github.com/AlienScroll78/soroban-cron.git

# Rename branch to main (if not already)
git branch -M main

# Push to GitHub
git push -u origin main
```

### Verify

Visit https://github.com/AlienScroll78/soroban-cron to confirm files are pushed.

---

## Deploy Anchor Contract

### Prerequisites

- Stellar CLI (≥ 0.9): [docs.stellar.org/tools/stellar-cli](https://docs.stellar.org/tools/stellar-cli)
- Soroban testnet access
- Funded Stellar account for admin operations
- WASM artifact built: `target/wasm32-unknown-unknown/release/the_anchor.wasm`

### Build WASM Artifact

```bash
cd "c:\Users\ROYALTY\Documents\DRIPS FOLDER\soroban-cron"

# Build the Anchor WASM file
cargo build \
  --target wasm32-unknown-unknown \
  --release \
  --package the-anchor

# Verify the artifact exists
ls -la target/wasm32-unknown-unknown/release/the_anchor.wasm
```

### Deploy to Testnet

```bash
# Set environment variables
export ADMIN_SECRET="SZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ"
export NETWORK="testnet"

# Deploy the WASM contract
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/the_anchor.wasm \
  --source $ADMIN_SECRET \
  --network $NETWORK

# Output will show the deployed contract ID (starts with C)
# Example: CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAA
```

### Initialise the Contract

```bash
export CONTRACT_ID="CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAA"  # From above
export ADMIN_ADDRESS="GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABHF5C"
export NATIVE_TOKEN_ADDRESS="CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAA"
export MIN_KEEPER_STAKE="10000000"  # 1 XLM in stroops

stellar contract invoke \
  --id $CONTRACT_ID \
  --source $ADMIN_SECRET \
  --network testnet \
  -- initialize \
  --admin $ADMIN_ADDRESS \
  --native_token $NATIVE_TOKEN_ADDRESS \
  --min_keeper_stake $MIN_KEEPER_STAKE

# Output: Transaction submitted successfully
```

### Fund the Contract (for rewards)

```bash
# Transfer XLM from admin account to contract for keeper rewards
# (Adjust amount based on expected reward distribution)

stellar contract invoke \
  --id $CONTRACT_ID \
  --source $ADMIN_SECRET \
  --network testnet \
  -- transfer_native \
  --amount 50000000  # 5 XLM in stroops
```

---

## Run Keeper Engine

### Option 1: Docker (Recommended for Production)

#### Build Docker Image

```bash
cd "c:\Users\ROYALTY\Documents\DRIPS FOLDER\soroban-cron"

docker build -t chronos-engine:latest .
```

#### Run Container

```bash
docker run -d \
  --name chronos-engine \
  --restart unless-stopped \
  -e KEEPER_KEYPAIR="SZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ" \
  -e RPC_ENDPOINT_URL="https://soroban-testnet.stellar.org" \
  -e ANCHOR_CONTRACT_ADDRESS="CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAA" \
  -e RUST_LOG=info \
  -p 8080:8080 \
  chronos-engine:latest
```

#### Verify Health

```bash
curl http://localhost:8080/health

# Expected output:
# {
#   "current_ledger": 12345678,
#   "last_execution_timestamp": null,
#   "in_flight_count": 0
# }
```

#### View Logs

```bash
docker logs -f chronos-engine
```

#### Stop Container

```bash
docker stop chronos-engine
docker rm chronos-engine
```

---

### Option 2: Cargo (Development / Local Testing)

```bash
cd "c:\Users\ROYALTY\Documents\DRIPS FOLDER\soroban-cron"

export KEEPER_KEYPAIR="SZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ"
export RPC_ENDPOINT_URL="https://soroban-testnet.stellar.org"
export ANCHOR_CONTRACT_ADDRESS="CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAA"
export RUST_LOG=info

cargo run --release --package the-engine
```

---

### Configuration Options

#### Environment Variables (Highest Priority)

```bash
# Required
KEEPER_KEYPAIR="S..."                          # Keeper's secret seed
RPC_ENDPOINT_URL="https://..."                 # Soroban JSON-RPC endpoint
ANCHOR_CONTRACT_ADDRESS="C..."                 # Deployed Anchor contract

# Optional (with defaults)
POLL_INTERVAL_SECS="2"                         # How often to poll (1-5)
TASK_QUERY_TIMEOUT_SECS="10"                   # Task discovery timeout
CONFIRMATION_TIMEOUT_SECS="60"                 # Tx confirmation timeout
RETRY_BACKOFF_INITIAL_SECS="1"                 # Initial backoff
RETRY_BACKOFF_MAX_SECS="60"                    # Maximum backoff
MAX_RETRIES="5"                                # Retry attempts
HEALTH_PORT="8080"                             # Health endpoint port
RUST_LOG="info"                                # Log level
CONFIG_FILE="/path/to/engine.toml"             # TOML config file
```

#### TOML Configuration File

Create `engine.toml`:

```toml
# Required
keeper_keypair          = "SZZZ..."
rpc_endpoint_url        = "https://soroban-testnet.stellar.org"
anchor_contract_address = "CAAA..."

# Optional (showing defaults)
poll_interval_secs          = 2
task_query_timeout_secs     = 10
confirmation_timeout_secs   = 60
retry_backoff_initial_secs  = 1
retry_backoff_max_secs      = 60
max_retries                 = 5
health_port                 = 8080
```

Then run with:

```bash
export CONFIG_FILE="/path/to/engine.toml"
cargo run --release --package the-engine
```

---

## Submit to Drips Wave

### Step 1: Verify GitHub Repository

1. Navigate to https://github.com/AlienScroll78/soroban-cron
2. Confirm:
   - ✅ Repository is public
   - ✅ README.md is present and complete
   - ✅ LICENSE (MIT) is present
   - ✅ Code has zero warnings
   - ✅ All tests pass
   - ✅ CI/CD pipeline is active

### Step 2: Create Drips Wave Issues

Before submitting, create two high-complexity Wave issues in the repo:

**Issue #1: Implement Transaction Signing**

```markdown
## Title
Implement stellar-xdr transaction signing in ExecutionPipeline

## Complexity
high (200 pts)

## Description
The ExecutionPipeline currently uses a stub that returns unsigned transactions.

**File:** `the-engine/src/execution_pipeline.rs` (function `build_signed_transaction`)

## Required Work
1. Add `stellar-xdr` and `stellar-strkey` crates to Cargo.toml
2. Build a valid Soroban InvokeHostFunction transaction
3. Sign with the keeper's private key (held in `self.signing_key_bytes`)
4. Serialize to XDR

## Acceptance Criteria
- Signed transaction is valid for `sendTransaction` RPC submission
- Transaction hash is deterministic
- All existing tests pass

## Labels
- Stellar Wave
- complexity: high
```

**Issue #2: Implement Keypair Derivation**

```markdown
## Title
Derive Stellar public key from secret seed

## Complexity
high (200 pts)

## Description
The Engine's main function currently derives the keeper address with a stub.

**File:** `the-engine/src/main.rs` (function `derive_keypair`)

## Required Work
1. Add `stellar-strkey` crate to Cargo.toml
2. Decode the secret seed (format: S + 56 base32 chars)
3. Extract public key from decoded bytes
4. Return formatted Soroban address (C + 56 base32 chars)

## Acceptance Criteria
- Correctly decodes valid Stellar secret seeds
- Derives correct public keys
- Formats address in Soroban strkey format
- Rejects invalid seeds with clear error
- All tests pass

## Labels
- Stellar Wave
- complexity: high
```

### Step 3: Submit to Drips Wave Program

1. Go to **https://www.drips.network/wave**
2. Click **Maintainers** → **Repos**
3. Click **Apply**
4. Enter repository:
   - Username: `AlienScroll78`
   - Repository: `soroban-cron`
5. Review and submit

### Step 4: Confirm Approval

- You'll receive email confirmation when the repo is added to the Wave program
- Issues will be available for Wave contributors to pick up
- Monitor GitHub for incoming pull requests and contributions

---

## Monitoring & Maintenance

### Health Check

```bash
# Regular health monitoring
watch -n 5 'curl -s http://localhost:8080/health | jq .'

# Expected output shows live ledger and task counts
```

### Log Rotation

If running in Docker:

```bash
docker run -d \
  --name chronos-engine \
  --log-driver json-file \
  --log-opt max-size=10m \
  --log-opt max-file=3 \
  ...
```

### Upgrades

When new versions are available:

```bash
# Pull latest code
git pull origin main

# Rebuild WASM (if contract changed)
cargo build --target wasm32-unknown-unknown --release --package the-anchor

# Rebuild Docker image
docker build -t chronos-engine:latest .

# Stop old container
docker stop chronos-engine
docker rm chronos-engine

# Start new container
docker run -d ... chronos-engine:latest
```

---

## Troubleshooting

### Engine won't start

Check the error message for missing environment variables:

```bash
# Example error:
# KEEPER_KEYPAIR: required env var is missing — exiting

# Fix:
export KEEPER_KEYPAIR="S..."
docker run ... chronos-engine:latest
```

### Health endpoint returns `current_ledger: 0`

RPC endpoint is unreachable:

```bash
# Test connectivity
curl https://soroban-testnet.stellar.org

# If timeout: check RPC_ENDPOINT_URL and network connectivity
```

### No executions happening

1. Verify tasks are provisioned:
   ```bash
   stellar contract invoke \
     --id $CONTRACT_ID \
     -- get_task \
     --task_id <task_id_hex>
   ```

2. Verify keeper is registered and funded

3. Check logs for `WARN` messages

---

## Production Checklist

Before going live:

- [ ] GitHub repo is public
- [ ] All code pushed to `main` branch
- [ ] CI/CD pipeline shows all green checks
- [ ] Anchor contract deployed to testnet
- [ ] Keeper engine running and health endpoint responding
- [ ] At least one task provisioned
- [ ] Two Wave issues created and visible
- [ ] Drips Wave application submitted
- [ ] Email confirmation received

---

**Deployment is complete. Your project is now live on the Stellar network and ready for Wave contributors.**

