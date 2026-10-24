//! Private bounded overlay IPC; not an MCP contract.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, BufRead};
pub const MAX_LINE: usize = 65536;
pub const GAME_HASH: &str = "95f95eee0b1913b8716de7fa5bf19877a91abe86e63bf2b6f0a29bd48d1d76f1";
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Start {
        version: u32,
        pause_interval_ms: u32,
        world_interval_ms: u32,
    },
    Stop,
}
impl Command {
    pub fn intervals(&self) -> Result<(u64, u64), &'static str> {
        match self {
            Self::Start {
                version: 1,
                pause_interval_ms,
                world_interval_ms,
            } if (25..=100).contains(pause_interval_ms)
                && (250..=1000).contains(world_interval_ms) =>
            {
                Ok((u64::from(*pause_interval_ms), u64::from(*world_interval_ms)))
            }
            _ => Err("Invalid stream start or cadence bounds"),
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GameIdentity {
    pub pid: u32,
    pub creation_ticks: String,
    pub executable_sha256: String,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Ready,
    Pause,
    World,
    Unavailable,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Message {
    pub version: u32,
    pub reader_pid: u32,
    pub sequence: u64,
    pub generation: u64,
    pub kind: Kind,
    pub sampled_at_unix_ms: u64,
    pub process: Option<GameIdentity>,
    pub read_only: bool,
    pub game_code_invoked: bool,
    pub data: Value,
}
pub fn read_line(reader: &mut impl BufRead, max: usize) -> io::Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    loop {
        let chunk = reader.fill_buf()?;
        if chunk.is_empty() {
            return if line.is_empty() {
                Ok(None)
            } else {
                Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Truncated IPC line"))
            };
        }
        let n = chunk.iter().position(|b| *b == b'\n').map_or(chunk.len(), |n| n + 1);
        if line.len() + n > max {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "IPC line exceeds bound"));
        }
        line.extend_from_slice(&chunk[..n]);
        reader.consume(n);
        if line.last() == Some(&b'\n') {
            return Ok(Some(line));
        }
    }
}
pub struct Validator {
    pid: u32,
    sequence: u64,
    generation: u64,
    process: Option<GameIdentity>,
    since: u64,
}
impl Validator {
    pub fn new(pid: u32, since: u64) -> Self {
        Self {
            pid,
            sequence: 0,
            generation: 0,
            process: None,
            since,
        }
    }
    pub fn accept(&mut self, m: &Message, now: u64) -> Result<(), &'static str> {
        if m.version != 1
            || m.reader_pid != self.pid
            || !m.read_only
            || m.game_code_invoked
            || self.sequence.checked_add(1) != Some(m.sequence)
            || m.sampled_at_unix_ms < self.since.saturating_sub(1000)
            || m.sampled_at_unix_ms > now + 1000
            || now.saturating_sub(m.sampled_at_unix_ms) > 2000
        {
            return Err("Invalid/stale stream envelope");
        }
        if m.kind == Kind::Ready {
            let p = m.process.as_ref().ok_or("Missing process identity")?;
            if p.pid == 0
                || p.executable_sha256 != GAME_HASH
                || p.creation_ticks.is_empty()
                || p.creation_ticks.len() > 20
                || !p.creation_ticks.bytes().all(|b| b.is_ascii_digit())
                || m.generation <= self.generation
            {
                return Err("Invalid process/generation");
            }
            self.generation = m.generation;
            self.process = Some(p.clone());
        } else if m.kind == Kind::Unavailable {
            if m.generation != self.generation {
                return Err("Invalid invalidation generation");
            }
            self.process = None;
        } else if m.generation != self.generation
            || self.process.is_none()
            || m.process != self.process
        {
            return Err("Stale/mismatched generation or process");
        }
        self.sequence = m.sequence;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn message(kind: Kind, sequence: u64, generation: u64) -> Message {
        Message {
            version: 1,
            reader_pid: 4,
            sequence,
            generation,
            kind,
            sampled_at_unix_ms: 1000,
            process: Some(GameIdentity {
                pid: 7,
                creation_ticks: "123".into(),
                executable_sha256: GAME_HASH.into(),
            }),
            read_only: true,
            game_code_invoked: false,
            data: json!({}),
        }
    }
    #[test]
    fn stream_bounds_and_commands_reject_unsafe_input() {
        assert!(
            Command::Start {
                version: 1,
                pause_interval_ms: 50,
                world_interval_ms: 250
            }
            .intervals()
            .is_ok()
        );
        assert!(
            Command::Start {
                version: 1,
                pause_interval_ms: 1,
                world_interval_ms: 250
            }
            .intervals()
            .is_err()
        );
        assert!(serde_json::from_str::<Command>(r#"{"command":"start","version":1,"pause_interval_ms":50,"world_interval_ms":250,"address":1}"#).is_err());
        assert!(read_line(&mut io::Cursor::new(b"12345\n"), 4).is_err());
        assert!(read_line(&mut io::Cursor::new(b"123"), 4).is_err());
        assert!(read_line(&mut io::Cursor::new(b""), 4).unwrap().is_none());
    }
    #[test]
    fn generations_sequence_identity_and_invalidation_are_enforced() {
        let mut v = Validator::new(4, 1000);
        assert!(v.accept(&message(Kind::Pause, 1, 1), 1000).is_err());
        assert!(v.accept(&message(Kind::Ready, 1, 1), 1000).is_ok());
        assert!(v.accept(&message(Kind::Pause, 2, 1), 1000).is_ok());
        assert!(v.accept(&message(Kind::Pause, 2, 1), 1000).is_err());
        assert!(v.accept(&message(Kind::Unavailable, 3, 1), 1000).is_ok());
        assert!(v.accept(&message(Kind::World, 4, 1), 1000).is_err());
        assert!(v.accept(&message(Kind::Ready, 4, 2), 1000).is_ok());
        let mut bad = message(Kind::Pause, 5, 2);
        bad.process.as_mut().unwrap().creation_ticks = "124".into();
        assert!(v.accept(&bad, 1000).is_err());
        bad = message(Kind::Pause, 5, 2);
        bad.game_code_invoked = true;
        assert!(v.accept(&bad, 1000).is_err());
        assert!(v.accept(&message(Kind::Pause, 5, 2), 5000).is_err());
        assert!(v.accept(&message(Kind::Pause, 5, 2), 1000).is_ok());
    }
}
