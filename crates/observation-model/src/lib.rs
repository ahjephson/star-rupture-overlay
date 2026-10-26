//! Observation envelopes, identity validation and HUD presentation.
pub mod observation;
pub mod overlay_stream;
pub mod wave;

/// Compatibility identifier checked by the typed observation validator.
pub const READER_CONTRACT_VERSION: &str = "0.5.0";
