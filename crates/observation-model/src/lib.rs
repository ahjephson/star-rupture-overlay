//! Observation envelopes, identity validation and HUD presentation.
pub mod overlay_stream;
pub mod wave;

/// Compatibility identifier carried by the observation's `helperVersion` field.
pub const READER_CONTRACT_VERSION: &str = "0.5.0";
