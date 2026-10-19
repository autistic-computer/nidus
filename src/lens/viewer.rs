//! The viewing side of a lens as the UI sees it: the other computer's pixels as they arrive,
//! which chunks of them changed, and input headed back. The UI reads chunks and sends input from
//! its own thread. Decoding happens elsewhere and only the changed tiles are copied in here, so
//! the UI never waits behind a decode, and input never waits behind pixels.

use std::sync::Mutex;

use tokio::sync::mpsc;

use super::tiles::TILE;
use crate::core::HidUsage;

/// The UI copies pixels in squares this size. 64 divides it, so a tile never straddles two.
pub const CHUNK: usize = 256;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum State {
    #[default]
    Closed = 0,
    Opening = 1,
    Open = 2,
    Failed = 3,
}

/// Input from the person looking through the lens. Positions are the viewed computer's capture
/// pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Input {
    Move { x: f64, y: f64, radius: f64 },
    Button { button: u8, pressed: bool },
    Key { usage: HidUsage, pressed: bool },
    Scroll { dx: f64, dy: f64 },
    Copy,
    Close,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Status {
    pub state: State,
    pub width: u32,
    pub height: u32,
    pub frames: u64,
}

/// The desktop as decoded so far, and the chunks the UI has not copied since they changed.
#[derive(Default)]
struct Shown {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
    dirty: Vec<bool>,
}

#[derive(Default)]
pub struct Viewer {
    input: Mutex<Option<mpsc::UnboundedSender<Input>>>,
    status: Mutex<(Status, Option<String>)>,
    shown: Mutex<Shown>,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

impl Viewer {
    pub fn status(&self) -> Status {
        lock(&self.status).0
    }

    pub fn error(&self) -> Option<String> {
        lock(&self.status).1.clone()
    }

    /// Writes into `out` the chunks under a circle that changed since the UI last copied them,
    /// and returns how many it wrote. Changed chunks outside the circle wait until it gets there.
    pub fn dirty(&self, x: f64, y: f64, radius: f64, out: &mut [u32]) -> usize {
        let shown = lock(&self.shown);
        if shown.width == 0 {
            return 0;
        }
        let columns = shown.width.div_ceil(CHUNK);
        let span = |center: f64, size: usize| {
            let low = (center - radius).max(0.) as usize / CHUNK;
            let high = ((center + radius).max(0.) as usize).min(size - 1) / CHUNK;
            low..=high
        };
        let mut count = 0;
        for row in span(y, shown.height) {
            for column in span(x, shown.width) {
                let id = row * columns + column;
                if count < out.len() && shown.dirty.get(id).copied().unwrap_or(false) {
                    out[count] = id as u32;
                    count += 1;
                }
            }
        }
        count
    }

    /// Copies one chunk into `pixels`, rows `stride` bytes apart, and marks it clean. Returns the
    /// chunk's size, which is smaller than [`CHUNK`] at the right and bottom edges.
    pub fn chunk(&self, id: usize, pixels: &mut [u8], stride: usize) -> Option<(usize, usize)> {
        let mut shown = lock(&self.shown);
        let (width, height) = (shown.width, shown.height);
        let columns = width.div_ceil(CHUNK);
        if columns == 0 {
            return None;
        }
        let (x, y) = ((id % columns) * CHUNK, (id / columns) * CHUNK);
        if y >= height {
            return None;
        }
        let (w, h) = (CHUNK.min(width - x), CHUNK.min(height - y));
        if stride < w * 4 || pixels.len() < stride * (h - 1) + w * 4 {
            return None;
        }
        for row in 0..h {
            let from = ((y + row) * width + x) * 4;
            pixels[row * stride..row * stride + w * 4]
                .copy_from_slice(&shown.pixels[from..from + w * 4]);
        }
        shown.dirty[id] = false;
        Some((w, h))
    }

    /// Queues input for the viewed computer. Input before the lens opens is dropped.
    pub fn send(&self, input: Input) {
        if let Some(sender) = &*lock(&self.input) {
            let _ = sender.send(input);
        }
    }

