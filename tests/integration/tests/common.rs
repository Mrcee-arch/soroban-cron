/// Shared mock Drip List contract used across all integration tests.
///
/// Each test binary independently registers this contract with the sandbox.
/// It accepts `distribute_wave_splits()` calls and does nothing else —
/// enough to satisfy the cross-contract call in `execute_drip_split`.
use soroban_sdk::{contract, contractimpl, Env};

#[contract]
pub struct MockDripList;

#[contractimpl]
impl MockDripList {
    /// No-op implementation — satisfies the Anchor's cross-contract call.
    pub fn distribute_wave_splits(_env: Env) {}
}
