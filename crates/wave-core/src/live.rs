//! Pure validation of read-only world observations; no MCP or process orchestration.
use crate::{AppError,facts::{Result,check},live_timing::live_wave_timing,saves::iso};
use serde_json::{Value,json};
pub const GAME_HASH: &str = "95f95eee0b1913b8716de7fa5bf19877a91abe86e63bf2b6f0a29bd48d1d76f1";
const LIMIT: usize = 65536;
pub fn unavailable(code: &str, message: impl Into<String>) -> Value {
    json!({"status":"unavailable","observation":null,"data":null,"error":{"code":code,"message":message.into()},"coverage":{"source":"live_process_memory","saveFallback":false}})
}
fn valid(v: bool, message: &str) -> Result<()> {
    check(v, "LIVE_READ_FAILED", message)
}
fn finite(v: &Value) -> bool {
    v.as_f64().is_some_and(f64::is_finite)
}
fn integer(v: &Value) -> bool {
    v.as_f64()
        .is_some_and(|v| v.fract() == 0.0 && v.abs() <= 9_007_199_254_740_991.0)
}
/// Pure validation entry point for offline retained-evidence replay; production always runs a pinned helper first.
pub fn validate_observation(tool: &str, r: &Value, start: i64, now: i64) -> Result<Value> {
    valid(r.is_object(), "Invalid helper response.")?;
    valid(
        r["schemaVersion"].as_f64() == Some(1.0)
            && r["helperVersion"] == "0.5.0"
            && r["source"] == "live_read_only_process_memory"
            && r["gameMemoryWrites"] == false,
        "Unsupported live helper contract.",
    )?;
    let process = &r["process"];
    let ticks = process["creation_ticks"].as_str();
    valid(
        process.is_object()
            && integer(&process["pid"])
            && process["pid"].as_f64().is_some_and(|v| v > 0.0)
            && ticks.is_some_and(|v| {
                !v.is_empty() && v.len() <= 20 && v.bytes().all(|b| b.is_ascii_digit())
            })
            && process["executable_sha256"] == GAME_HASH,
        "Unsupported process identity.",
    )?;
    valid(
        r["observedAtUnixMilliseconds"]
            .as_f64()
            .is_some_and(|v| v >= (start - 1000) as f64 && v <= (now + 1000) as f64),
        "Stale or invalid live timestamp.",
    )?;
    valid(
        r["position"].is_object()
            && ["x", "y", "z"].iter().all(|k| {
                r["position"][k]
                    .as_f64()
                    .is_some_and(|v| v.abs() < 100000000.0)
            }),
        "Invalid live position.",
    )?;
    let l = &r["live"];
    valid(
        l.is_object()
            && [
                "player",
                "world",
                "clocks",
                "pause",
                "wave",
                "observationWindow",
            ]
            .iter()
            .all(|k| l[k].is_object()),
        "Missing live context.",
    )?;
    valid(
        l["player"]["dead"].is_boolean()
            && ["on_foot", "builder_drone", "unknown"]
                .iter()
                .any(|s| l["player"]["viewMode"] == *s)
            && l["player"]["mainCharacterPath"].is_string(),
        "Invalid player context.",
    )?;
    valid(
        l["world"]["worldPath"] == "/Game/Chimera/Maps/ChimeraMain/ChimeraMain.ChimeraMain"
            && l["world"]["origin"].is_object()
            && ["x", "y", "z"]
                .iter()
                .all(|k| integer(&l["world"]["origin"][k]))
            && l["world"]["originRebasingEnabled"].is_boolean()
            && l["world"]["mapCoordinatesCompatible"].is_boolean(),
        "Unsupported world frame.",
    )?;
    valid(
        [
            "simulationSeconds",
            "audioSeconds",
            "realSeconds",
            "unpausedSeconds",
            "simulationAtObservationStart",
            "timeDilation",
        ]
        .iter()
        .all(|k| finite(&l["clocks"][k]) && l["clocks"][k].as_f64().unwrap() >= 0.0),
        "Invalid live clocks.",
    )?;
    valid(
        l["pause"]
            .get("paused")
            .is_some_and(|v| v.is_null() || v.is_boolean())
            && l["pause"]["pauseMenuOpen"].is_boolean()
            && l["pause"]["pauserPresent"].is_boolean(),
        "Invalid pause evidence.",
    )?;
    valid(
        l["wave"]["waitingForNextWave"].is_boolean()
            && l["wave"]["wavesDisabled"].is_boolean()
            && ["waiting", "disabled", "active_unverified"]
                .iter()
                .any(|s| l["wave"]["phase"] == *s),
        "Invalid live wave evidence.",
    )?;
    let w = &l["observationWindow"];
    let observed = r["observedAtUnixMilliseconds"].as_f64().unwrap();
    valid(
        finite(&w["startedAtUnixMilliseconds"])
            && finite(&w["completedAtUnixMilliseconds"])
            && w["startedAtUnixMilliseconds"].as_f64().unwrap() >= (start - 1000) as f64
            && w["completedAtUnixMilliseconds"].as_f64().unwrap()
                >= w["startedAtUnixMilliseconds"].as_f64().unwrap()
            && w["completedAtUnixMilliseconds"].as_f64().unwrap() <= observed
            && w["atomic"] == false,
        "Invalid observation window.",
    )?;
    let tick_value = ticks.unwrap().parse::<i128>().unwrap();
    let ms = (tick_value - 116444736000000000i128) / 10000;
    valid(
        ms > 0 && ms <= now as i128 && ms <= 9_007_199_254_740_991,
        "Invalid process creation time.",
    )?;
    let map = l["world"]["mapCoordinatesCompatible"] == true
        && l["world"]["originRebasingEnabled"] == false
        && ["x", "y", "z"]
            .iter()
            .all(|k| l["world"]["origin"][k].as_f64() == Some(0.0))
        && l["player"]["dead"] == false;
    let wave = live_wave_timing(
        &l["wave"],
        l["clocks"]["simulationSeconds"].as_f64().unwrap(),
    );
    let data = if tool == "get_live_player_context" {
        json!({"position":r["position"],"mapPosition":if map{r["position"].clone()}else{Value::Null},"player":l["player"],"world":l["world"]})
    } else {
        json!({"world":l["world"],"clocks":l["clocks"],"pause":l["pause"],"wave":wave})
    };
    let mut observation = json!({"observationId":uuid::Uuid::new_v4().to_string(),"source":"live_process_memory","observedAt":iso(observed.trunc() as i64),"window":w,"processId":process["pid"],"creationTicks":process["creation_ticks"],"processStartedAtUtc":iso(ms as i64),"sessionId":format!("{}:{}:{GAME_HASH}",crate::saves::js_json(&process["pid"]),ticks.unwrap()),"helperVersion":r["helperVersion"],"gameExecutableSha256":GAME_HASH});
    if let Some(v) = r.get("readerIsElevated") {
        observation["readerIsElevated"] = v.clone();
    }
    let mut result = json!({"status":"ok","observation":observation,"data":data,"coverage":{"saveSnapshotRefreshed":false,"atomic":false,"mainMapOnly":true,"characterPositionNotDrone":true,"mapPositionAvailable":map,"waveEta":"Per-field verification: active Heat timings use progress/rate with conditional elapsed-time estimates; actor deadline drift is diagnostic; waiting countdown is direct; interval/warning read live. Changed runtime settings, explosion cue and exact substage deadlines retain limits."}});
    if let Some(v) = l.get("evidence") {
        result["evidence"] = v.clone();
    }
    Ok(result)
}
pub fn process_response(
    tool: &str,
    stdout: &[u8],
    failed: bool,
    start: i64,
    now: i64,
) -> Result<Value> {
    valid(stdout.len() <= LIMIT, "Helper response exceeds limit.")?;
    let text = std::str::from_utf8(stdout)
        .map_err(|_| AppError::new("LIVE_READ_FAILED", "Helper response is not valid UTF-8."))?;
    let r: Value = serde_json::from_str(text.strip_prefix('\u{feff}').unwrap_or(text))
        .map_err(|_| AppError::new("LIVE_READ_FAILED", "Invalid helper JSON response."))?;
    valid(r.is_object(), "Invalid helper response.")?;
    if failed || r["status"] != "ok" {
        return Ok(unavailable(
            "LIVE_UNAVAILABLE",
            r["error"].as_str().unwrap_or(
                "The read-only helper could not obtain a validated current observation.",
            ),
        ));
    }
    validate_observation(tool, &r, start, now)
}
