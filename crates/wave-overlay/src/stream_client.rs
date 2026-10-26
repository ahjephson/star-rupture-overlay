//! Embedded wave reader and validated observations, in the GUI's worker thread.
use serde_json::{Value, json};
use starrupture_observation_model::overlay_stream::{Kind, Validator};
use starrupture_wave_core::{config::Config, formatting::now_ms, live};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
pub enum Update {
    Reset(u32),
    Menu(bool, u64),
    World(Value),
    Unavailable(String),
}
pub fn run(
    config: Config,
    stop: Arc<AtomicBool>,
    mut publish: impl FnMut(Update),
) -> Result<Value, String> {
    let cache = config.cache_directory.as_ref().ok_or("Private cache directory required")?;
    let mut sink = ObservationSink {
        validator: Validator::new(std::process::id(), now_ms().max(0) as u64),
        world_reads: 0,
        menu_updates: 0,
        statistics: Value::Null,
        publish: &mut publish,
    };
    starrupture_overlay_reader::overlay_stream::run(cache, stop, |message| {
        sink.accept(message).map_err(anyhow::Error::msg)
    })
    .map_err(|error| format!("{error:#}"))?;
    Ok(json!({
        "worldReads": sink.world_reads,
        "menuUpdates": sink.menu_updates,
        "lastStatistics": sink.statistics,
        "readerMode": "embedded_worker",
        "childProcesses": 0,
    }))
}

/// Keeps fast layout updates separate from independently validated timer evidence.
struct ObservationSink<F> {
    validator: Validator,
    world_reads: u64,
    menu_updates: u64,
    statistics: Value,
    publish: F,
}

impl<F: FnMut(Update)> ObservationSink<F> {
    fn accept(
        &mut self,
        message: starrupture_observation_model::overlay_stream::Message,
    ) -> Result<(), String> {
        let now = now_ms().max(0) as u64;
        self.validator.accept(&message, now).map_err(str::to_owned)?;
        match message.kind {
            Kind::Ready => {
                let process =
                    message.process.as_ref().ok_or("Missing validated process identity")?;
                eprintln!(
                    "Embedded overlay reader PID{}, generation{}",
                    std::process::id(),
                    message.generation
                );
                (self.publish)(Update::Reset(process.pid));
            }
            Kind::Pause => {
                let data = &message.data;
                let menu = data["menuOpen"].as_bool().ok_or("Invalid menu value")?;
                let begin = data["startedUnixMs"].as_u64().ok_or("Missing menu start")?;
                let end = data["finishedUnixMs"].as_u64().ok_or("Missing menu end")?;
                if data["guardsValidated"] != true
                    || begin > end
                    || end > message.sampled_at_unix_ms
                    || end - begin > 250
                {
                    return Err("Invalid menu observation window/guards".into());
                }
                self.menu_updates += 1;
                (self.publish)(Update::Menu(menu, end));
            }
            Kind::World => {
                let raw = &message.data["raw"];
                let process =
                    message.process.as_ref().ok_or("Missing validated process identity")?;
                if raw["process"]["pid"] != process.pid
                    || raw["process"]["creation_ticks"] != process.creation_ticks
                    || raw["process"]["executable_sha256"] != process.executable_sha256
                {
                    return Err("World message identity mismatch".into());
                }
                let start =
                    message.data["readStartedUnixMs"].as_i64().ok_or("Missing world start")?;
                let bytes = serde_json::to_vec(raw).map_err(|e| e.to_string())?;
                let mut value = live::process_response(
                    "get_live_world_state",
                    &bytes,
                    false,
                    start,
                    now as i64,
                )
                .map_err(|e| e.to_string())?;
                value["observation"]["overlaySession"] =
                    json!([std::process::id(), message.generation]);
                self.statistics = message.data["statistics"].clone();
                self.world_reads += 1;
                if self.world_reads == 1 {
                    eprintln!(
                        "Wave timer: first validated live observation received from embedded reader."
                    );
                }
                (self.publish)(Update::World(value));
            }
            Kind::Unavailable => (self.publish)(Update::Unavailable(
                message.data["reason"]
                    .as_str()
                    .unwrap_or("Live session unavailable")
                    .chars()
                    .take(240)
                    .collect(),
            )),
        }

        Ok(())
    }
}

pub fn self_test(dir: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(dir)?;
    let stop = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicBool::new(false));
    let deadline_stop = stop.clone();
    let deadline_done = done.clone();
    let timer = std::thread::spawn(move || {
        let start = Instant::now();
        while !deadline_done.load(Ordering::Relaxed) {
            if start.elapsed() > Duration::from_secs(60) {
                deadline_stop.store(true, Ordering::Relaxed);
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    });
    let config = Config {
        cache_directory: Some(dir.join("cache")),
        ..Config::default()
    };
    let mut worlds = 0;
    let result = run(config, stop.clone(), |event| {
        if matches!(event, Update::World(_)) {
            worlds += 1;
            if worlds >= 24 {
                stop.store(true, Ordering::Relaxed);
            }
        }
    });
    done.store(true, Ordering::Relaxed);
    timer.join().map_err(|_| "Deadline thread panic")?;
    let value = result?;
    std::fs::write(dir.join("reader-validation.json"), serde_json::to_vec_pretty(&value)?)?;
    if worlds < 24 {
        return Err("No sufficient validated live observations".into());
    }
    Ok(())
}
