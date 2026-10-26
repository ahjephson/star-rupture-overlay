//! Bounded observation envelopes, process identities and sequence validation.
use serde::{Deserialize, Serialize};
pub const MAX_OBSERVATION_BYTES: usize = 65536;
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameIdentity {
    pub pid: u32,
    pub creation_ticks: u64,
    pub executable_sha256: &'static str,
}
#[derive(Clone, Copy, Debug)]
pub struct MenuSample {
    pub menu_open: bool,
    pub started_ms: u64,
    pub finished_ms: u64,
    pub guards_validated: bool,
}
#[derive(Clone, Copy, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Statistics {
    pub menu_reads: u64,
    pub world_reads: u64,
    pub sessions: u64,
    pub world_read_millis: u64,
    pub menu_max_micros: u64,
}
#[derive(Debug)]
pub struct WorldSample {
    pub read_started_unix_ms: i64,
    pub raw: crate::observation::RawObservation,
    pub statistics: Statistics,
}
#[derive(Debug)]
pub enum Payload {
    Ready,
    Menu(MenuSample),
    World(Box<WorldSample>),
    Unavailable(String),
}
#[derive(Debug)]
pub struct Message {
    pub version: u32,
    pub reader_pid: u32,
    pub sequence: u64,
    pub generation: u64,
    pub sampled_at_unix_ms: u64,
    pub process: Option<GameIdentity>,
    pub read_only: bool,
    pub game_code_invoked: bool,
    pub payload: Payload,
}
impl Message {
    /// Bounds retained payload storage without serializing observations.
    pub fn within_bound(&self) -> bool {
        let heap = match &self.payload {
            Payload::World(sample) => sample
                .raw
                .live
                .heap_bytes()
                .and_then(|n| n.checked_add(std::mem::size_of::<WorldSample>())),
            Payload::Unavailable(reason) => Some(reason.capacity()),
            _ => Some(0),
        };
        heap.and_then(|n| n.checked_add(std::mem::size_of::<Self>()))
            .is_some_and(|n| n < MAX_OBSERVATION_BYTES)
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
        if !m.within_bound()
            || m.version != 1
            || m.reader_pid != self.pid
            || !m.read_only
            || m.game_code_invoked
            || self.sequence.checked_add(1) != Some(m.sequence)
            || m.sampled_at_unix_ms < self.since.saturating_sub(1000)
            || m.sampled_at_unix_ms > now.saturating_add(1000)
            || now.saturating_sub(m.sampled_at_unix_ms) > 2000
        {
            return Err("Invalid/stale stream envelope");
        }
        if matches!(m.payload, Payload::Ready) {
            let p = m.process.as_ref().ok_or("Missing process identity")?;
            if p.pid == 0
                || p.executable_sha256 != GAME_HASH
                || p.creation_ticks == 0
                || m.generation <= self.generation
            {
                return Err("Invalid process/generation");
            }
            self.generation = m.generation;
            self.process = Some(*p);
        } else if matches!(m.payload, Payload::Unavailable(_)) {
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
    fn message(kind: bool, sequence: u64, generation: u64) -> Message {
        Message {
            version: 1,
            reader_pid: 4,
            sequence,
            generation,
            sampled_at_unix_ms: 1000,
            process: Some(GameIdentity {
                pid: 7,
                creation_ticks: 123,
                executable_sha256: GAME_HASH,
            }),
            read_only: true,
            game_code_invoked: false,
            payload: if kind {
                Payload::Ready
            } else {
                Payload::Menu(MenuSample {
                    menu_open: true,
                    started_ms: 1000,
                    finished_ms: 1000,
                    guards_validated: true,
                })
            },
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
        let oversized = Message {
            payload: Payload::Unavailable("x".repeat(MAX_OBSERVATION_BYTES)),
            ..message(false, 1, 1)
        };
        assert!(!oversized.within_bound());
    }
    #[test]
    fn generations_sequence_identity_and_invalidation_are_enforced() {
        let mut v = Validator::new(4, 1000);
        assert!(v.accept(&message(false, 1, 1), 1000).is_err());
        assert!(v.accept(&message(true, 1, 1), 1000).is_ok());
        assert!(v.accept(&message(false, 2, 1), 1000).is_ok());
        assert!(v.accept(&message(false, 2, 1), 1000).is_err());
        assert!(
            v.accept(
                &Message {
                    payload: Payload::Unavailable(String::new()),
                    ..message(false, 3, 1)
                },
                1000
            )
            .is_ok()
        );
        assert!(v.accept(&message(false, 4, 1), 1000).is_err());
        assert!(v.accept(&message(true, 4, 2), 1000).is_ok());
        let mut bad = message(false, 5, 2);
        bad.process.as_mut().unwrap().creation_ticks = 124;
        assert!(v.accept(&bad, 1000).is_err());
        bad = message(false, 5, 2);
        bad.game_code_invoked = true;
        assert!(v.accept(&bad, 1000).is_err());
        assert!(v.accept(&message(false, 5, 2), 5000).is_err());
        assert!(v.accept(&message(false, 5, 2), 1000).is_ok());
    }
}
