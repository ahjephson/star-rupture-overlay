pub mod target_triplet;
pub mod type_model;

#[allow(unused)]
use crate::target_triplet::{
    TargetArchitecture, TargetEnvironment, TargetOperatingSystem, TargetTriplet,
};

/// Returns the target that the current executable has been compiled for
pub fn compiled_target_triplet() -> Option<TargetTriplet> {
    include!(concat!(env!("OUT_DIR"), "/", "compiled_target_triplet.in"))
}
