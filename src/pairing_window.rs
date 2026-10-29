use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use serde::{Deserialize, Serialize};
use tokio::time::Instant;

use crate::neighbors::{Stranger, Strangers};

pub const FILE_NAME: &str = "pairing-window";
pub const OPEN_FOR: Duration = Duration::from_secs(10 * 60);
pub const HOLD_OFF: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    HadPeers,
    Accepted,
    Expired,
    Rival,
    Restarted,
    Failed,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    #[default]
    Never,
    Eligible,
    Open,
    Closed,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct View {
    pub state: State,
    pub seconds_left: Option<u64>,
    pub holding: Option<Holding>,
    pub reason: Option<Reason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Holding {
    pub name: String,
    pub mark: String,
    pub ms_left: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Never,
    Eligible,
    Open { until: Instant },
    Closed(Reason),
}

#[derive(Debug)]
pub struct PairingWindow {
    path: PathBuf,
    phase: Phase,
    holding: Option<(Stranger, Instant)>,
}

impl PairingWindow {
    pub fn load(state_dir: &Path) -> Self {
        let path = state_dir.join(FILE_NAME);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                if error.kind() != ErrorKind::NotFound {
                    tracing::warn!(path = %path.display(), %error, "could not read the pairing window");
                }
                String::new()
            }
        };
        let mut window = Self {
            path,
            phase: Phase::Never,
            holding: None,
        };
        match text.split_whitespace().collect::<Vec<_>>()[..] {
            ["eligible"] => window.phase = Phase::Eligible,
            ["open", ..] => window.close(Reason::Restarted),
            ["closed", reason] => {
                window.phase = Phase::Closed(parse_reason(reason).unwrap_or(Reason::Restarted));
            }
            _ => {}
        }
        window
    }

    pub fn mark_eligible(state_dir: &Path) -> Result<(), crate::config::ConfigError> {
        crate::config::save_text(&state_dir.join(FILE_NAME), "eligible\n")
    }

    pub fn open(&mut self, has_peers: bool, now: Instant) {
        if self.phase != Phase::Eligible {
            return;
        }
        if has_peers {
            self.close(Reason::HadPeers);
            return;
        }
        let since = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if let Err(error) = crate::config::save_text(&self.path, &format!("open {since}\n")) {
            tracing::warn!(%error, "the pairing window stays shut, since it could not be saved open");
            self.phase = Phase::Never;
            return;
        }
        self.phase = Phase::Open {
            until: now + OPEN_FOR,
        };
    }

    pub fn placed(&mut self) {
        if matches!(self.phase, Phase::Eligible | Phase::Open { .. }) {
            self.close(Reason::Accepted);
        }
    }

    pub fn not_added(&mut self) {
        if matches!(self.phase, Phase::Open { .. }) {
            self.close(Reason::Failed);
        }
    }

    pub fn is_open(&self) -> bool {
        matches!(self.phase, Phase::Open { .. })
    }

    pub fn tick(&mut self, strangers: &Strangers, now: Instant) -> Option<Stranger> {
        let Phase::Open { until } = self.phase else {
            return None;
        };
        if now >= until {
            self.close(Reason::Expired);
            return None;
        }
        if strangers.present.len() > 1 || strangers.turned_away {
            self.close(Reason::Rival);
            return None;
        }
        let candidate = match &strangers.present[..] {
            [only]
                if strangers.unidentified == 0
                    && only.answered
                    && only.fresh
                    && !only.duplicate_name =>
            {
                Some(only)
            }
            _ => None,
        };
        let same = |held: &(Stranger, Instant)| Some(&held.0) == candidate;
        if !self.holding.as_ref().is_some_and(same) {
            self.holding = candidate.map(|stranger| (stranger.clone(), now));
        }
        let (stranger, since) = self.holding.as_ref()?;
        if now.duration_since(*since) < HOLD_OFF {
            return None;
        }
        Some(stranger.clone())
    }

    pub fn view(&self, now: Instant) -> View {
        match self.phase {
            Phase::Never => View::default(),
            Phase::Eligible => View {
                state: State::Eligible,
                ..View::default()
            },
            Phase::Open { until } => View {
                state: State::Open,
                seconds_left: Some(until.saturating_duration_since(now).as_secs()),
                holding: self.holding.as_ref().map(|(stranger, since)| Holding {
                    name: stranger.name.clone(),
                    mark: stranger.mark.clone(),
                    ms_left: (*since + HOLD_OFF)
                        .saturating_duration_since(now)
                        .as_millis() as u64,
                }),
                reason: None,
            },
            Phase::Closed(reason) => View {
                state: State::Closed,
                reason: Some(reason),
                ..View::default()
            },
        }
    }

    fn close(&mut self, reason: Reason) {
        self.phase = Phase::Closed(reason);
        self.holding = None;
        let name = serde_json::to_value(reason)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_default();
        self.save(&format!("closed {name}\n"));
    }

    fn save(&self, text: &str) {
        if let Err(error) = crate::config::save_text(&self.path, text) {
            tracing::warn!(%error, "could not save the pairing window");
        }
    }
}

