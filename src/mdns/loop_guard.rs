use std::collections::VecDeque;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::{Duration, Instant};

const ECHO_WINDOW: Duration = Duration::from_secs(5);

/// Remembers packets this relay multicast, so their loopback copies are not relayed back.
#[derive(Debug, Default)]
pub struct LoopGuard {
    /// Oldest first.
    sent: VecDeque<(u64, Instant)>,
}

impl LoopGuard {
    pub fn record(&mut self, bytes: &[u8], now: Instant) {
        self.prune(now);
        self.sent.push_back((hash(bytes), now));
    }

    pub fn is_echo(&self, bytes: &[u8], now: Instant) -> bool {
        let hash = hash(bytes);
        self.sent
            .iter()
            .any(|&(sent, at)| sent == hash && now.duration_since(at) < ECHO_WINDOW)
    }

    fn prune(&mut self, now: Instant) {
        while let Some(&(_, at)) = self.sent.front()
            && now.duration_since(at) >= ECHO_WINDOW
        {
            self.sent.pop_front();
        }
    }
}

fn hash(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guard_with(bytes: &[u8], now: Instant) -> LoopGuard {
        let mut guard = LoopGuard::default();
        guard.record(bytes, now);
        guard
    }

    #[test]
    fn echo_within_window_is_detected() {
        let now = Instant::now();
        let guard = guard_with(b"packet", now);
        assert!(guard.is_echo(b"packet", now + Duration::from_secs(4)));
    }

    #[test]
    fn echo_after_window_is_not_detected() {
        let now = Instant::now();
        let guard = guard_with(b"packet", now);
        assert!(!guard.is_echo(b"packet", now + ECHO_WINDOW));
    }

    #[test]
    fn different_bytes_are_not_an_echo() {
        let now = Instant::now();
        let guard = guard_with(b"packet", now);
        assert!(!guard.is_echo(b"other", now));
    }

    #[test]
    fn old_entries_are_pruned() {
        let now = Instant::now();
        let mut guard = guard_with(b"old", now);
        guard.record(b"new", now + ECHO_WINDOW);
        assert_eq!(guard.sent.len(), 1);
    }
}
