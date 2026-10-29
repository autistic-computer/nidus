use std::collections::VecDeque;

const MIN_WINDOW: usize = 8 * 1024;
const RATE_WINDOW_NS: u64 = 2_000_000_000;
const RTT_WINDOW_NS: u64 = 10_000_000_000;

struct Sent {
    serial: u64,
    bytes: usize,
    at: u64,
    delivered: u64,
    delivered_at: u64,
    app_limited: bool,
}

#[derive(Default)]
pub struct Pacer {
    sent: VecDeque<Sent>,
    serial: u64,
    in_flight: usize,
    delivered: u64,
    delivered_at: u64,
    rates: VecDeque<(u64, f64)>,
    min_rtt: Option<(u64, u64)>,
}

impl Pacer {
    pub fn sent(&mut self, bytes: usize, more: bool, now: u64) -> u64 {
        if self.in_flight == 0 {
            self.delivered_at = now;
        }
        self.serial += 1;
        self.in_flight += bytes;
        self.sent.push_back(Sent {
            serial: self.serial,
            bytes,
            at: now,
            delivered: self.delivered,
            delivered_at: self.delivered_at,
            app_limited: !more,
        });
        self.serial
    }

    pub fn acked(&mut self, serial: u64, now: u64) -> Option<u64> {
        let mut last = None;
        while self.sent.front().is_some_and(|s| s.serial <= serial) {
            let sent = self.sent.pop_front().unwrap();
            self.delivered += sent.bytes as u64;
            self.in_flight -= sent.bytes;
            last = Some(sent);
        }
        let sent = last?;
        self.delivered_at = now;
        let rtt = now.saturating_sub(sent.at).max(1);
        if self
            .min_rtt
            .is_none_or(|(value, at)| rtt <= value || now - at > RTT_WINDOW_NS)
        {
            self.min_rtt = Some((rtt, now));
        }
        let interval = now.saturating_sub(sent.delivered_at).max(rtt);
        let rate = (self.delivered - sent.delivered) as f64 / interval as f64;
        if !sent.app_limited || rate > self.rate() {
            self.rates.push_back((now, rate));
        }
        while self.rates.len() > 1
            && self
                .rates
                .front()
                .is_some_and(|(at, _)| now - at > RATE_WINDOW_NS)
        {
            self.rates.pop_front();
        }
        Some(rtt)
    }

    fn rate(&self) -> f64 {
        self.rates.iter().map(|r| r.1).fold(0., f64::max)
    }

    pub fn window(&self) -> usize {
        let rate = self.rate();
        let rtt = self.min_rtt.map_or(0, |r| r.0);
        ((2. * rate * rtt as f64) as usize).max(MIN_WINDOW)
    }

    pub fn in_flight(&self) -> usize {
        self.in_flight
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_tracks_two_round_trips_of_delivery_rate() {
        let mut pacer = Pacer::default();
        assert_eq!(pacer.window(), MIN_WINDOW);
        let mut now = 0;
        for _ in 0..50 {
            let serial = pacer.sent(1_000_000, true, now);
            now += 5_000_000;
            assert_eq!(pacer.acked(serial, now), Some(5_000_000));
            now += 5_000_000;
        }
        let window = pacer.window();
        assert!((1_900_000..=2_100_000).contains(&window), "{window}");
        assert_eq!(pacer.in_flight(), 0);
    }

    #[test]
    fn a_still_screen_keeps_the_window_the_link_last_showed() {
        let mut pacer = Pacer::default();
        let mut now = 0;
        for _ in 0..50 {
            let serial = pacer.sent(1_000_000, true, now);
            now += 5_000_000;
            pacer.acked(serial, now);
        }
        let busy = pacer.window();
        for _ in 0..600 {
            let serial = pacer.sent(200, false, now);
            now += 5_000_000;
            pacer.acked(serial, now);
            now += 95_000_000;
        }
        assert_eq!(pacer.window(), busy);
        for _ in 0..50 {
            let serial = pacer.sent(100_000, true, now);
            now += 5_000_000;
            pacer.acked(serial, now);
            now += 95_000_000;
        }
        assert!(pacer.window() < busy / 4, "{}", pacer.window());
    }
}
