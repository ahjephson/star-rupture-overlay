//! Embedded wave reader and validated observations, in the GUI's worker thread.
use serde::Serialize;
use starrupture_observation_model::observation::{Observation, OverlaySession};
use starrupture_observation_model::overlay_stream::{Payload, Statistics, Validator};
use starrupture_observation_model::shutdown::StopSignal;
use starrupture_wave_core::{config::Config, formatting::now_ms, live};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunStatistics {
    world_reads: u64,
    menu_updates: u64,
    last_statistics: Option<Statistics>,
    reader_mode: &'static str,
    child_processes: u32,
}
pub enum Update {
    Reset(u32),
    Menu(bool, u64),
    World(Box<Observation>),
    Unavailable(String),
}
pub fn run(
    config: Config,
    stop: Arc<StopSignal>,
    mut publish: impl FnMut(Update),
) -> Result<RunStatistics, String> {
    let cache = config.cache_directory.as_ref().ok_or("Private cache directory required")?;
    let mut sink = ObservationSink {
        validator: Validator::new(std::process::id(), now_ms().max(0) as u64),
        world_reads: 0,
        menu_updates: 0,
        statistics: None,
        publish: &mut publish,
    };
    starrupture_overlay_reader::overlay_stream::run(cache, stop, |message| {
        sink.accept(message).map_err(anyhow::Error::msg)
    })
    .map_err(|error| format!("{error:#}"))?;
    Ok(RunStatistics {
        world_reads: sink.world_reads,
        menu_updates: sink.menu_updates,
        last_statistics: sink.statistics,
        reader_mode: "embedded_worker",
        child_processes: 0,
    })
}

/// Keeps fast layout updates separate from independently validated timer evidence.
struct ObservationSink<F> {
    validator: Validator,
    world_reads: u64,
    menu_updates: u64,
    statistics: Option<Statistics>,
    publish: F,
}

impl<F: FnMut(Update)> ObservationSink<F> {
    fn accept(
        &mut self,
        message: starrupture_observation_model::overlay_stream::Message,
    ) -> Result<(), String> {
        let now = now_ms().max(0) as u64;
        self.validator.accept(&message, now).map_err(str::to_owned)?;
        match message.payload {
            Payload::Ready => {
                let process = message.process.ok_or("Missing validated process identity")?;
                eprintln!(
                    "Embedded overlay reader PID{}, generation{}",
                    std::process::id(),
                    message.generation
                );
                (self.publish)(Update::Reset(process.pid));
            }
            Payload::Menu(data) => {
                if !data.guards_validated
                    || data.started_ms > data.finished_ms
                    || data.finished_ms > message.sampled_at_unix_ms
                    || data.finished_ms - data.started_ms > 250
                {
                    return Err("Invalid menu observation window/guards".into());
                }
                self.menu_updates += 1;
                (self.publish)(Update::Menu(data.menu_open, data.finished_ms));
            }
            Payload::World(sample) => {
                if Some(sample.raw.process) != message.process {
                    return Err("World message identity mismatch".into());
                }
                let mut value =
                    live::validate_observation(sample.raw, sample.read_started_unix_ms, now as i64)
                        .map_err(|e| e.to_string())?;
                value.overlay_session = OverlaySession {
                    reader_pid: std::process::id(),
                    generation: message.generation,
                };
                self.statistics = Some(sample.statistics);
                self.world_reads += 1;
                if self.world_reads == 1 {
                    eprintln!(
                        "Wave timer: first validated live observation received from embedded reader."
                    );
                }
                (self.publish)(Update::World(Box::new(value)));
            }
            Payload::Unavailable(reason) => {
                (self.publish)(Update::Unavailable(reason.chars().take(240).collect()))
            }
        }

        Ok(())
    }
}

