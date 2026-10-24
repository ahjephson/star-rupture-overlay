//! Embedded read-only engine on the overlay worker thread; no child or IPC.
use super::*;
use serde_json::{Value, json};
use starrupture_observation_model::overlay_stream::{
    Command, GameIdentity, Kind, MAX_LINE, Message,
};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}
struct Output<'a> {
    sink: &'a mut dyn FnMut(Message) -> Result<()>,
    sequence: u64,
    generation: u64,
    process: Option<GameIdentity>,
}
impl Output<'_> {
    fn send(&mut self, kind: Kind, data: Value) -> Result<()> {
        self.sequence = self.sequence.checked_add(1).context("Stream sequence exhausted")?;
        let message = Message {
            version: 1,
            reader_pid: std::process::id(),
            sequence: self.sequence,
            generation: self.generation,
            kind,
            sampled_at_unix_ms: now(),
            process: self.process.clone(),
            read_only: true,
            game_code_invoked: false,
            data,
        };
        let bytes = serde_json::to_vec(&message)?;
        ensure!(bytes.len() < MAX_LINE, "Stream message bound");
        (self.sink)(message)?;
        Ok(())
    }
}
struct Session {
    process: platform::Process,
    base: Cache,
    live: live_context::resident::Resident,
}
async fn attach(
    directory: &Path,
    retained: &mut Option<platform::Process>,
    budget: Arc<platform::ReadBudget>,
) -> Result<Session> {
    if retained.as_ref().is_some_and(|p| p.ensure_alive().is_err()) {
        *retained = None;
    }
    if retained.is_none() {
        *retained = Some(platform::find_game(budget)?);
    }
    let process = retained.as_ref().unwrap().clone();
    ensure!(process.identity.executable_sha256 == BUILD_HASH, "Unsupported game executable");
    let path = directory.join("session.json");
    let cached = crate::cache::read_json::<Cache>(&path, 100000)
        .ok()
        .filter(|c| cache_matches(c, &process.identity));
    let base = if let Some(c) = cached {
        if sample(&process, &c).await.is_ok() {
            c
        } else {
            discover(&process).await?
        }
    } else {
        discover(&process).await?
    };
    sample(&process, &base).await?;
    let temporary = directory.join(format!("resident-session-{}.tmp", std::process::id()));
    std::fs::write(&temporary, serde_json::to_vec(&base)?)?;
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    std::fs::rename(temporary, &path)?;
    let live = live_context::resident::Resident::open(&process, &base, directory).await?;
    Ok(Session {
        process,
        base,
        live,
    })
}
pub fn run(
    directory: &Path,
    stop: Arc<AtomicBool>,
    mut sink: impl FnMut(Message) -> Result<()>,
) -> Result<()> {
    ensure!(!platform::is_elevated()?, "Do not run overlay elevated");
    ensure!(directory.is_absolute(), "Absolute private cache directory required");
    let (pause_ms, world_ms) = Command::Start {
        version: 1,
        pause_interval_ms: 50,
        world_interval_ms: 250,
    }
    .intervals()
    .map_err(anyhow::Error::msg)?;
    let budget = platform::ReadBudget::new(stop.clone());
    std::fs::create_dir_all(directory)?;
    jmap_dumper::diag::set_verbosity(jmap_dumper::diag::Verbosity::Quiet);
    let mut output = Output {
        sink: &mut sink,
        sequence: 0,
        generation: 0,
        process: None,
    };
    let mut retained = None;
    let mut sessions = 0u64;
    let mut menu_reads = 0u64;
    let mut world_reads = 0u64;
    let mut menu_max_micros = 0u128;
    smol::block_on(async {
        let mut session: Option<Session> = None;
        let mut retry = Instant::now();
        let mut failures = 0u32;
        let mut pause_due = Instant::now();
        let mut world_due = Instant::now();
        let mut last_menu = None;
        let mut menu_sent = Instant::now();
        while !stop.load(Ordering::Relaxed) {
            budget.renew(5000);
            if session.is_none() {
                if Instant::now() < retry {
                    smol::Timer::after(Duration::from_millis(10)).await;
                    continue;
                }
                budget.renew(60000);
                match attach(directory, &mut retained, budget.clone()).await {
                    Ok(s) => {
                        output.generation =
                            output.generation.checked_add(1).context("Generation exhausted")?;
                        output.process = Some(GameIdentity {
                            pid: s.process.identity.pid,
                            creation_ticks: s.process.identity.creation_ticks.to_string(),
                            executable_sha256: s.process.identity.executable_sha256.clone(),
                        });
                        output.send(
                            Kind::Ready,
                            json!({"pauseIntervalMs":pause_ms,"worldIntervalMs":world_ms}),
                        )?;
                        sessions += 1;
                        failures = 0;
                        session = Some(s);
                        last_menu = None;
                        pause_due = Instant::now();
                        world_due = Instant::now();
                    }
                    Err(e) => {
                        output.send(
                            Kind::Unavailable,
                            json!({"reason":format!("{e:#}"),"savedFallback":false}),
                        )?;
                        failures = (failures + 1).min(3);
                        retry = Instant::now() + Duration::from_secs(1 << failures);
                        continue;
                    }
                }
            }
            budget.renew(5000);
            let s = session.as_mut().unwrap();
            let result:Result<()>=async {
                if Instant::now()>=pause_due {
                    let started=now();let clock=Instant::now();
                    if let Some(menu)=s.live.menu(&s.process,&s.base).await? {
                        menu_reads+=1;
                        if last_menu!=Some(menu) || menu_sent.elapsed()>=Duration::from_millis(250) {
                            output.send(Kind::Pause,json!({"menuOpen":menu,"startedUnixMs":started,"finishedUnixMs":now(),"guardsValidated":true}))?;
                            last_menu=Some(menu);menu_sent=Instant::now();
                        }
                    }
                    menu_max_micros=menu_max_micros.max(clock.elapsed().as_micros());
                    pause_due=Instant::now()+Duration::from_millis(pause_ms);
                }
                if Instant::now()>=world_due {
                    let cycle=Instant::now();let started=now();
                    sample_context(&s.process,&s.base,s.live.context()).await?;
                    let live=s.live.snapshot(&s.process,&s.base).await?;
                    let mut raw=sample_context(&s.process,&s.base,s.live.context()).await?;
                    raw["live"]=live;raw["process"]["creation_ticks"]=json!(s.process.identity.creation_ticks.to_string());
                    raw["observedAtUnixMilliseconds"]=json!(now());
                    output.send(Kind::World,json!({"readStartedUnixMs":started,"raw":raw,"statistics":{"menuReads":menu_reads,"worldReads":world_reads+1,"sessions":sessions,"worldReadMillis":cycle.elapsed().as_millis(),"menuMaxMicros":menu_max_micros}}))?;
                    world_reads+=1;
                    world_due=cycle+Duration::from_millis(world_ms);
                }
                Ok(())
            }.await;
            if let Err(e) = result {
                let reason = e.to_string();
                if reason == "Wave phase changed during observation"
                    || reason == "Phase changed while sampling"
                    || reason == "Timer configuration changed during observation"
                {
                    // Discard a torn wave read, preserving no new timestamp. The
                    // retained world/allocation guards are rechecked next cycle.
                    world_due = Instant::now() + Duration::from_millis(50);
                    continue;
                }
                output.send(
                    Kind::Unavailable,
                    json!({"reason":format!("{e:#}"),"savedFallback":false}),
                )?;
                session = None;
                last_menu = None;
                retry = Instant::now() + Duration::from_secs(1);
            }
            smol::Timer::after(Duration::from_millis(1)).await;
        }
        Ok::<(), anyhow::Error>(())
    })?;
    eprintln!(
        "Overlay reader stopped: {sessions} sessions, {menu_reads} menu reads, {world_reads} world reads"
    );
    Ok(())
}
