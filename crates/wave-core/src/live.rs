//! Validate typed reader observations before calculating display timing.
use crate::{AppError, live_timing::live_wave_timing};
pub use starrupture_observation_model::overlay_stream::GAME_HASH;
use starrupture_observation_model::{
    READER_CONTRACT_VERSION,
    observation::{Observation, OverlaySession, RawObservation, WORLD_PATH},
};
fn valid(value: bool, message: &str) -> Result<(), AppError> {
    if value {
        Ok(())
    } else {
        Err(AppError::new("LIVE_READ_FAILED", message))
    }
}
pub fn validate_observation(
    raw: RawObservation,
    start: i64,
    now: i64,
) -> Result<Observation, AppError> {
    valid(
        raw.schema_version == 1
            && raw.contract_version == READER_CONTRACT_VERSION
            && raw.source_is_live_memory
            && !raw.game_memory_writes,
        "Unsupported observation contract.",
    )?;
    valid(
        raw.process.pid > 0
            && raw.process.creation_ticks > 0
            && raw.process.executable_sha256 == GAME_HASH,
        "Unsupported process identity.",
    )?;
    valid(
        raw.sample.observed_at_ms as i128 >= start as i128 - 1000
            && raw.sample.observed_at_ms as i128 <= now as i128 + 1000,
        "Stale or invalid live timestamp.",
    )?;
    valid(
        [raw.sample.position.x, raw.sample.position.y, raw.sample.position.z]
            .iter()
            .all(|v| v.abs() < 100000000.0),
        "Invalid live position.",
    )?;
    let live = raw.live;
    valid(live.world.world_path == WORLD_PATH, "Unsupported world frame.")?;
    valid(
        [
            live.clocks.simulation_seconds,
            live.clocks.audio_seconds,
            live.clocks.real_seconds,
            live.clocks.unpaused_seconds,
            live.clocks.simulation_at_start,
            live.clocks.time_dilation,
        ]
        .iter()
        .all(|v| v.is_finite() && *v >= 0.0),
        "Invalid live clocks.",
    )?;
    valid(
        live.window.started_ms as i128 >= start as i128 - 1000
            && live.window.completed_ms >= live.window.started_ms
            && live.window.completed_ms <= raw.sample.observed_at_ms
            && !live.window.atomic,
        "Invalid observation window.",
    )?;
    let ms = (raw.process.creation_ticks as i128 - 116444736000000000i128) / 10000;
    valid(
        ms > 0 && ms <= now as i128 && ms <= 9_007_199_254_740_991,
        "Invalid process creation time.",
    )?;
    let map = live.world.map_coordinates_compatible
        && !live.world.origin_rebasing_enabled
        && live.world.origin == [0, 0, 0]
        && !live.player.dead;
    let wave = live_wave_timing(&live.wave, live.clocks.simulation_seconds);
    Ok(Observation {
        process: raw.process,
        overlay_session: OverlaySession::default(),
        observed_at_ms: raw.sample.observed_at_ms,
        window: live.window,
        reader_is_elevated: raw.sample.reader_is_elevated,
        position: raw.sample.position,
        map_position: map.then_some(raw.sample.position),
        world: live.world,
        clocks: live.clocks,
        pause: live.pause,
        wave,
    })
}
