//! Frame pacing: how long to hold a frame back so a consumer that asks as
//! fast as it can still receives a steady frame rate.

/// Schedules deliveries one `interval` apart. Times are in any one unit.
#[derive(Debug, Default, Clone, Copy)]
pub struct Pacer {
    next_due: Option<u64>,
}

impl Pacer {
    /// Returns how long to wait before delivering a frame requested at `now`.
    pub fn wait(&mut self, now: u64, interval: u64) -> u64 {
        let (wait, next) = match self.next_due {
            None => (0, now + interval),
            // Far behind schedule: start a new one rather than catching up in a burst.
            Some(due) if now >= due && now - due >= interval => (0, now + interval),
            Some(due) if now >= due => (0, due + interval),
            Some(due) => {
                let wait = (due - now).min(interval);
                (wait, now + wait + interval)
            }
        };
        self.next_due = Some(next);
        wait
    }

    /// Forgets the schedule, so the next request is served at once.
    pub fn reset(&mut self) {
        self.next_due = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_request_is_served_at_once() {
        assert_eq!(Pacer::default().wait(5_000, 33), 0);
    }

    #[test]
    fn back_to_back_requests_are_spaced_one_interval_apart() {
        let mut p = Pacer::default();
        assert_eq!(p.wait(1_000, 33), 0);
        assert_eq!(p.wait(1_000, 33), 33);
        // The caller slept until 1033 and asks again immediately.
        assert_eq!(p.wait(1_033, 33), 33);
    }

    #[test]
    fn a_slightly_late_request_keeps_the_average_rate() {
        let mut p = Pacer::default();
        p.wait(1_000, 33);
        // Due at 1033, asked at 1040: served now, next still due at 1066.
        assert_eq!(p.wait(1_040, 33), 0);
        assert_eq!(p.wait(1_040, 33), 26);
    }

    #[test]
    fn a_long_gap_does_not_cause_a_burst() {
        let mut p = Pacer::default();
        p.wait(1_000, 33);
        assert_eq!(p.wait(9_000, 33), 0);
        assert_eq!(p.wait(9_000, 33), 33);
    }

    #[test]
    fn a_clock_that_goes_backwards_never_waits_longer_than_one_interval() {
        let mut p = Pacer::default();
        p.wait(1_000_000, 33);
        assert!(p.wait(10, 33) <= 33);
        assert!(p.wait(10, 33) <= 33);
    }

    #[test]
    fn reset_serves_the_next_request_at_once() {
        let mut p = Pacer::default();
        p.wait(1_000, 33);
        p.reset();
        assert_eq!(p.wait(1_000, 33), 0);
    }
}
