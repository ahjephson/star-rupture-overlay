//! Per-save-folder recipe choices. Never persists research amounts or game state.
mod diagnostics;
mod identity;
mod storage;
pub use diagnostics::self_test;
pub(crate) use identity::Binding;
pub(crate) use storage::Remembered;