fn parse_reason(text: &str) -> Option<Reason> {
    serde_json::from_value(serde_json::Value::String(text.to_owned())).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stranger(key: &str, name: &str) -> Stranger {
        Stranger {
            key: key.into(),
            name: name.into(),
            mark: "a1b2c3".into(),
            answered: true,
            fresh: true,
            duplicate_name: false,
        }
    }

    fn around(present: &[Stranger], unidentified: usize) -> Strangers {
        Strangers {
            present: present.to_vec(),
            unidentified,
            turned_away: false,
        }
    }

    fn opened() -> (tempfile::TempDir, PairingWindow) {
        let directory = tempfile::tempdir().unwrap();
        PairingWindow::mark_eligible(directory.path()).unwrap();
        let mut window = PairingWindow::load(directory.path());
        assert_eq!(window.view(Instant::now()).state, State::Eligible);
        window.open(false, Instant::now());
        (directory, window)
    }

    async fn after(
        window: &mut PairingWindow,
        wait: Duration,
        who: &Strangers,
    ) -> Option<Stranger> {
        tokio::time::advance(wait).await;
        window.tick(who, Instant::now())
    }

    #[tokio::test(start_paused = true)]
    async fn a_lone_stranger_joins_after_exactly_the_hold_off() {
        let (_directory, mut window) = opened();
        let desk = around(&[stranger("k1", "desk")], 0);
        assert_eq!(window.tick(&desk, Instant::now()), None);
        let view = window.view(Instant::now());
        assert_eq!(view.state, State::Open);
        assert_eq!(view.seconds_left, Some(OPEN_FOR.as_secs()));
        assert_eq!(view.holding.unwrap().ms_left, HOLD_OFF.as_millis() as u64);

        let almost = HOLD_OFF - Duration::from_millis(1);
        assert_eq!(after(&mut window, almost, &desk).await, None);
        let joined = after(&mut window, Duration::from_millis(1), &desk).await;
        assert_eq!(joined, Some(stranger("k1", "desk")));
        assert_eq!(window.view(Instant::now()).state, State::Open);
        window.placed();
        assert_eq!(window.view(Instant::now()).reason, Some(Reason::Accepted));
        assert_eq!(after(&mut window, HOLD_OFF, &desk).await, None);
    }

    #[tokio::test(start_paused = true)]
    async fn a_computer_that_could_not_be_added_closes_the_window_as_failed() {
        let (directory, mut window) = opened();
        let desk = around(&[stranger("k1", "desk")], 0);
        window.tick(&desk, Instant::now());
        assert!(after(&mut window, HOLD_OFF, &desk).await.is_some());
        window.not_added();
        assert!(!window.is_open());
        assert_eq!(after(&mut window, HOLD_OFF, &desk).await, None);
        let reloaded = PairingWindow::load(directory.path());
        assert_eq!(reloaded.view(Instant::now()).reason, Some(Reason::Failed));
        let path = directory.path().join(FILE_NAME);
        assert_eq!(std::fs::read_to_string(path).unwrap(), "closed failed\n");
    }

    #[tokio::test(start_paused = true)]
    async fn a_rival_during_the_hold_off_closes_the_window_for_good() {
        let (_directory, mut window) = opened();
        let desk = around(&[stranger("k1", "desk")], 0);
        window.tick(&desk, Instant::now());
        let both = around(&[stranger("k1", "desk"), stranger("k2", "evil")], 0);
        assert_eq!(
            after(&mut window, Duration::from_secs(3), &both).await,
            None
        );
        assert_eq!(after(&mut window, HOLD_OFF * 2, &desk).await, None);
        assert_eq!(window.view(Instant::now()).reason, Some(Reason::Rival));
    }

    #[tokio::test(start_paused = true)]
    async fn the_window_closes_after_ten_minutes() {
        let (_directory, mut window) = opened();
        let nobody = around(&[], 0);
        assert_eq!(after(&mut window, OPEN_FOR / 2, &nobody).await, None);
        assert_eq!(window.view(Instant::now()).state, State::Open);
        let desk = around(&[stranger("k1", "desk")], 0);
        let late = OPEN_FOR / 2 - Duration::from_millis(1);
        assert_eq!(after(&mut window, late, &desk).await, None);
        assert_eq!(
            after(&mut window, Duration::from_millis(1), &desk).await,
            None
        );
        assert_eq!(window.view(Instant::now()).reason, Some(Reason::Expired));
    }

    #[tokio::test(start_paused = true)]
    async fn namesakes_stale_hellos_and_unknown_records_never_join() {
        let (_directory, mut window) = opened();
        let namesake = Stranger {
            duplicate_name: true,
            ..stranger("k1", "desk")
        };
        let stale = Stranger {
            fresh: false,
            ..stranger("k1", "desk")
        };
        for who in [around(&[namesake], 0), around(&[stale], 0)] {
            window.tick(&who, Instant::now());
            assert_eq!(after(&mut window, HOLD_OFF * 3, &who).await, None);
        }
        let unsure = around(&[stranger("k1", "desk")], 1);
        window.tick(&unsure, Instant::now());
        assert_eq!(after(&mut window, HOLD_OFF * 3, &unsure).await, None);
        let known = around(&[stranger("k1", "desk")], 0);
        assert_eq!(after(&mut window, Duration::ZERO, &known).await, None);
        assert_eq!(
            after(&mut window, HOLD_OFF - Duration::from_millis(1), &known).await,
            None
        );
        assert!(
            after(&mut window, Duration::from_millis(1), &known)
                .await
                .is_some()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_computer_that_only_said_hello_never_joins_but_is_a_rival() {
        let (_directory, mut window) = opened();
        let knocked = Stranger {
            answered: false,
            ..stranger("k1", "desk")
        };
        let alone = around(std::slice::from_ref(&knocked), 0);
        window.tick(&alone, Instant::now());
        assert_eq!(after(&mut window, HOLD_OFF * 3, &alone).await, None);
        assert_eq!(window.view(Instant::now()).holding, None);
        let known = around(&[stranger("k1", "desk")], 0);
        assert_eq!(after(&mut window, Duration::ZERO, &known).await, None);
        assert!(after(&mut window, HOLD_OFF, &known).await.is_some());

        let (_directory, mut window) = opened();
        let evil = Stranger {
            answered: false,
            ..stranger("k2", "evil")
        };
        let both = around(&[stranger("k1", "desk"), evil], 0);
        assert_eq!(after(&mut window, Duration::ZERO, &both).await, None);
        assert_eq!(window.view(Instant::now()).reason, Some(Reason::Rival));
    }

    #[tokio::test(start_paused = true)]
    async fn anything_turned_away_for_want_of_room_closes_the_window() {
        let (_directory, mut window) = opened();
        let desk = around(&[stranger("k1", "desk")], 0);
        window.tick(&desk, Instant::now());
        let full = Strangers {
            turned_away: true,
            ..desk.clone()
        };
        assert_eq!(after(&mut window, HOLD_OFF / 2, &full).await, None);
        assert_eq!(after(&mut window, HOLD_OFF * 2, &desk).await, None);
        assert_eq!(window.view(Instant::now()).reason, Some(Reason::Rival));
    }

    #[tokio::test(start_paused = true)]
    async fn a_new_candidate_starts_the_wait_again() {
        let (_directory, mut window) = opened();
        window.tick(&around(&[stranger("k1", "desk")], 0), Instant::now());
        let other = around(&[stranger("k2", "laptop")], 0);
        assert_eq!(
            after(&mut window, Duration::from_secs(4), &other).await,
            None
        );
        assert_eq!(
            after(&mut window, Duration::from_secs(4), &other).await,
            None
        );
        let joined = after(&mut window, Duration::from_secs(1), &other).await;
        assert_eq!(joined.unwrap().key, "k2");
    }

    #[tokio::test(start_paused = true)]
    async fn the_window_opens_only_once_on_a_fresh_install() {
        let directory = tempfile::tempdir().unwrap();
        let mut window = PairingWindow::load(directory.path());
        window.open(false, Instant::now());
        assert_eq!(window.view(Instant::now()), View::default());

        PairingWindow::mark_eligible(directory.path()).unwrap();
        let mut window = PairingWindow::load(directory.path());
        window.open(true, Instant::now());
        assert_eq!(window.view(Instant::now()).reason, Some(Reason::HadPeers));

        PairingWindow::mark_eligible(directory.path()).unwrap();
        let mut window = PairingWindow::load(directory.path());
        window.open(false, Instant::now());
        let path = directory.path().join(FILE_NAME);
        assert!(std::fs::read_to_string(&path).unwrap().starts_with("open "));
        let mut window = PairingWindow::load(directory.path());
        assert_eq!(window.view(Instant::now()).reason, Some(Reason::Restarted));
        window.open(false, Instant::now());
        let window = PairingWindow::load(directory.path());
        assert_eq!(window.view(Instant::now()).reason, Some(Reason::Restarted));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "closed restarted\n"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_window_that_cannot_be_saved_open_stays_shut() {
        let directory = tempfile::tempdir().unwrap();
        PairingWindow::mark_eligible(directory.path()).unwrap();
        let mut window = PairingWindow::load(directory.path());
        let path = directory.path().join(FILE_NAME);
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        window.open(false, Instant::now());
        assert_eq!(window.view(Instant::now()), View::default());
        let desk = around(&[stranger("k1", "desk")], 0);
        window.tick(&desk, Instant::now());
        assert_eq!(after(&mut window, HOLD_OFF * 2, &desk).await, None);
    }

    #[tokio::test(start_paused = true)]
    async fn placing_a_computer_by_hand_closes_the_window() {
        let (directory, mut window) = opened();
        window.tick(&around(&[stranger("k1", "desk")], 0), Instant::now());
        window.placed();
        assert_eq!(window.view(Instant::now()).reason, Some(Reason::Accepted));
        assert_eq!(window.view(Instant::now()).holding, None);
        let reloaded = PairingWindow::load(directory.path());
        assert_eq!(reloaded.view(Instant::now()).reason, Some(Reason::Accepted));
    }

    #[test]
    fn the_view_serializes_with_the_shared_names() {
        let view = View {
            state: State::Open,
            seconds_left: Some(42),
            holding: Some(Holding {
                name: "desk".into(),
                mark: "a1b2c3".into(),
                ms_left: 1500,
            }),
            reason: None,
        };
        assert_eq!(
            serde_json::to_value(&view).unwrap(),
            serde_json::json!({
                "state": "open", "seconds_left": 42, "reason": null,
                "holding": {"name": "desk", "mark": "a1b2c3", "ms_left": 1500},
            })
        );
        assert_eq!(
            serde_json::to_value(Reason::HadPeers).unwrap(),
            serde_json::json!("had_peers")
        );
        assert_eq!(parse_reason("had_peers"), Some(Reason::HadPeers));
        assert_eq!(parse_reason("nonsense"), None);
    }
}
