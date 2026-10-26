//! Connection presentation and confirmed-session exit accounting.
pub use starrupture_overlay_reader::overlay_stream::GameStatus;
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct Launch {
    requested: bool,
    deadline: Option<Instant>,
}
#[derive(Debug, PartialEq, Eq)]
pub enum LaunchAction {
    None,
    OpenSteam,
    TimedOut,
}
impl Launch {
    pub fn request(&mut self) {
        if !self.pending() {
            self.requested = true;
        }
    }
    pub fn pending(&self) -> bool {
        self.deadline.is_some()
    }
    pub fn failed(&mut self) {
        self.requested = false;
        self.deadline = None;
    }
    pub fn advance(&mut self, connection: &Connection, now: Instant) -> LaunchAction {
        if connection.running() {
            self.failed();
        }
        if self.deadline.is_some_and(|deadline| now >= deadline) {
            self.failed();
            return LaunchAction::TimedOut;
        }
        if self.requested && connection.can_launch() && !self.pending() {
            self.requested = false;
            self.deadline = Some(now + Duration::from_secs(120));
            return LaunchAction::OpenSteam;
        }
        LaunchAction::None
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connection {
    pub status: GameStatus,
    pub connected: bool,
    pub closed_sessions: u64,
    attached: Option<u32>,
}
impl Default for Connection {
    fn default() -> Self {
        Self {
            status: GameStatus::Unknown,
            connected: false,
            closed_sessions: 0,
            attached: None,
        }
    }
}
impl Connection {
    pub fn indicator_colour(&self) -> [u8; 3] {
        match self.status {
            GameStatus::Running {
                supported: true,
                ..
            } if self.connected => [0x16, 0x83, 0x4a],
            GameStatus::Running {
                ..
            } => [0xa7, 0x66, 0x04],
            _ => [0x85, 0x8e, 0x9a],
        }
    }
    pub fn update(&mut self, status: GameStatus) {
        if status == GameStatus::Waiting {
            if self.attached.take().is_some() {
                self.closed_sessions += 1;
            }
            self.connected = false;
        }
        self.status = status;
    }
    pub fn ready(&mut self, pid: u32) {
        self.attached = Some(pid);
        self.connected = true;
    }
    pub fn unavailable(&mut self) {
        self.connected = false;
    }
    pub fn running(&self) -> bool {
        matches!(self.status, GameStatus::Running { .. })
    }
    pub fn can_launch(&self) -> bool {
        self.status == GameStatus::Waiting
    }
    pub fn label(&self) -> &'static str {
        match self.status {
            GameStatus::Waiting => "Waiting for StarRupture",
            GameStatus::Running {
                supported: false,
                ..
            } => "Unsupported game build",
            GameStatus::Running {
                ..
            } if self.connected => "Connected to StarRupture",
            GameStatus::Running {
                ..
            } => "StarRupture detected — connecting",
            GameStatus::Unknown => "Checking StarRupture status",
        }
    }
    pub fn location(&self) -> &str {
        match &self.status {
            GameStatus::Running {
                executable,
                ..
            } => executable,
            _ => "Detected when the game starts",
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn running(supported: bool) -> GameStatus {
        GameStatus::Running {
            pid: 123,
            creation_ticks: 456,
            executable: r"D:\Games\Game.exe".into(),
            supported,
        }
    }
    #[test]
    fn launch_waits_for_presence_check_suppresses_duplicates_and_allows_explicit_retry() {
        let now = Instant::now();
        let mut connection = Connection::default();
        let mut launch = Launch::default();
        assert_eq!(launch.advance(&connection, now), LaunchAction::None);
        launch.request();
        assert_eq!(launch.advance(&connection, now), LaunchAction::None);
        connection.update(GameStatus::Waiting);
        assert_eq!(launch.advance(&connection, now), LaunchAction::OpenSteam);
        launch.request();
        assert_eq!(launch.advance(&connection, now + Duration::from_secs(119)), LaunchAction::None);
        assert_eq!(
            launch.advance(&connection, now + Duration::from_secs(120)),
            LaunchAction::TimedOut
        );
        assert_eq!(launch.advance(&connection, now + Duration::from_secs(121)), LaunchAction::None);
        launch.request();
        assert_eq!(
            launch.advance(&connection, now + Duration::from_secs(122)),
            LaunchAction::OpenSteam
        );
        connection.update(running(false));
        launch.request();
        assert_eq!(launch.advance(&connection, now + Duration::from_secs(123)), LaunchAction::None);
        assert!(!launch.pending());
        connection.update(GameStatus::Waiting);
        assert_eq!(launch.advance(&connection, now + Duration::from_secs(124)), LaunchAction::None);
    }
    #[test]
    fn only_confirmed_exit_of_an_attached_session_counts() {
        let mut connection = Connection::default();
        assert_eq!(connection.indicator_colour(), [0x85, 0x8e, 0x9a]);
        assert!(!connection.can_launch());
        connection.update(GameStatus::Waiting);
        assert!(connection.can_launch());
        connection.update(running(false));
        assert_eq!(connection.indicator_colour(), [0xa7, 0x66, 0x04]);
        assert!(connection.running());
        assert!(!connection.can_launch());
        connection.update(GameStatus::Waiting);
        assert_eq!(connection.closed_sessions, 0);
        connection.update(running(true));
        connection.ready(123);
        assert_eq!(connection.indicator_colour(), [0x16, 0x83, 0x4a]);
        connection.unavailable();
        assert_eq!(connection.indicator_colour(), [0xa7, 0x66, 0x04]);
        connection.update(GameStatus::Unknown);
        assert_eq!(connection.closed_sessions, 0);
        connection.update(running(true));
        connection.ready(123);
        connection.update(GameStatus::Waiting);
        connection.update(GameStatus::Waiting);
        assert_eq!(connection.closed_sessions, 1);
        assert!(!connection.connected);
    }
}
