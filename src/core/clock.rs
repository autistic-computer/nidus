use std::{collections::VecDeque, error::Error, fmt, time::Duration};

use super::MonotonicTimeMicros;

const DEFAULT_SAMPLE_WINDOW: usize = 64;
const DEFAULT_MAXIMUM_SKEW_PPM: u32 = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockConfig {
    pub maximum_skew_ppm: u32,
    pub sample_window: usize,
}

impl Default for ClockConfig {
    fn default() -> Self {
        Self {
            maximum_skew_ppm: DEFAULT_MAXIMUM_SKEW_PPM,
            sample_window: DEFAULT_SAMPLE_WINDOW,
        }
    }
}

impl ClockConfig {
    pub fn validate(self) -> Result<Self, ClockError> {
        if self.sample_window < 2 {
            return Err(ClockError::InvalidConfig(
                "clock sample window must contain at least two observations",
            ));
        }
        if self.maximum_skew_ppm > 100_000 {
            return Err(ClockError::InvalidConfig(
                "maximum clock skew must not exceed 100000 ppm",
            ));
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockSample {
    pub sender_time: MonotonicTimeMicros,
    pub receiver_time: MonotonicTimeMicros,
    pub uncertainty: Duration,
}

impl ClockSample {
    pub const fn exact(
        sender_time: MonotonicTimeMicros,
        receiver_time: MonotonicTimeMicros,
    ) -> Self {
        Self {
            sender_time,
            receiver_time,
            uncertainty: Duration::ZERO,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbeExchange {
    pub receiver_sent_at: MonotonicTimeMicros,
    pub sender_received_at: MonotonicTimeMicros,
    pub sender_echoed_at: MonotonicTimeMicros,
    pub receiver_received_at: MonotonicTimeMicros,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClockStats {
    pub offset_micros: Option<f64>,
    pub skew: Option<f64>,
    pub skew_ppm: Option<f64>,
    pub residual_error_micros: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
struct Fit {
    sender_origin: MonotonicTimeMicros,
    receiver_origin: MonotonicTimeMicros,
    centered_intercept: f64,
    skew: f64,
    residual_error_micros: f64,
}

#[derive(Debug, Clone)]
pub struct ClockMapper {
    config: ClockConfig,
    samples: VecDeque<ClockSample>,
    fit: Option<Fit>,
    arrival_bootstrap: bool,
}

impl ClockMapper {
    pub fn new(config: ClockConfig) -> Result<Self, ClockError> {
        let config = config.validate()?;
        Ok(Self {
            config,
            samples: VecDeque::with_capacity(config.sample_window),
            fit: None,
            arrival_bootstrap: false,
        })
    }

    pub fn is_ready(&self) -> bool {
        self.fit.is_some()
    }

    pub fn uses_arrival_bootstrap(&self) -> bool {
        self.arrival_bootstrap
    }

    pub fn bootstrap_from_arrival(
        &mut self,
        sender_time: MonotonicTimeMicros,
        receiver_time: MonotonicTimeMicros,
    ) -> Result<(), ClockError> {
        if self.is_ready() {
            return Err(ClockError::InvalidSample(
                "arrival bootstrap requires an empty clock fit",
            ));
        }
        self.ingest_sample(ClockSample::exact(sender_time, receiver_time))?;
        self.arrival_bootstrap = true;
        Ok(())
    }

    pub fn ingest_sample(&mut self, sample: ClockSample) -> Result<(), ClockError> {
        if sample.uncertainty.as_micros() > u128::from(u64::MAX) {
            return Err(ClockError::InvalidSample(
                "clock sample uncertainty does not fit in microseconds",
            ));
        }
        if self.samples.len() == self.config.sample_window {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
        self.arrival_bootstrap = false;
        self.refit();
        Ok(())
    }

    pub fn ingest_probe(&mut self, probe: ProbeExchange) -> Result<(), ClockError> {
        let receiver_round_trip = probe
            .receiver_received_at
            .0
            .checked_sub(probe.receiver_sent_at.0)
            .ok_or(ClockError::InvalidSample(
                "receiver probe clock moved backwards",
            ))?;
        let sender_processing = probe
            .sender_echoed_at
            .0
            .checked_sub(probe.sender_received_at.0)
            .ok_or(ClockError::InvalidSample(
                "sender probe clock moved backwards",
            ))?;

        if sender_processing > receiver_round_trip {
            return Err(ClockError::InvalidSample(
                "sender processing exceeded receiver round trip",
            ));
        }

        let sender_midpoint = midpoint(probe.sender_received_at.0, probe.sender_echoed_at.0);
        let receiver_midpoint = midpoint(probe.receiver_sent_at.0, probe.receiver_received_at.0);
        let uncertainty = Duration::from_micros((receiver_round_trip - sender_processing) / 2);
        if self.arrival_bootstrap {
            self.samples.clear();
            self.fit = None;
        }
        self.ingest_sample(ClockSample {
            sender_time: MonotonicTimeMicros(sender_midpoint),
            receiver_time: MonotonicTimeMicros(receiver_midpoint),
            uncertainty,
        })
    }

    pub fn map(&self, sender_time: MonotonicTimeMicros) -> Result<MonotonicTimeMicros, ClockError> {
        let fit = self.fit.ok_or(ClockError::NotReady)?;
        let mapped = mapped_micros(fit, sender_time);
        if !mapped.is_finite() || !(0.0..=u64::MAX as f64).contains(&mapped) {
            return Err(ClockError::MappedTimeOutOfRange);
        }
        Ok(MonotonicTimeMicros(mapped.round() as u64))
    }

    pub fn stats(&self) -> ClockStats {
        let offset_micros = self.fit.map(|fit| {
            fit.receiver_origin.0 as f64 + fit.centered_intercept
                - fit.skew * fit.sender_origin.0 as f64
        });
        ClockStats {
            offset_micros,
            skew: self.fit.map(|fit| fit.skew),
            skew_ppm: self.fit.map(|fit| (fit.skew - 1.0) * 1_000_000.0),
            residual_error_micros: self.fit.map(|fit| fit.residual_error_micros),
        }
    }

    fn refit(&mut self) {
        let Some(first) = self.samples.front().copied() else {
            self.fit = None;
            return;
        };

        let count = self.samples.len() as f64;
        let weight = |sample: &ClockSample| {
            let uncertainty = sample.uncertainty.as_micros().max(1) as f64;
            1.0 / (uncertainty * uncertainty)
        };
        let (total_weight, sum_x, sum_y) =
            self.samples
                .iter()
                .fold((0.0, 0.0, 0.0), |(total, x, y), sample| {
                    let weight = weight(sample);
                    (
                        total + weight,
                        x + weight * signed_difference(sample.sender_time.0, first.sender_time.0),
                        y + weight
                            * signed_difference(sample.receiver_time.0, first.receiver_time.0),
                    )
                });
        let mean_x = sum_x / total_weight;
        let mean_y = sum_y / total_weight;
        let (variance, covariance) =
            self.samples
                .iter()
                .fold((0.0, 0.0), |(variance, covariance), sample| {
                    let weight = weight(sample);
                    let x = signed_difference(sample.sender_time.0, first.sender_time.0) - mean_x;
                    let y =
                        signed_difference(sample.receiver_time.0, first.receiver_time.0) - mean_y;
                    (variance + weight * x * x, covariance + weight * x * y)
                });

        let maximum_skew = f64::from(self.config.maximum_skew_ppm) / 1_000_000.0;
        let skew = if variance > f64::EPSILON {
            (covariance / variance).clamp(1.0 - maximum_skew, 1.0 + maximum_skew)
        } else {
            1.0
        };
        let centered_intercept = mean_y - skew * mean_x;

        let mut residual_squared = 0.0;
        for sample in &self.samples {
            let x = signed_difference(sample.sender_time.0, first.sender_time.0);
            let observed = signed_difference(sample.receiver_time.0, first.receiver_time.0);
            let residual = observed - (centered_intercept + skew * x);
            residual_squared += residual * residual;
        }

        self.fit = Some(Fit {
            sender_origin: first.sender_time,
            receiver_origin: first.receiver_time,
            centered_intercept,
            skew,
            residual_error_micros: (residual_squared / count).sqrt(),
        });
    }
}

impl Default for ClockMapper {
    fn default() -> Self {
        Self::new(ClockConfig::default()).expect("default clock config is valid")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockError {
    InvalidConfig(&'static str),
    InvalidSample(&'static str),
    NotReady,
    MappedTimeOutOfRange,
}

impl fmt::Display for ClockError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(message) | Self::InvalidSample(message) => {
                formatter.write_str(message)
            }
            Self::NotReady => formatter.write_str("clock mapping has no probe observations"),
            Self::MappedTimeOutOfRange => {
                formatter.write_str("mapped receiver time is outside its monotonic range")
            }
        }
    }
}

impl Error for ClockError {}

fn midpoint(start: u64, end: u64) -> u64 {
    start + (end - start) / 2
}

fn signed_difference(value: u64, origin: u64) -> f64 {
    if value >= origin {
        (value - origin) as f64
    } else {
        -((origin - value) as f64)
    }
}

fn mapped_micros(fit: Fit, sender_time: MonotonicTimeMicros) -> f64 {
    fit.receiver_origin.0 as f64
        + fit.centered_intercept
        + fit.skew * signed_difference(sender_time.0, fit.sender_origin.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn time(value: u64) -> MonotonicTimeMicros {
        MonotonicTimeMicros(value)
    }

    #[test]
    fn affine_fit_recovers_offset_and_skew() {
        let mut mapper = ClockMapper::default();
        for sender in (1_000_000..=6_000_000).step_by(1_000_000) {
            let receiver = 75_000 + sender + sender / 5_000;
            mapper
                .ingest_sample(ClockSample::exact(time(sender), time(receiver)))
                .unwrap();
        }

        let mapped = mapper.map(time(7_500_000)).unwrap();
        assert_eq!(mapped, time(7_576_500));
        let stats = mapper.stats();
        assert!((stats.offset_micros.unwrap() - 75_000.0).abs() < 0.001);
        assert!((stats.skew_ppm.unwrap() - 200.0).abs() < 0.001);
        assert!(stats.residual_error_micros.unwrap() < 0.001);
    }

    #[test]
    fn skew_is_bounded_by_configuration() {
        let mut mapper = ClockMapper::new(ClockConfig {
            maximum_skew_ppm: 100,
            ..ClockConfig::default()
        })
        .unwrap();
        mapper
            .ingest_sample(ClockSample::exact(time(0), time(10_000)))
            .unwrap();
        mapper
            .ingest_sample(ClockSample::exact(time(1_000_000), time(1_015_000)))
            .unwrap();

        assert!((mapper.stats().skew_ppm.unwrap() - 100.0).abs() < 0.001);
        assert!(mapper.stats().residual_error_micros.unwrap() > 1_000.0);
    }

    #[test]
    fn probe_midpoints_create_a_sender_to_receiver_observation() {
        let mut mapper = ClockMapper::default();
        mapper
            .ingest_probe(ProbeExchange {
                receiver_sent_at: time(10_000),
                sender_received_at: time(51_000),
                sender_echoed_at: time(51_200),
                receiver_received_at: time(12_200),
            })
            .unwrap();

        assert_eq!(mapper.map(time(52_000)).unwrap(), time(12_000));
        assert_eq!(mapper.samples[0].uncertainty, Duration::from_millis(1));
    }

    #[test]
    fn queued_probes_barely_move_a_fit_built_from_clean_ones() {
        let offset = 10_000;
        let mut mapper = ClockMapper::default();
        for second in 1..=8 {
            let sent = second * 1_000_000;
            let back = if second % 2 == 0 { 8_200 } else { 200 };
            mapper
                .ingest_probe(ProbeExchange {
                    receiver_sent_at: time(sent),
                    sender_received_at: time(sent + 200 - offset),
                    sender_echoed_at: time(sent + 200 - offset),
                    receiver_received_at: time(sent + 200 + back),
                })
                .unwrap();
        }
        let expected = 9_000_000 + offset;
        let mapped = mapper.map(time(9_000_000)).unwrap().0;
        assert!(
            mapped.abs_diff(expected) < 100,
            "mapped {mapped}, expected about {expected}"
        );
    }

    #[test]
    fn malformed_probe_keeps_the_fit() {
        let mut mapper = ClockMapper::default();
        mapper
            .ingest_sample(ClockSample::exact(time(1), time(2)))
            .unwrap();
        let error = mapper
            .ingest_probe(ProbeExchange {
                receiver_sent_at: time(20),
                sender_received_at: time(50),
                sender_echoed_at: time(40),
                receiver_received_at: time(30),
            })
            .unwrap_err();

        assert!(matches!(error, ClockError::InvalidSample(_)));
        assert!(mapper.is_ready());
        assert_eq!(mapper.samples.len(), 1);
    }

    #[test]
    fn first_probe_replaces_network_delay_biased_arrival_bootstrap() {
        let mut mapper = ClockMapper::default();
        mapper
            .bootstrap_from_arrival(time(100_000), time(150_000))
            .unwrap();
        assert!(mapper.uses_arrival_bootstrap());

        mapper
            .ingest_probe(ProbeExchange {
                receiver_sent_at: time(200_000),
                sender_received_at: time(200_500),
                sender_echoed_at: time(200_500),
                receiver_received_at: time(201_000),
            })
            .unwrap();

        assert!(!mapper.uses_arrival_bootstrap());
        assert_eq!(mapper.samples.len(), 1);
        assert_eq!(mapper.map(time(201_000)).unwrap(), time(201_000));
    }

    #[test]
    fn recent_window_bounds_memory() {
        let mut mapper = ClockMapper::new(ClockConfig {
            sample_window: 3,
            ..ClockConfig::default()
        })
        .unwrap();
        for sender in 0..6 {
            mapper
                .ingest_sample(ClockSample::exact(
                    time(sender * 1_000),
                    time(sender * 1_000),
                ))
                .unwrap();
        }
        assert_eq!(mapper.samples.len(), 3);
        assert_eq!(mapper.samples[0].sender_time, time(3_000));
    }
}
