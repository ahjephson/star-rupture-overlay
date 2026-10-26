use serde_json::{Value, json};
use starrupture_wave_core::{
    live::{GAME_HASH, process_response, validate_observation},
    live_timing::{HEAT_SETTINGS, live_wave_timing},
};
fn wave(stage: usize, progress: f64) -> Value {
    let duration = [0.0, 120.0, 10.0, 80.0, 735.0][stage];
    let clock = 10000.0;
    let settings: serde_json::Map<String, Value> =
        HEAT_SETTINGS.iter().map(|(k, v)| (k.to_string(), json!(v))).collect();
    json!({
        "phase": "active_unverified",
        "waitingForNextWave": false,
        "wavesDisabled": false,
        "specificWavePauseFlag": false,
        "previousStageRaw": stage,
        "timingConfiguration": {
            "status": "ok",
            "nativeControlFlowVerified": true,
            "intervalIncludesWarning": true,
            "intervalSeconds": 2400,
            "warningDurationSeconds": 120,
        },
        "actor": {
            "specificPauseFlag": false,
            "nextTime": clock + duration * (1.0 - progress),
            "nextPhase": ([0, 0, 1, 2, 3][stage]),
        },
        "phaseEvidence": {
            "status": "ok",
            "getterLayoutsVerified": true,
            "stage": stage,
            "waveType": 1,
            "progress": progress,
            "inverseDuration": 1.0 / duration,
            "phaseClockEvidence": {
                "status": "ok",
                "nativeUpdateBindingVerified": true,
                "deltaCapSeconds": 0.1,
            },
            "settings": settings,
            "waveStartTimeSeconds": clock + duration * (1.0 - progress)
                - [0.0, 0.0, 10.0, 90.0, 825.0][stage],
        },
    })
}
fn fixture() -> Value {
    json!({
        "status": "ok",
        "schemaVersion": 1,
        "helperVersion": "0.5.0",
        "source": "live_read_only_process_memory",
        "gameMemoryWrites": false,
        "process": {
            "pid": 123,
            "creation_ticks": "134000000000000000",
            "executable_sha256": GAME_HASH,
        },
        "observedAtUnixMilliseconds": 1780000000000i64,
        "position": {
            "x": 1,
            "y": 2,
            "z": 3,
        },
        "readerIsElevated": false,
        "live": {
            "player": {
                "dead": false,
                "viewMode": "builder_drone",
                "mainCharacterPath": "Synthetic character",
            },
            "world": {
                "worldPath": "/Game/Chimera/Maps/ChimeraMain/ChimeraMain.ChimeraMain",
                "origin": {
                    "x": 0,
                    "y": 0,
                    "z": 0,
                },
                "originRebasingEnabled": false,
                "mapCoordinatesCompatible": true,
            },
            "clocks": {
                "simulationSeconds": 10000,
                "audioSeconds": 10000,
                "realSeconds": 10000,
                "unpausedSeconds": 10000,
                "simulationAtObservationStart": 10000,
                "timeDilation": 1,
            },
            "pause": {
                "paused": true,
                "pauseMenuOpen": true,
                "pauserPresent": true,
            },
            "wave": wave(4, 0.5),
            "observationWindow": {
                "startedAtUnixMilliseconds": 1779999999990i64,
                "completedAtUnixMilliseconds": 1780000000000i64,
                "atomic": false,
            },
            "evidence": [],
        },
    })
}
#[test]
fn active_timing_uses_native_progress_rate_and_keeps_actor_drift_diagnostic() {
    for stage in 1..=4 {
        for progress in [0.0, 0.5, 1.0] {
            let mut w = wave(stage, progress);
            w["actor"]["nextTime"] = json!(2400);
            w["actor"]["nextPhase"] = json!(99);
            let r = live_wave_timing(&w, 10000.0);
            let expected = [0.0, 120.0, 10.0, 80.0, 735.0][stage] * (1.0 - progress);
            assert_eq!(r["nextPhaseRemainingSeconds"].as_f64(), Some(expected));
            assert_eq!(r["stageTimingVerification"], "progress_based_estimate");
            assert_eq!(r["actorDeadlineDiagnostic"]["usedForTiming"], false);
            assert_eq!(r["estimate"]["verification"], "conditional_estimate");
        }
    }
    let mut w = wave(4, 0.569159746170044);
    w["phaseEvidence"]["inverseDuration"] = json!((1.0f32 / 735.0) as f64);
    w["phaseEvidence"]["waveStartTimeSeconds"] = json!(137901.28125);
    w["actor"]["nextTime"] = json!(138726.28125);
    let r = live_wave_timing(&w, 138411.75120060518);
    assert!(r["actorDeadlineDiagnostic"]["progressMinusDeadlineSeconds"].as_f64().unwrap() > 2.0);
    assert_eq!(r["stageTimingVerification"], "progress_based_estimate");
}
#[test]
fn invalid_phase_settings_pause_binding_and_cap_fail_closed() {
    for (path, value) in [
        ("/specificWavePauseFlag", json!(true)),
        ("/actor/specificPauseFlag", json!(true)),
        ("/phaseEvidence/waveType", json!(2)),
        ("/phaseEvidence/progress", json!(1.1)),
        ("/phaseEvidence/inverseDuration", json!(0)),
        ("/phaseEvidence/getterLayoutsVerified", json!(false)),
        ("/phaseEvidence/phaseClockEvidence/nativeUpdateBindingVerified", json!(false)),
        ("/phaseEvidence/phaseClockEvidence/deltaCapSeconds", json!(0.2)),
        ("/phaseEvidence/settings/preWaveDuration", json!(30)),
        ("/phaseEvidence/waveStartTimeSeconds", json!(20000)),
        ("/previousStageRaw", json!(2)),
    ] {
        let mut w = wave(4, 0.5);
        *w.pointer_mut(path).unwrap() = value;
        let r = live_wave_timing(&w, 10000.0);
        assert!(r["nextPhaseRemainingSeconds"].is_null(), "{path}");
        assert!(r["ruptureRemainingSeconds"].is_null());
    }
}
#[test]
fn waiting_and_following_cycle_use_live_configuration_without_defaults() {
    let mut w = wave(4, 0.5);
    w.as_object_mut().unwrap().shift_remove("timingConfiguration");
    let r = live_wave_timing(&w, 10000.0);
    assert!(r["ruptureRemainingSeconds"].is_null());
    assert_eq!(r["nextPhaseRemainingSeconds"].as_f64(), Some(367.5));
    w["waitingForNextWave"] = json!(true);
    w["phaseEvidence"] = json!({
        "status": "inactive",
    });
    w["ruptureRemainingSeconds"] = json!(524.4);
    w["countdownBasis"] = json!("native_waiting_countdown");
    let r = live_wave_timing(&w, 10000.0);
    assert_eq!(r["ruptureTimingVerification"], "verified");
    assert!(r["nextPhaseRemainingSeconds"].is_null());
    w["timingConfiguration"] = json!({
        "status": "ok",
        "nativeControlFlowVerified": true,
        "intervalIncludesWarning": true,
        "intervalSeconds": 1800,
        "warningDurationSeconds": 90,
    });
    let r = live_wave_timing(&w, 10000.0);
    assert!((r["nextPhaseRemainingSeconds"].as_f64().unwrap() - 434.4).abs() < 1e-9);
    assert_eq!(r["stageTimingVerification"], "unverified_runtime_configuration");
    assert!(r["waitingIntervalAssumptionSeconds"].is_null());
}
#[test]
fn live_identity_frame_window_and_map_suppression_are_independent_of_saves() {
    let r = fixture();
    let v =
        validate_observation("get_live_player_context", &r, 1780000000000, 1780000000000).unwrap();
    assert_eq!(v["data"]["mapPosition"], r["position"]);
    assert!(v.get("snapshotId").is_none());
    assert_eq!(v["coverage"]["saveSnapshotRefreshed"], false);
    for (path, value) in [
        ("/live/player/dead", json!(true)),
        ("/live/world/origin/x", json!(1)),
        ("/live/world/originRebasingEnabled", json!(true)),
    ] {
        let mut r = fixture();
        *r.pointer_mut(path).unwrap() = value;
        let v = validate_observation("get_live_player_context", &r, 1780000000000, 1780000000000)
            .unwrap();
        assert!(v["data"]["mapPosition"].is_null());
    }
    for (path, value) in [
        ("/process/executable_sha256", json!("wrong")),
        ("/process/creation_ticks", json!(134000000000000000u64)),
        ("/observedAtUnixMilliseconds", json!(1)),
        ("/live/observationWindow/atomic", json!(true)),
        ("/live/world/worldPath", json!("interior")),
        ("/position/x", Value::Null),
        ("/live/clocks/simulationSeconds", json!(-1)),
        ("/schemaVersion", json!(2)),
        ("/helperVersion", json!("unsupported")),
        ("/helperVersion", Value::Null),
    ] {
        let mut r = fixture();
        *r.pointer_mut(path).unwrap() = value;
        assert!(
            validate_observation("get_live_world_state", &r, 1780000000000, 1780000000000).is_err(),
            "{path}"
        );
    }
}
#[test]
fn malformed_failed_and_oversized_helper_outputs_never_become_observations() {
    assert!(process_response("get_live_world_state", b"not json", false, 0, 0).is_err());
    assert!(process_response("get_live_world_state", &vec![b' '; 65537], false, 0, 0).is_err());
    let v = process_response(
        "get_live_world_state",
        br#"{"status":"unavailable","error":"Game is not running"}"#,
        true,
        0,
        0,
    )
    .unwrap();
    assert_eq!(v["error"]["code"], "LIVE_UNAVAILABLE");
    assert!(v["data"].is_null());
    assert!(v["observation"].is_null());
    assert_eq!(v["coverage"]["saveFallback"], false);
}
