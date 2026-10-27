//! Cooperative cancellation with interruptible deadline waits.
use std::{
    sync::{
        Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};
#[derive(Default)]
pub struct StopSignal {
    requested: AtomicBool,
    wake: Condvar,
    gate: Mutex<()>,
}
impl StopSignal {
    pub fn is_requested(&self) -> bool {
        self.requested.load(Ordering::Relaxed)
    }
    pub fn request(&self) {
        let _gate = self.gate.lock().unwrap_or_else(|e| e.into_inner());
        self.requested.store(true, Ordering::Relaxed);
        self.wake.notify_all();
    }
    pub fn wait_until(&self, deadline: Instant) {
        let gate = self.gate.lock().unwrap_or_else(|e| e.into_inner());
        let duration = deadline.saturating_duration_since(Instant::now());
        let _wait = self
            .wake
            .wait_timeout_while(gate, duration, |_| !self.is_requested())
            .unwrap_or_else(|e| e.into_inner());
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{Arc, Barrier},
        time::Duration,
    };
    #[test]
    fn cancellation_before_and_during_a_wait_returns_promptly() {
        let signal = Arc::new(StopSignal::default());
        let entered = Arc::new(Barrier::new(2));
        let worker_signal = signal.clone();
        let worker_entered = entered.clone();
        let worker = std::thread::spawn(move || {
            worker_entered.wait();
            worker_signal.wait_until(Instant::now() + Duration::from_secs(5));
            assert!(worker_signal.is_requested());
        });
        let start = Instant::now();
        entered.wait();
        signal.request();
        worker.join().unwrap();
        signal.wait_until(Instant::now() + Duration::from_secs(5));
        assert!(start.elapsed() < Duration::from_secs(2));
    }
    #[test]
    fn a_deadline_expires_without_requesting_cancellation() {
        let signal = StopSignal::default();
        let deadline = Instant::now() + Duration::from_millis(5);
        signal.wait_until(deadline);
        assert!(Instant::now() >= deadline);
        assert!(!signal.is_requested());
        signal.wait_until(deadline);
    }
}