    pub(crate) fn opening(&self, input: mpsc::UnboundedSender<Input>) {
        *lock(&self.shown) = Shown::default();
        *lock(&self.status) = (
            Status {
                state: State::Opening,
                ..Status::default()
            },
            None,
        );
        *lock(&self.input) = Some(input);
    }

    /// The viewed computer's desktop size, before its tiles.
    pub(crate) fn ready(&self, width: usize, height: usize) {
        *lock(&self.shown) = Shown {
            width,
            height,
            pixels: vec![0; width * height * 4],
            dirty: vec![false; width.div_ceil(CHUNK) * height.div_ceil(CHUNK)],
        };
        lock(&self.status).0 = Status {
            state: State::Open,
            width: width as u32,
            height: height as u32,
            frames: 0,
        };
    }

    /// Copies the tiles a frame changed from the decoder's desktop, and marks their chunks.
    pub(crate) fn show(&self, desktop: &[u8], changed: &[(usize, usize, usize, usize)]) {
        {
            let mut shown = lock(&self.shown);
            let shown = &mut *shown;
            let columns = shown.width.div_ceil(CHUNK);
            if desktop.len() != shown.pixels.len() {
                return;
            }
            for &(x, y, w, h) in changed {
                for row in y..y + h {
                    let at = (row * shown.width + x) * 4;
                    shown.pixels[at..at + w * 4].copy_from_slice(&desktop[at..at + w * 4]);
                }
                debug_assert_eq!(CHUNK % TILE, 0);
                shown.dirty[y / CHUNK * columns + x / CHUNK] = true;
            }
        }
        lock(&self.status).0.frames += 1;
    }

    pub(crate) fn finish(&self, error: Option<String>) {
        *lock(&self.input) = None;
        let mut status = lock(&self.status);
        status.0.state = if error.is_some() {
            State::Failed
        } else {
            State::Closed
        };
        status.1 = error;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lens::tiles::{Budget, Decoder, Encoder};
    use anyhow::Result;

    #[test]
    fn chunks_under_the_lens_report_once_until_they_change_again() -> Result<()> {
        let (width, height) = (600, 300);
        let viewer = Viewer::default();
        viewer.ready(width, height);
        let mut encoder = Encoder::new(width, height)?;
        let mut decoder = Decoder::new(width, height)?;
        let pixels = vec![9u8; width * height * 4];
        let budget = Budget {
            lens: usize::MAX,
            fill: usize::MAX,
        };
        let packet = encoder.encode(&pixels, width * 4, None, budget, 0)?;
        let changed = decoder.apply(&packet.body.unwrap())?;
        viewer.show(&decoder.pixels, &changed);
        let mut ids = [0u32; 16];
        let found = viewer.dirty(100., 100., 50., &mut ids);
        assert_eq!(&ids[..found], &[0]);
        let mut chunk = vec![0u8; CHUNK * CHUNK * 4];
        assert_eq!(viewer.chunk(0, &mut chunk, CHUNK * 4), Some((CHUNK, CHUNK)));
        assert!(chunk.iter().all(|b| *b == 9));
        assert_eq!(viewer.dirty(100., 100., 50., &mut ids), 0);
        // The far edge chunk is narrower, and still waiting because the lens never went there.
        let found = viewer.dirty(590., 290., 20., &mut ids);
        assert_eq!(&ids[..found], &[5]);
        assert_eq!(viewer.chunk(5, &mut chunk, CHUNK * 4), Some((88, 44)));
        assert_eq!(viewer.status().frames, 1);
        Ok(())
    }

    #[test]
    fn input_still_flows_while_pixels_are_locked() {
        let viewer = Viewer::default();
        let (sender, mut receiver) = mpsc::unbounded_channel();
        viewer.opening(sender);
        let _decoding = lock(&viewer.shown);
        viewer.send(Input::Copy);
        assert_eq!(receiver.try_recv().unwrap(), Input::Copy);
        assert_eq!(viewer.status().state, State::Opening);
    }
}
