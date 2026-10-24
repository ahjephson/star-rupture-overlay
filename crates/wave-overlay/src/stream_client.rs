//! SHA-pinned, overlay-owned persistent read-only child; bounded IPC and lifetime.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use starrupture_wave_core::{config::Config, live, saves::now_ms};
use starrupture_observation_model::overlay_stream::{
    Command, Kind, MAX_LINE, Message, Validator, read_line,
};
use std::{
    fs::File,
    io::{Read, Write},
    process::{Child, ChildStdin, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
pub enum Update {
    Reset(u32),
    Menu(bool, u64),
    World(Value),
    Unavailable(String),
}
struct OwnedChild {
    child: Child,
    input: Option<ChildStdin>,
    _pin: File,
    threads: Vec<thread::JoinHandle<()>>,
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if let Some(mut input) = self.input.take() {
            let _ = writeln!(input, "{}", serde_json::to_string(&Command::Stop).unwrap());
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => break,
                _ if Instant::now() >= deadline => {
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    break;
                }
                _ => thread::sleep(Duration::from_millis(10)),
            }
        }
        for worker in self.threads.drain(..) {
            let _ = worker.join();
        }
    }
}
pub fn run(
    config: Config,
    stop: Arc<AtomicBool>,
    mut publish: impl FnMut(Update),
) -> Result<(), String> {
    let cfg = config
        .live
        .as_ref()
        .filter(|v| v.enabled)
        .ok_or("Pinned overlay reader is disabled")?;
    use std::os::windows::{fs::OpenOptionsExt, process::CommandExt};
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&cfg.helper_file)
        .map_err(|e| e.to_string())?;
    if !cfg
        .helper_file
        .extension()
        .is_some_and(|v| v.eq_ignore_ascii_case("exe"))
        || file.metadata().map_err(|e| e.to_string())?.len() > 64 * 1024 * 1024
    {
        return Err("Invalid overlay reader executable".into());
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if format!("{:x}", Sha256::digest(&bytes)) != cfg.helper_sha256 {
        return Err("Persistent overlay reader SHA256 mismatch".into());
    }
    let cache = config
        .cache_directory
        .as_ref()
        .ok_or("Private overlay cache directory required")?;
    let mut command = std::process::Command::new(&cfg.helper_file);
    command
        .arg("--cache-dir")
        .arg(cache)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x08000000);
    let since = now_ms().max(0) as u64;
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let pid = child.id();
    let input = child.stdin.take();
    let output = child.stdout.take().unwrap();
    let mut errors = child.stderr.take().unwrap();
    let mut owner = OwnedChild {
        child,
        input,
        _pin: file,
        threads: Vec::new(),
    };
    let (tx, rx) = mpsc::sync_channel::<Result<Message, String>>(8);
    owner.threads.push(thread::spawn(move || {
        let mut reader = std::io::BufReader::new(output);
        loop {
            let message = match read_line(&mut reader, MAX_LINE) {
                Ok(None) => break,
                Ok(Some(line)) => serde_json::from_slice::<Message>(&line)
                    .map_err(|e| format!("Invalid reader message: {e}")),
                Err(e) => Err(e.to_string()),
            };
            let failed = message.is_err();
            if tx.send(message).is_err() || failed {
                break;
            }
        }
    }));
    owner.threads.push(thread::spawn(move || {
        let mut buffer = [0; 4096];
        while let Ok(n) = errors.read(&mut buffer) {
            if n == 0 {
                break;
            }
        }
    }));
    let result = (|| {
        writeln!(
            owner.input.as_mut().unwrap(),
            "{}",
            serde_json::to_string(&Command::Start {
                version: 1,
                pause_interval_ms: 50,
                world_interval_ms: 250
            })
            .unwrap()
        )
        .map_err(|e| e.to_string())?;
        let mut validator = Validator::new(pid, since);
        let mut last = Instant::now();
        let mut stale = false;
        let mut world_reads = 0u64;
        let mut menu_updates = 0u64;
        let mut statistics = Value::Null;
        while !stop.load(Ordering::Relaxed) {
            let message = match rx.recv_timeout(Duration::from_millis(25)) {
                Ok(value) => value?,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(
                        "Persistent overlay reader exited; no fallback or automatic respawn".into(),
                    );
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if last.elapsed() > Duration::from_secs(2) && !stale {
                        publish(Update::Unavailable(
                            "Persistent reader is not responding".into(),
                        ));
                        stale = true;
                    }
                    if last.elapsed() > Duration::from_secs(65) {
                        return Err("Persistent reader timeout".into());
                    }
                    continue;
                }
            };
            let now = now_ms().max(0) as u64;
            validator.accept(&message, now).map_err(str::to_owned)?;
            last = Instant::now();
            stale = false;
            match message.kind {
                Kind::Ready => {
                    let process = message.process.as_ref().unwrap();
                    eprintln!(
                        "Persistent overlay reader PID{pid}, generation{}",
                        message.generation
                    );
                    publish(Update::Reset(process.pid));
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
                    menu_updates += 1;
                    publish(Update::Menu(menu, end));
                }
                Kind::World => {
                    let raw = &message.data["raw"];
                    let process = message.process.as_ref().unwrap();
                    if raw["process"]["pid"] != process.pid
                        || raw["process"]["creation_ticks"] != process.creation_ticks
                        || raw["process"]["executable_sha256"] != process.executable_sha256
                    {
                        return Err("World message identity mismatch".into());
                    }
                    let start = message.data["readStartedUnixMs"]
                        .as_i64()
                        .ok_or("Missing world start")?;
                    let bytes = serde_json::to_vec(raw).map_err(|e| e.to_string())?;
                    let mut value = live::process_response(
                        "get_live_world_state",
                        &bytes,
                        false,
                        start,
                        now as i64,
                    )
                    .map_err(|e| e.to_string())?;
                    value["observation"]["overlaySession"] = json!([pid, message.generation]);
                    statistics = message.data["statistics"].clone();
                    world_reads += 1;
                    if world_reads == 1 {
                        eprintln!(
                            "Wave timer: first validated live observation received from persistent reader."
                        );
                    }
                    publish(Update::World(value));
                }
                Kind::Unavailable => publish(Update::Unavailable(
                    message.data["reason"]
                        .as_str()
                        .unwrap_or("Live session unavailable")
                        .chars()
                        .take(240)
                        .collect(),
                )),
            }
        }
        eprintln!(
            "Persistent overlay reader stopping: {world_reads} validated world observations, {menu_updates} menu updates; last statistics: {statistics}"
        );
        Ok(())
    })();
    // Unblock a bounded output pump before joining it during child cleanup.
    drop(rx);
    drop(owner);
    result
}
