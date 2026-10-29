use std::{path::PathBuf, time::Instant};

use crate::core::{HidUsage, MotionDelta, PointerButton, TouchState};

pub const MAX_TOUCHPAD_CONTACTS: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyState {
    Released,
    Pressed,
    Repeat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureTransition {
    Key {
        usage: HidUsage,
        state: KeyState,
    },
    Button {
        button: PointerButton,
        state: KeyState,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CaptureFrame {
    pub transitions: Vec<CaptureTransition>,
    pub motion: MotionDelta,
    pub touch_snapshot: Option<TouchState>,
    pub as_pointer: Option<PointerFrame>,
    pub event_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PointerFrame {
    pub transitions: Vec<CaptureTransition>,
    pub motion: MotionDelta,
}

impl CaptureFrame {
    pub fn is_empty(&self) -> bool {
        self.transitions.is_empty()
            && self.motion == MotionDelta::default()
            && self.touch_snapshot.is_none()
            && self.as_pointer.as_ref().is_none_or(|pointer| {
                pointer.transitions.is_empty() && pointer.motion == MotionDelta::default()
            })
    }

    pub fn for_target(&mut self, takes_contacts: bool) {
        if let Some(pointer) = self.as_pointer.take()
            && !takes_contacts
        {
            self.transitions = pointer.transitions;
            self.motion = pointer.motion;
            self.touch_snapshot = None;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedDeviceFrame {
    pub device_path: PathBuf,
    pub frame: CaptureFrame,
    pub captured_at: Instant,
}
