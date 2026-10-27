//! Recoverable observation conflicts, independent of display text.
use std::{error::Error, fmt};
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
