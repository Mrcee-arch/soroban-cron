# Wave Program Sprint Plan — Chronos Keeper Network

**Maintainer:** Mrcee-arch  
**Repository:** https://github.com/Mrcee-arch/soroban-cron  
**Target:** Stellar Wave Program on Drips

---

## Overview

Chronos Keeper Network is a production-ready decentralised keeper network for Drips on Stellar/Soroban. This document outlines the sprint-based contribution roadmap for Wave contributors.

---

## Sprint 1: Core Implementation (Current)

### High-Complexity Issues (200 pts each)

**Issue: Implement Transaction Signing**
- **File:** `the-engine/src/execution_pipeline.rs` (build_signed_transaction)
- **Work:** Sign Soroban transactions using stellar-xdr + stellar-strkey
- **Scope:** Add crates, build InvokeHostFunction tx, sign with keeper key, serialize XDR
- **Acceptance:** Signed txs pass sendTransaction RPC; deterministic hashing

**Issue: Implement Keypair Derivation**
- **File:** `the-engine/src/main.rs` (derive_keypair)
- **Work:** Decode Stellar secret seed → derive public key → format as Soroban address
- **Scope:** Import stellar-strkey, decode S format, Ed25519 derivation, return C format
- **Acceptance:** Correct key derivation; rejects invalid seeds; all tests pass

---

## Sprint 2: Testing & Hardening

### Medium-Complexity Issues (150 pts each)

**Issue: Add Property-Based Tests for Slashing**
- Verify slash formula: `new_stake = stake - (stake * 5 / 100)`
- Test edge cases: zero stake, minimum stake, large stakes
- Use proptest crate

**Issue: Add Stress Tests for Engine**
- Simulate 100+ concurrent tasks in-flight
- Test ledger polling under high RPC latency
- Verify graceful degradation

**Issue: Add Docker Compose Test Environment**
- Local Soroban sandbox + engine + mock Drip List
- One-command full integration test
- Document in operator guide

---

## Sprint 3: Documentation & DevEx

### Trivial-Complexity Issues (100 pts each)

**Issue: Add Keeper Setup Tutorial**
- Step-by-step: generate keypair, fund account, register keeper, monitor health
- Include example curl commands
- Add to docs/

**Issue: Add Helm Chart for Kubernetes**
- Deploy engine on k8s with ConfigMap
- Include resource limits, healthcheck, logging

**Issue: Add Grafana Dashboard Template**
- Visualise keeper stats, execution rates, grace periods
- JSON template for import

**Issue: Expand Protocol Spec with Examples**
- Add worked examples for slash calculations
- Include task ID generation walkthrough

---

## Sprint 4: Features & Optimization

### Medium-Complexity Issues (150 pts each)

**Issue: Add Metrics Endpoint**
- Expose Prometheus-compatible `/metrics`
- Track keeper executions, grace periods, slash events
- Counter/gauge types

**Issue: Add Multi-Chain Support**
- Abstract RPC client for different Stellar networks
- Config-driven network selection (testnet/mainnet/public)

**Issue: Optimize Task Discovery**
- Cache task results with TTL
- Reduce RPC calls during stable periods
- Benchmark improvement

---

## Sprint 5: Security Audit & Polish

### High-Complexity Issues (200 pts each)

**Issue: Security Audit Report**
- Review auth logic, overflow handling, replay protection
- Document findings and mitigations
- Recommend best practices for deployers

**Issue: Add Formal Verification**
- Use Z3/SMT solver to prove slash formula correctness
- Verify non-negativity invariant: `new_stake >= 0`
- Document proofs

### Trivial-Complexity Issues (100 pts each)

**Issue: Add Deployment Checklist**
- Pre-deployment security validation script
- Verify contract initialization, keeper registration
- Include in operator guide

**Issue: Add Troubleshooting Guide**
- Common errors + fixes (config validation, RPC issues, grace period edge cases)
- Expand node-operator-guide.md

---

## Work Categories

| Category | Examples | Points |
|----------|----------|--------|
| **Implementation** | Signing, key derivation, metrics | 150-200 |
| **Testing** | Property tests, stress tests, integration | 100-150 |
| **Documentation** | Guides, examples, tutorials, checklists | 100 |
| **DevOps** | Docker, k8s, CI/CD, monitoring | 100-150 |
| **Security** | Audits, formal verification, hardening | 150-200 |
| **Optimization** | Caching, RPC reduction, benchmarking | 100-150 |

---

## Contribution Process

1. Pick an issue labelled `Stellar Wave`
2. Comment to claim it
3. Create a branch: `feature/issue-name`
4. Ensure all checks pass:
   ```bash
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```
5. Submit PR with clear description
6. Maintainer reviews and merges
7. Points awarded upon merge

---

## Success Metrics

- ✅ All TODOs implemented by end of Sprint 2
- ✅ Test coverage > 90% by Sprint 3
- ✅ Zero production bugs by Sprint 5
- ✅ Community engagement: 5+ contributors per sprint

---

**Ready to contribute? Pick an issue and get started!**
