use std::time::{Duration, Instant};

pub struct FrontendRateLimit {
    start: Instant,
    count: u8,
    suppressed: u32,
}

impl Default for FrontendRateLimit {
    fn default() -> Self {
        Self {
            start: Instant::now(),
            count: 0,
            suppressed: 0,
        }
    }
}

impl FrontendRateLimit {
    /// Returns whether to keep this event and whether it is the first suppression in the window.
    pub fn allow(&mut self) -> (bool, bool) {
        if self.start.elapsed() >= Duration::from_secs(60) {
            self.start = Instant::now();
            self.count = 0;
            self.suppressed = 0;
        }
        if self.count >= 20 {
            self.suppressed = self.suppressed.saturating_add(1);
            return (false, self.suppressed == 1);
        }
        self.count += 1;
        (true, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_twenty_frontend_errors_and_signals_suppression_once() {
        let mut limit = FrontendRateLimit::default();
        let results = (0..100).map(|_| limit.allow()).collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|(allowed, _)| *allowed).count(), 20);
        assert_eq!(results.iter().filter(|(_, first)| *first).count(), 1);
    }
}