pub fn self_test(dir: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(dir)?;
    let stop = Arc::new(StopSignal::default());
    let done = Arc::new(AtomicBool::new(false));
    let deadline_stop = stop.clone();
    let deadline_done = done.clone();
    let timer = std::thread::spawn(move || {
        let start = Instant::now();
        while !deadline_done.load(Ordering::Relaxed) {
            if start.elapsed() > Duration::from_secs(60) {
                deadline_stop.request();
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
                stop.request();
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

#[cfg(test)]
mod tests {
    use super::*;
    use starrupture_observation_model::{
        READER_CONTRACT_VERSION,
        observation::*,
        overlay_stream::{GAME_HASH, GameIdentity, MenuSample, Message, WorldSample},
    };
    fn identity() -> GameIdentity {
        GameIdentity {
            pid: 123,
            creation_ticks: 134000000000000000,
            executable_sha256: GAME_HASH,
        }
    }
    fn message(payload: Payload, sequence: u64, now: u64) -> Message {
        Message {
            version: 1,
            reader_pid: std::process::id(),
            sequence,
            generation: 1,
            sampled_at_unix_ms: now,
            process: Some(identity()),
            read_only: true,
            game_code_invoked: false,
            payload,
        }
    }
    fn world(now: u64) -> WorldSample {
        WorldSample {
            read_started_unix_ms: now as i64,
            statistics: Statistics::default(),
            raw: RawObservation {
                schema_version: 1,
                contract_version: READER_CONTRACT_VERSION,
                source_is_live_memory: true,
                game_memory_writes: false,
                process: identity(),
                sample: PlayerSample {
                    position: Position::default(),
                    observed_at_ms: now,
                    reader_is_elevated: false,
                },
                live: LiveContext {
                    player: PlayerContext {
                        dead: false,
                        view_mode: ViewMode::OnFoot,
                        main_character_name: String::new(),
                        main_character_path: String::new(),
                        controller_path: String::new(),
                        first_person_active: true,
                        drone_active: false,
                        drone_present: false,
                    },
                    world: WorldState {
                        world_path: WORLD_PATH.into(),
                        ..WorldState::default()
                    },
                    clocks: Clocks::default(),
                    pause: PauseState::default(),
                    wave: WaveObservation::default(),
                    window: ReadWindow {
                        started_ms: now,
                        completed_ms: now,
                        atomic: false,
                    },
                },
            },
        }
    }
    #[test]
    fn menu_updates_and_invalidation_cannot_refresh_world_evidence() {
        let now = now_ms().max(0) as u64;
        let mut events = Vec::new();
        let mut sink = ObservationSink {
            validator: Validator::new(std::process::id(), now),
            world_reads: 0,
            menu_updates: 0,
            statistics: None,
            publish: |event| events.push(event),
        };
        sink.accept(message(Payload::Ready, 1, now)).unwrap();
        sink.accept(message(Payload::World(Box::new(world(now))), 2, now)).unwrap();
        sink.accept(message(
            Payload::Menu(MenuSample {
                menu_open: true,
                started_ms: now,
                finished_ms: now,
                guards_validated: true,
            }),
            3,
            now,
        ))
        .unwrap();
        sink.accept(message(Payload::Unavailable("Unavailable".into()), 4, now)).unwrap();
        assert_eq!((sink.world_reads, sink.menu_updates), (1, 1));
        assert!(sink.accept(message(Payload::World(Box::new(world(now))), 5, now)).is_err());
        assert_eq!(events.len(), 4);
        assert!(matches!(events[0], Update::Reset(123)));
        let Update::World(value) = &events[1] else {
            panic!("Expected world observation")
        };
        assert_eq!(value.window.completed_ms, now);
        assert_eq!(value.overlay_session.generation, 1);
        assert!(matches!(events[2], Update::Menu(true, at) if at == now));
        assert!(matches!(&events[3], Update::Unavailable(reason) if reason == "Unavailable"));
    }
    #[test]
    fn invalid_menu_guards_windows_and_world_identity_are_rejected() {
        for case in 0..5 {
            let now = now_ms().max(0) as u64;
            let mut published = 0;
            let mut sink = ObservationSink {
                validator: Validator::new(std::process::id(), now),
                world_reads: 0,
                menu_updates: 0,
                statistics: None,
                publish: |_| published += 1,
            };
            sink.accept(message(Payload::Ready, 1, now)).unwrap();
            let mut menu = MenuSample {
                menu_open: true,
                started_ms: now,
                finished_ms: now,
                guards_validated: true,
            };
            let payload = match case {
                0 => {
                    menu.guards_validated = false;
                    Payload::Menu(menu)
                }
                1 => {
                    menu.started_ms = now + 1;
                    Payload::Menu(menu)
                }
                2 => {
                    menu.finished_ms = now + 1;
                    Payload::Menu(menu)
                }
                3 => {
                    menu.started_ms = now - 251;
                    Payload::Menu(menu)
                }
                4 => {
                    let mut sample = world(now);
                    sample.raw.process.creation_ticks += 1;
                    Payload::World(Box::new(sample))
                }
                _ => unreachable!(),
            };
            assert!(sink.accept(message(payload, 2, now)).is_err(), "case {case}");
            assert_eq!((sink.world_reads, sink.menu_updates), (0, 0));
            assert_eq!(published, 1);
        }
    }
}
