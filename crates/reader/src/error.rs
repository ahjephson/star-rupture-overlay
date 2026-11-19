//! Recoverable observation conflicts, independent of display text.
use std::{error::Error, fmt};
#[derive(Debug)]
pub(crate) struct WorldNotLoaded;
impl fmt::Display for WorldNotLoaded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("No loaded local gameplay world")
    }
}
impl Error for WorldNotLoaded {}
pub(crate) fn local_controller(locals: &[u64]) -> anyhow::Result<u64> {
    match locals {
        [] => Err(WorldNotLoaded.into()),
        [address] => Ok(*address),
        _ => anyhow::bail!("Ambiguous local controllers"),
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ObservationConflict {
    Phase,
    Wave,
    TimerConfiguration,
}
impl fmt::Display for ObservationConflict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Phase => "Phase changed while sampling",
            Self::Wave => "Wave phase changed during observation",
            Self::TimerConfiguration => "Timer configuration changed during observation",
        })
    }
}
impl Error for ObservationConflict {}
pub(crate) fn is_retryable(error: &anyhow::Error) -> bool {
    error.downcast_ref::<ObservationConflict>().is_some()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absent_local_world_is_distinct_from_ambiguous_or_failed_reads() {
        assert!(local_controller(&[]).unwrap_err().is::<WorldNotLoaded>());
        assert_eq!(local_controller(&[42]).unwrap(), 42);
        assert!(!local_controller(&[42, 43]).unwrap_err().is::<WorldNotLoaded>());
        assert!(!anyhow::Error::msg(WorldNotLoaded.to_string()).is::<WorldNotLoaded>());
    }
    #[test]
    fn recovery_uses_error_type_through_context_and_not_message_text() {
        for conflict in [
            ObservationConflict::Phase,
            ObservationConflict::Wave,
            ObservationConflict::TimerConfiguration,
        ] {
            let message = conflict.to_string();
            assert!(is_retryable(&anyhow::Error::new(conflict).context("Additional context")));
            assert!(!is_retryable(&anyhow::Error::msg(message)));
        }
    }
}
