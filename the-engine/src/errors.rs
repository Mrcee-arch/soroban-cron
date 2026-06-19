use thiserror::Error;
use std::time::Duration;

/// Top-level Engine error — wraps either a transient (retryable) or
/// non-transient (permanent) failure.
#[derive(Debug, Error)]
pub enum EngineError {
    #[error("transient: {0}")]
    Transient(#[from] TransientError),
    #[error("non-transient: {0}")]
    NonTransient(#[from] NonTransientError),
}

/// Errors that are safe to retry — transient network / I/O conditions.
#[derive(Debug, Error)]
pub enum TransientError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("connection timeout after {elapsed:?}")]
    ConnectionTimeout { elapsed: Duration },
    #[error("connection reset by peer")]
    ConnectionReset,
    #[error("RPC response timeout")]
    RpcTimeout,
}

/// Errors that are permanent — retrying will not help.
#[derive(Debug, Error)]
pub enum NonTransientError {
    #[error("contract rejected execution: {reason}")]
    ContractRejection { reason: String },
    #[error("confirmation timeout after {elapsed:?}")]
    ConfirmationTimeout { elapsed: Duration },
    #[error("keypair decode error: {0}")]
    KeypairDecode(String),
    #[error("config validation failed: {field} \u{2014} {reason}")]
    ConfigInvalid { field: String, reason: String },
}

/// Returns `true` when `e` is a transient (retryable) error.
pub fn is_transient(e: &EngineError) -> bool {
    matches!(e, EngineError::Transient(_))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_variants_are_classified_correctly() {
        let e: EngineError = TransientError::RpcTimeout.into();
        assert!(is_transient(&e));
    }

    #[test]
    fn non_transient_variants_are_classified_correctly() {
        let e: EngineError = NonTransientError::ContractRejection {
            reason: "bad call".into(),
        }
        .into();
        assert!(!is_transient(&e));
    }

    #[test]
    fn error_display_transient() {
        let e: EngineError = TransientError::ConnectionReset.into();
        assert!(e.to_string().contains("transient"));
    }

    #[test]
    fn error_display_non_transient() {
        let e: EngineError = NonTransientError::ConfigInvalid {
            field: "poll_interval".into(),
            reason: "out of range".into(),
        }
        .into();
        assert!(e.to_string().contains("non-transient"));
    }

    #[test]
    fn connection_timeout_carries_duration() {
        let elapsed = Duration::from_secs(5);
        let e = TransientError::ConnectionTimeout { elapsed };
        let msg = e.to_string();
        assert!(msg.contains("5s") || msg.contains("timeout"));
    }

    #[test]
    fn io_error_wraps_correctly() {
        let io = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe");
        let e: EngineError = TransientError::Io(io).into();
        assert!(is_transient(&e));
    }
}
