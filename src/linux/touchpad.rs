use std::time::Duration;

use crate::{
    capture::{CaptureTransition, KeyState, PointerFrame},
    core::{ContactId, MotionDelta, PointerButton},
};

const SLOW_COUNTS_PER_MM: f64 = 8.0;
const FAST_COUNTS_PER_MM: f64 = 24.0;
const SLOW_MM_PER_S: f64 = 25.0;
const FAST_MM_PER_S: f64 = 250.0;
const MIN_INTERVAL: Duration = Duration::from_millis(4);
const MOTION_START_MM: f64 = 0.5;
const SCROLL_UNITS_PER_MM: f64 = 24.0;
const SCROLL_START_MM: f64 = 1.0;
const SCROLL_AXIS_LOCK: f64 = 2.0;
const TAP_TIME: Duration = Duration::from_millis(180);
const TAP_TRAVEL_MM: f64 = 1.3;
const CLICK_SPREAD_MM: [f64; 2] = [40.0, 30.0];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TouchpadSettings {
    pub tap_to_click: bool,
    pub natural_scroll: bool,
}

impl Default for TouchpadSettings {
    fn default() -> Self {
        Self {
            tap_to_click: true,
            natural_scroll: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Finger {
    pub id: ContactId,
    pub x: i32,
    pub y: i32,
}

impl Finger {
    fn offset_from(self, other: Finger) -> [f64; 2] {
        [
            f64::from(self.x - other.x) / 100.0,
            f64::from(self.y - other.y) / 100.0,
        ]
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Kind {
    #[default]
    Still,
    Pointer,
    Scroll,
}

#[derive(Debug, Default)]
struct Gesture {
    kind: Kind,
    travel: [f64; 2],
    started: bool,
    axis: Option<usize>,
    remainder: [f64; 2],
}

impl Gesture {
    fn starts(&mut self, delta: [f64; 2], distance: f64) -> bool {
        if !self.started {
            self.travel = [self.travel[0] + delta[0], self.travel[1] + delta[1]];
            self.started = length(self.travel) >= distance;
        }
        self.started
    }

    fn whole(&mut self, amount: [f64; 2]) -> [i64; 2] {
        std::array::from_fn(|axis| {
            let total = self.remainder[axis] + amount[axis];
            let whole = total.trunc();
            self.remainder[axis] = total - whole;
            whole as i64
        })
    }
}

#[derive(Debug)]
struct Tap {
    landed: Duration,
    most: usize,
    origins: Vec<Finger>,
}

#[derive(Debug)]
pub struct TouchpadPointer {
    settings: TouchpadSettings,
    clickpad: bool,
    fingers: Vec<Finger>,
    count: usize,
    at: Duration,
    gesture: Gesture,
    tap: Option<Tap>,
    pressed: Option<PointerButton>,
}

impl TouchpadPointer {
    pub fn new(clickpad: bool, settings: TouchpadSettings) -> Self {
        Self {
            settings,
            clickpad,
            fingers: Vec::new(),
            count: 0,
            at: Duration::ZERO,
            gesture: Gesture::default(),
            tap: None,
            pressed: None,
        }
    }

    pub fn report(
        &mut self,
        fingers: &[Finger],
        count: usize,
        buttons: &[CaptureTransition],
        at: Duration,
    ) -> PointerFrame {
        let mut frame = PointerFrame::default();
        if self.buttons(fingers, count, buttons, &mut frame.transitions) {
            self.tap = None;
            self.gesture = Gesture::default();
        }
        self.tap(fingers, count, at, &mut frame.transitions);
        frame.motion = self.motion(fingers, count, at);
        self.fingers.clear();
        self.fingers.extend_from_slice(fingers);
        self.count = count;
        self.at = at;
        frame
    }

    fn buttons(
        &mut self,
        fingers: &[Finger],
        count: usize,
        buttons: &[CaptureTransition],
        out: &mut Vec<CaptureTransition>,
    ) -> bool {
        let mut changed = false;
        for &transition in buttons {
            let CaptureTransition::Button { button, state } = transition else {
                out.push(transition);
                continue;
            };
            changed = true;
            let button = if self.clickpad && button == PointerButton::PRIMARY {
                match state {
                    KeyState::Pressed => *self.pressed.insert(clickfinger(fingers, count)),
                    KeyState::Released => self.pressed.take().unwrap_or(button),
                    KeyState::Repeat => self.pressed.unwrap_or(button),
                }
            } else {
                button
            };
            out.push(CaptureTransition::Button { button, state });
        }
        changed
    }

    fn tap(
        &mut self,
        fingers: &[Finger],
        count: usize,
        at: Duration,
        out: &mut Vec<CaptureTransition>,
    ) {
        if self.settings.tap_to_click && self.pressed.is_none() && self.count == 0 && count > 0 {
            self.tap = Some(Tap {
                landed: at,
                most: 0,
                origins: Vec::new(),
            });
        }
        let Some(tap) = self.tap.as_mut() else {
            return;
        };
        tap.most = tap.most.max(count);
        let mut moved = false;
        for finger in fingers {
            match tap.origins.iter().find(|origin| origin.id == finger.id) {
                Some(origin) => moved |= length(finger.offset_from(*origin)) > TAP_TRAVEL_MM,
                None => tap.origins.push(*finger),
            }
        }
        if moved || at.saturating_sub(tap.landed) > TAP_TIME {
            self.tap = None;
        } else if count == 0 {
            let most = tap.most;
            self.tap = None;
            let button = match most {
                1 => PointerButton::PRIMARY,
                2 => PointerButton::SECONDARY,
                3 => PointerButton::MIDDLE,
                _ => return,
            };
            for state in [KeyState::Pressed, KeyState::Released] {
                out.push(CaptureTransition::Button { button, state });
            }
        }
    }

    fn motion(&mut self, fingers: &[Finger], count: usize, at: Duration) -> MotionDelta {
        let kind = match (count, fingers.len()) {
            (1, 1) => Kind::Pointer,
            (2, 2) if self.pressed.is_some() => Kind::Pointer,
            (2, 2) => Kind::Scroll,
            _ => Kind::Still,
        };
        let same = count == self.count
            && fingers.len() == self.fingers.len()
            && fingers
                .iter()
                .all(|finger| self.fingers.iter().any(|last| last.id == finger.id));
        if kind != self.gesture.kind || !same {
            self.gesture = Gesture {
                kind,
                ..Gesture::default()
            };
            return MotionDelta::default();
        }
        let moves = fingers.iter().filter_map(|finger| {
            self.fingers
                .iter()
                .find(|last| last.id == finger.id)
                .map(|last| finger.offset_from(*last))
        });
        match kind {
            Kind::Still => MotionDelta::default(),
            Kind::Pointer => {
                let delta = moves
                    .max_by(|left, right| length(*left).total_cmp(&length(*right)))
                    .unwrap_or_default();
                if !self.gesture.starts(delta, MOTION_START_MM) {
                    return MotionDelta::default();
                }
                let interval = at.saturating_sub(self.at).max(MIN_INTERVAL);
                let gain = counts_per_mm(length(delta) / interval.as_secs_f64());
                let [dx, dy] = self.gesture.whole(delta.map(|axis| axis * gain));
                MotionDelta {
                    dx,
                    dy,
                    ..MotionDelta::default()
                }
            }
            Kind::Scroll => {
                let mut delta = moves.fold([0.0; 2], |sum, delta| {
                    [sum[0] + delta[0] / 2.0, sum[1] + delta[1] / 2.0]
                });
                let starting = !self.gesture.started;
                if !self.gesture.starts(delta, SCROLL_START_MM) {
                    return MotionDelta::default();
                }
                if starting {
                    self.gesture.axis = locked_axis(self.gesture.travel);
                }
                if let Some(axis) = self.gesture.axis {
                    delta[1 - axis] = 0.0;
                }
                let direction = if self.settings.natural_scroll {
                    1.0
                } else {
                    -1.0
                };
                let [scroll_x, scroll_y] = self.gesture.whole([
                    -direction * delta[0] * SCROLL_UNITS_PER_MM,
                    direction * delta[1] * SCROLL_UNITS_PER_MM,
                ]);
                MotionDelta {
                    scroll_x,
                    scroll_y,
                    ..MotionDelta::default()
                }
            }
        }
    }
}

fn clickfinger(fingers: &[Finger], count: usize) -> PointerButton {
    let thumb = matches!(fingers, [left, right]
        if left.offset_from(*right)
            .iter()
            .zip(CLICK_SPREAD_MM)
            .any(|(offset, spread)| offset.abs() > spread));
    match count {
        0 | 1 => PointerButton::PRIMARY,
        2 if thumb => PointerButton::PRIMARY,
        2 => PointerButton::SECONDARY,
        _ => PointerButton::MIDDLE,
    }
}

fn counts_per_mm(speed: f64) -> f64 {
    let fast = ((speed - SLOW_MM_PER_S) / (FAST_MM_PER_S - SLOW_MM_PER_S)).clamp(0.0, 1.0);
    SLOW_COUNTS_PER_MM + fast * (FAST_COUNTS_PER_MM - SLOW_COUNTS_PER_MM)
}

fn locked_axis([x, y]: [f64; 2]) -> Option<usize> {
    if y.abs() >= SCROLL_AXIS_LOCK * x.abs() {
        Some(1)
    } else if x.abs() >= SCROLL_AXIS_LOCK * y.abs() {
        Some(0)
    } else {
        None
    }
}

fn length([x, y]: [f64; 2]) -> f64 {
    x.hypot(y)
}

#[cfg(test)]
mod tests {
    use evdev::{AbsoluteAxisCode, EventType, InputEvent, KeyCode, SynchronizationCode};

    use super::*;
    use crate::linux::{FrameAccumulator, TouchAccumulator, TouchAxisRange};

    struct Pad {
        contacts: TouchAccumulator,
        frames: FrameAccumulator,
        pointer: TouchpadPointer,
    }

    impl Pad {
        fn new(clickpad: bool, settings: TouchpadSettings) -> Self {
            Self::with_axes(
                TouchAxisRange::new(0, 1_000, 10).unwrap(),
                TouchAxisRange::new(0, 700, 10).unwrap(),
                clickpad,
                settings,
            )
        }

        fn with_axes(
            x: TouchAxisRange,
            y: TouchAxisRange,
            clickpad: bool,
            settings: TouchpadSettings,
        ) -> Self {
            Self {
                contacts: TouchAccumulator::new(x, y),
                frames: FrameAccumulator::default(),
                pointer: TouchpadPointer::new(clickpad, settings),
            }
        }

        fn report(&mut self, ms: u64, events: &[InputEvent]) -> PointerFrame {
            let mut frame = None;
            for &event in events.iter().chain([&syn()]) {
                self.contacts.push(event).unwrap();
                frame = self.frames.push(event).unwrap();
            }
            self.pointer.report(
                &self.contacts.fingers(),
                self.contacts.finger_count(),
                &frame.unwrap().transitions,
                Duration::from_millis(ms),
            )
        }
    }

    fn syn() -> InputEvent {
        InputEvent::new(
            EventType::SYNCHRONIZATION.0,
            SynchronizationCode::SYN_REPORT.0,
            0,
        )
    }

    fn abs(axis: AbsoluteAxisCode, value: i32) -> InputEvent {
        InputEvent::new(EventType::ABSOLUTE.0, axis.0, value)
    }

    fn key(key: KeyCode, value: i32) -> InputEvent {
        InputEvent::new(EventType::KEY.0, key.code(), value)
    }

    fn tool(fingers: usize, down: bool) -> InputEvent {
        let keys = [
            KeyCode::BTN_TOOL_FINGER,
            KeyCode::BTN_TOOL_DOUBLETAP,
            KeyCode::BTN_TOOL_TRIPLETAP,
        ];
        key(keys[fingers - 1], i32::from(down))
    }

    fn land(slot: i32, x: i32, y: i32) -> Vec<InputEvent> {
        vec![
            abs(AbsoluteAxisCode::ABS_MT_SLOT, slot),
            abs(AbsoluteAxisCode::ABS_MT_TRACKING_ID, 100 + slot),
            abs(AbsoluteAxisCode::ABS_MT_POSITION_X, x),
            abs(AbsoluteAxisCode::ABS_MT_POSITION_Y, y),
        ]
    }

    fn slide(slot: i32, x: i32, y: i32) -> Vec<InputEvent> {
        vec![
            abs(AbsoluteAxisCode::ABS_MT_SLOT, slot),
            abs(AbsoluteAxisCode::ABS_MT_POSITION_X, x),
            abs(AbsoluteAxisCode::ABS_MT_POSITION_Y, y),
        ]
    }

    fn lift(slot: i32) -> Vec<InputEvent> {
        vec![
            abs(AbsoluteAxisCode::ABS_MT_SLOT, slot),
            abs(AbsoluteAxisCode::ABS_MT_TRACKING_ID, -1),
        ]
    }

    fn one_finger_down(pad: &mut Pad, ms: u64, x: i32, y: i32) -> PointerFrame {
        let mut events = land(0, x, y);
        events.extend([key(KeyCode::BTN_TOUCH, 1), tool(1, true)]);
        pad.report(ms, &events)
    }

    fn two_fingers_down(pad: &mut Pad, ms: u64, y: i32) -> PointerFrame {
        let mut events = land(0, 400, y);
        events.extend(land(1, 600, y));
        events.extend([key(KeyCode::BTN_TOUCH, 1), tool(2, true)]);
        pad.report(ms, &events)
    }

    fn all_up(pad: &mut Pad, ms: u64, slots: &[i32], fingers: usize) -> PointerFrame {
        let mut events = slots
            .iter()
            .flat_map(|slot| lift(*slot))
            .collect::<Vec<_>>();
        events.extend([key(KeyCode::BTN_TOUCH, 0), tool(fingers, false)]);
        pad.report(ms, &events)
    }

    fn one_up(pad: &mut Pad, ms: u64) -> PointerFrame {
        let mut events = lift(1);
        events.extend([tool(2, false), tool(1, true)]);
        pad.report(ms, &events)
    }

    fn click(button: PointerButton) -> Vec<CaptureTransition> {
        [KeyState::Pressed, KeyState::Released]
            .map(|state| CaptureTransition::Button { button, state })
            .to_vec()
    }

    fn total(frames: impl IntoIterator<Item = PointerFrame>) -> MotionDelta {
        frames
            .into_iter()
            .fold(MotionDelta::default(), |sum, frame| MotionDelta {
                dx: sum.dx + frame.motion.dx,
                dy: sum.dy + frame.motion.dy,
                scroll_x: sum.scroll_x + frame.motion.scroll_x,
                scroll_y: sum.scroll_y + frame.motion.scroll_y,
            })
    }

    #[test]
    fn one_finger_moves_the_pointer_by_millimetres_with_mild_acceleration() {
        let mut pad = Pad::new(true, TouchpadSettings::default());
        assert_eq!(
            one_finger_down(&mut pad, 0, 300, 300),
            PointerFrame::default()
        );
        let slow = (1..=10).map(|step| {
            let step_units = step as i32;
            pad.report(
                step * 50,
                &slide(0, 300 + 10 * step_units, 300 + 5 * step_units),
            )
        });
        assert_eq!(
            total(slow),
            MotionDelta {
                dx: 80,
                dy: 40,
                ..MotionDelta::default()
            }
        );
        let fast =
            (1..=5).map(|step| pad.report(500 + step * 10, &slide(0, 400 - 30 * step as i32, 350)));
        assert_eq!(
            total(fast),
            MotionDelta {
                dx: -360,
                ..MotionDelta::default()
            }
        );
        assert!(all_up(&mut pad, 600, &[0], 1).transitions.is_empty());
    }

    #[test]
    fn a_pad_without_resolution_is_taken_as_100_mm_wide() {
        let mut pad = Pad::with_axes(
            TouchAxisRange::new(0, 2_000, 0).unwrap(),
            TouchAxisRange::new(0, 1_300, 0).unwrap(),
            true,
            TouchpadSettings::default(),
        );
        one_finger_down(&mut pad, 0, 1_000, 600);
        let moved =
            (1..=5).map(|step| pad.report(step * 50, &slide(0, 1_000, 600 + 20 * step as i32)));
        assert_eq!(
            total(moved),
            MotionDelta {
                dy: 40,
                ..MotionDelta::default()
            }
        );
    }

    #[test]
    fn a_resting_finger_does_not_nudge_the_pointer() {
        let mut pad = Pad::new(true, TouchpadSettings::default());
        one_finger_down(&mut pad, 0, 300, 300);
        let jitter = [(301, 299), (299, 302), (302, 300), (300, 301)];
        let frames = jitter
            .into_iter()
            .zip(1..)
            .map(|((x, y), step)| pad.report(step * 50, &slide(0, x, y)));
        assert_eq!(total(frames), MotionDelta::default());
    }

    #[test]
    fn a_quick_touch_is_a_click_and_a_long_or_moving_one_is_not() {
        let mut pad = Pad::new(true, TouchpadSettings::default());
        one_finger_down(&mut pad, 0, 300, 300);
        pad.report(60, &slide(0, 305, 302));
        assert_eq!(
            all_up(&mut pad, 120, &[0], 1).transitions,
            click(PointerButton::PRIMARY)
        );

        one_finger_down(&mut pad, 1_000, 300, 300);
        assert!(all_up(&mut pad, 1_300, &[0], 1).transitions.is_empty());

        one_finger_down(&mut pad, 2_000, 300, 300);
        pad.report(2_050, &slide(0, 330, 300));
        assert!(all_up(&mut pad, 2_100, &[0], 1).transitions.is_empty());

        let mut off = Pad::new(
            true,
            TouchpadSettings {
                tap_to_click: false,
                ..TouchpadSettings::default()
            },
        );
        one_finger_down(&mut off, 0, 300, 300);
        assert!(all_up(&mut off, 100, &[0], 1).transitions.is_empty());
    }

    #[test]
    fn a_two_finger_tap_is_a_right_click() {
        let mut pad = Pad::new(true, TouchpadSettings::default());
        one_finger_down(&mut pad, 0, 400, 300);
        let mut second = land(1, 600, 300);
        second.extend([tool(1, false), tool(2, true)]);
        pad.report(20, &second);
        assert_eq!(
            all_up(&mut pad, 140, &[0, 1], 2).transitions,
            click(PointerButton::SECONDARY)
        );
    }

    #[test]
    fn a_clickpad_press_clicks_by_how_many_fingers_press_it() {
        let mut pad = Pad::new(true, TouchpadSettings::default());
        let press = |pad: &mut Pad, ms, down| pad.report(ms, &[key(KeyCode::BTN_LEFT, down)]);
        let button = |button, state| vec![CaptureTransition::Button { button, state }];

        one_finger_down(&mut pad, 0, 300, 300);
        assert_eq!(
            press(&mut pad, 50, 1).transitions,
            button(PointerButton::PRIMARY, KeyState::Pressed)
        );
        assert_eq!(
            press(&mut pad, 300, 0).transitions,
            button(PointerButton::PRIMARY, KeyState::Released)
        );
        assert!(all_up(&mut pad, 320, &[0], 1).transitions.is_empty());

        two_fingers_down(&mut pad, 1_000, 300);
        assert_eq!(
            press(&mut pad, 1_050, 1).transitions,
            button(PointerButton::SECONDARY, KeyState::Pressed)
        );
        one_up(&mut pad, 1_100);
        assert_eq!(
            press(&mut pad, 1_150, 0).transitions,
            button(PointerButton::SECONDARY, KeyState::Released)
        );
        all_up(&mut pad, 1_200, &[0], 1);

        let mut thumb = land(0, 400, 200);
        thumb.extend(land(1, 420, 600));
        thumb.extend([key(KeyCode::BTN_TOUCH, 1), tool(2, true)]);
        pad.report(2_000, &thumb);
        assert_eq!(
            press(&mut pad, 2_050, 1).transitions,
            button(PointerButton::PRIMARY, KeyState::Pressed)
        );
    }

    #[test]
    fn dragging_on_a_pressed_clickpad_follows_the_moving_finger() {
        let mut pad = Pad::new(true, TouchpadSettings::default());
        let mut thumb = land(0, 400, 200);
        thumb.extend(land(1, 420, 600));
        thumb.extend([key(KeyCode::BTN_TOUCH, 1), tool(2, true)]);
        pad.report(0, &thumb);
        pad.report(50, &[key(KeyCode::BTN_LEFT, 1)]);
        let dragged =
            (1..=5).map(|step| pad.report(50 + step * 50, &slide(0, 400 + 10 * step as i32, 200)));
        assert_eq!(
            total(dragged),
            MotionDelta {
                dx: 40,
                ..MotionDelta::default()
            }
        );
    }

    #[test]
    fn other_buttons_and_keys_pass_through() {
        let mut pad = Pad::new(false, TouchpadSettings::default());
        two_fingers_down(&mut pad, 0, 300);
        let frame = pad.report(
            50,
            &[
                key(KeyCode::BTN_LEFT, 1),
                key(KeyCode::BTN_RIGHT, 1),
                key(KeyCode::KEY_A, 1),
            ],
        );
        assert_eq!(
            frame.transitions,
            [
                CaptureTransition::Button {
                    button: PointerButton::PRIMARY,
                    state: KeyState::Pressed,
                },
                CaptureTransition::Button {
                    button: PointerButton::SECONDARY,
                    state: KeyState::Pressed,
                },
                CaptureTransition::Key {
                    usage: crate::core::HidUsage::keyboard(0x04),
                    state: KeyState::Pressed,
                },
            ]
        );
    }

    #[test]
    fn two_fingers_scroll_naturally_on_one_axis() {
        let scroll = |settings, dx: i32, dy: i32| {
            let mut pad = Pad::new(true, settings);
            two_fingers_down(&mut pad, 0, 300);
            total((1..=5).map(|step: i32| {
                let mut events = slide(0, 400 + dx * step, 300 + dy * step);
                events.extend(slide(1, 600 + dx * step, 300 + dy * step));
                pad.report(step as u64 * 20, &events)
            }))
        };
        let natural = TouchpadSettings::default();
        assert_eq!(
            scroll(natural, 1, 10),
            MotionDelta {
                scroll_y: 120,
                ..MotionDelta::default()
            }
        );
        assert_eq!(
            scroll(natural, -10, 0),
            MotionDelta {
                scroll_x: 120,
                ..MotionDelta::default()
            }
        );
        let traditional = TouchpadSettings {
            natural_scroll: false,
            ..natural
        };
        assert_eq!(
            scroll(traditional, 1, 10),
            MotionDelta {
                scroll_y: -120,
                ..MotionDelta::default()
            }
        );
        assert_eq!(
            scroll(traditional, -10, 0),
            MotionDelta {
                scroll_x: -120,
                ..MotionDelta::default()
            }
        );
    }

    #[test]
    fn lifting_one_of_two_fingers_ends_the_scroll_without_a_jump() {
        let mut pad = Pad::new(true, TouchpadSettings::default());
        two_fingers_down(&mut pad, 0, 300);
        for step in 1..=3 {
            let mut events = slide(0, 400, 300 + 10 * step);
            events.extend(slide(1, 600, 300 + 10 * step));
            pad.report(step as u64 * 20, &events);
        }
        assert_eq!(one_up(&mut pad, 80).motion, MotionDelta::default());
        let frames = [
            pad.report(130, &slide(0, 403, 330)),
            pad.report(180, &slide(0, 413, 330)),
        ];
        assert_eq!(
            total(frames),
            MotionDelta {
                dx: 8,
                ..MotionDelta::default()
            }
        );
        assert!(all_up(&mut pad, 200, &[0], 1).transitions.is_empty());
    }

    #[test]
    fn palms_are_ignored() {
        let palm = abs(AbsoluteAxisCode::ABS_MT_TOOL_TYPE, 2);
        let mut pad = Pad::new(true, TouchpadSettings::default());
        let mut events = land(0, 300, 300);
        events.extend([palm, key(KeyCode::BTN_TOUCH, 1), tool(1, true)]);
        pad.report(0, &events);
        let moved =
            (1..=5).map(|step| pad.report(step * 50, &slide(0, 300 + 10 * step as i32, 300)));
        assert_eq!(total(moved), MotionDelta::default());

        let mut finger = land(1, 700, 300);
        finger.extend([tool(1, false), tool(2, true)]);
        pad.report(300, &finger);
        let moved =
            (1..=5).map(|step| pad.report(300 + step * 50, &slide(1, 700 + 10 * step as i32, 300)));
        assert_eq!(
            total(moved),
            MotionDelta {
                dx: 40,
                ..MotionDelta::default()
            }
        );

        let mut pad = Pad::new(true, TouchpadSettings::default());
        let mut events = land(0, 300, 300);
        events.extend([palm, key(KeyCode::BTN_TOUCH, 1), tool(1, true)]);
        pad.report(0, &events);
        assert!(all_up(&mut pad, 80, &[0], 1).transitions.is_empty());
    }

    #[test]
    fn three_fingers_only_tap_a_middle_click() {
        let mut pad = Pad::new(true, TouchpadSettings::default());
        let mut events = land(0, 300, 300);
        events.extend(land(1, 500, 300));
        events.extend(land(2, 700, 300));
        events.extend([key(KeyCode::BTN_TOUCH, 1), tool(3, true)]);
        pad.report(0, &events);
        assert_eq!(
            all_up(&mut pad, 100, &[0, 1, 2], 3).transitions,
            click(PointerButton::MIDDLE)
        );

        let mut pad = Pad::new(true, TouchpadSettings::default());
        two_fingers_down(&mut pad, 0, 300);
        pad.report(20, &[tool(2, false), tool(3, true)]);
        let frames = (1..=5).map(|step: i32| {
            let mut events = slide(0, 400, 300 + 10 * step);
            events.extend(slide(1, 600, 300 + 10 * step));
            pad.report(20 + step as u64 * 20, &events)
        });
        assert_eq!(total(frames), MotionDelta::default());
    }
}
