# Wave Contribution Plan — Chronos Keeper Network

## Scoped Issues by Category

### Implementation (200 pts each)

**Implement Transaction Signing** — `execution_pipeline.rs`
Add stellar-xdr + stellar-strkey to sign real Soroban transactions with keeper's private key.

**Implement Keypair Derivation** — `main.rs`
Decode Stellar secret seed (S…) to derive public key and format as Soroban address (C…).

### Testing (150 pts each)

**Property-Based Tests for Slashing** — Test slash formula edge cases: zero stake, minimum stake, large amounts.

**Stress Tests for Engine** — Simulate 100+ concurrent tasks; verify graceful degradation under high RPC latency.

**Docker Compose Integration** — Local Soroban sandbox + engine + mock Drip List for one-command testing.

### Documentation (100 pts each)

**Keeper Setup Tutorial** — Step-by-step guide: generate keypair, fund, register, monitor health.

**Prometheus Metrics Guide** — Expose keeper stats, execution rates, grace period events.

**Troubleshooting Guide** — Expand node-operator-guide.md with common errors and fixes.

**Kubernetes Deployment** — Add Helm chart for k8s deployment with ConfigMap integration.

### Optimization (150 pts)

**Cache Task Discovery** — Add TTL-based caching to reduce RPC calls during stable periods.

**Benchmark Engine Performance** — Profile ledger polling, task discovery, execution throughput.

### Security (200 pts)

**Formal Verification** — Prove slash formula correctness: `new_stake ≥ 0` using Z3/SMT solver.

**Security Audit Report** — Review auth logic, overflow handling, replay protection; document findings.

## Wave Sprint Workflow

1. Browse issues labelled `Stellar Wave`
2. Comment to claim an issue
3. Create branch from `main`
4. Ensure code quality:
   ```bash
   cargo fmt --all -- --check
   cargo clippy --all-targets -- -D warnings
   cargo test --workspace
   ```
5. Submit PR with clear description
6. Merge after review
7. Points awarded

## Success Criteria

- Core TODOs (signing, key derivation) ✅
- Test coverage > 90% ✅
- Production-ready monitoring ✅
- Community engagement 5+ contributors/sprint ✅
