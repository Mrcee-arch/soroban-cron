//! In-flight task deduplication set.
//!
//! [`InFlightSet`] wraps an `Arc<Mutex<HashSet<TaskId>>>` and ensures that
//! at most one execution attempt per `TaskId` is active at any point in time.
//!
//! All public methods acquire the mutex for the minimum duration necessary;
//! callers do not need to manage locking manually.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

/// 32-byte task identifier (matches `chronos_types::TaskId` on native targets).
pub type TaskId = [u8; 32];

/// Thread-safe set of task identifiers currently being processed.
///
/// Cloning an [`InFlightSet`] is cheap — both copies share the same underlying
/// data via the inner `Arc`.
#[derive(Clone, Debug, Default)]
pub struct InFlightSet {
    inner: Arc<Mutex<HashSet<TaskId>>>,
}

impl InFlightSet {
    /// Create a new, empty [`InFlightSet`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert `task_id` into the set.
    ///
    /// Returns `true` if the task was newly inserted, `false` if it was
    /// already present (i.e. already in-flight).
    pub fn insert(&self, task_id: TaskId) -> bool {
        self.inner
            .lock()
            .expect("InFlightSet mutex poisoned")
            .insert(task_id)
    }

    /// Remove `task_id` from the set.
    ///
    /// Called after an execution attempt completes (success or failure).
    pub fn remove(&self, task_id: &TaskId) {
        self.inner
            .lock()
            .expect("InFlightSet mutex poisoned")
            .remove(task_id);
    }

    /// Returns `true` if `task_id` is currently in the set.
    pub fn contains(&self, task_id: &TaskId) -> bool {
        self.inner
            .lock()
            .expect("InFlightSet mutex poisoned")
            .contains(task_id)
    }

    /// Returns the number of tasks currently in-flight.
    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .expect("InFlightSet mutex poisoned")
            .len()
    }

    /// Returns `true` if no tasks are currently in-flight.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_id(byte: u8) -> TaskId {
        [byte; 32]
    }

    #[test]
    fn insert_returns_true_for_new_entry() {
        let set = InFlightSet::new();
        assert!(set.insert(make_id(1)));
    }

    #[test]
    fn insert_returns_false_for_duplicate() {
        let set = InFlightSet::new();
        set.insert(make_id(2));
        assert!(!set.insert(make_id(2)));
    }

    #[test]
    fn contains_reflects_membership() {
        let set = InFlightSet::new();
        assert!(!set.contains(&make_id(3)));
        set.insert(make_id(3));
        assert!(set.contains(&make_id(3)));
    }

    #[test]
    fn remove_clears_entry() {
        let set = InFlightSet::new();
        set.insert(make_id(4));
        set.remove(&make_id(4));
        assert!(!set.contains(&make_id(4)));
    }

    #[test]
    fn len_tracks_correctly() {
        let set = InFlightSet::new();
        assert_eq!(set.len(), 0);
        set.insert(make_id(5));
        set.insert(make_id(6));
        assert_eq!(set.len(), 2);
        set.remove(&make_id(5));
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn clone_shares_state() {
        let set = InFlightSet::new();
        let clone = set.clone();
        set.insert(make_id(7));
        // Both handles see the same data.
        assert!(clone.contains(&make_id(7)));
    }

    #[tokio::test]
    async fn concurrent_inserts_never_duplicate() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc as StdArc;

        let set = InFlightSet::new();
        let inserted_count = StdArc::new(AtomicUsize::new(0));

        let task_id = make_id(8);
        let mut handles = vec![];

        for _ in 0..20 {
            let s = set.clone();
            let counter = inserted_count.clone();
            handles.push(tokio::spawn(async move {
                if s.insert(task_id) {
                    counter.fetch_add(1, Ordering::SeqCst);
                }
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        // Exactly one insert should have succeeded.
        assert_eq!(inserted_count.load(Ordering::SeqCst), 1);
        assert_eq!(set.len(), 1);
    }
}
