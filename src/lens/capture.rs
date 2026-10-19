//! Frames of the recorded monitor, received from Mutter over PipeWire in shared memory. Nothing
//! is copied: the newest buffer stays out of the stream while the encoder reads it in place, and
//! goes back to Mutter when a newer one arrives. Mutter's damage rectangles tell the encoder which
//! tiles to compare. A 4K frame is 33 MB; the encoder reads the few tiles it needs.

use crate::lens::tiles::Rect;
use anyhow::{Context, Result, anyhow, ensure};
use pipewire as pw;
use pw::spa;
use spa::pod::{ChoiceValue, Object, Pod, Property, Value};
use spa::utils::{Choice, ChoiceEnum, ChoiceFlags, Fraction, Id, Rectangle, SpaTypes};
use std::ops::Range;
use std::sync::{Arc, Mutex, MutexGuard};

/// Rectangles kept between two takes. Past this, comparing every tile is cheaper than the list.
const MAX_DAMAGE: usize = 128;
/// Damage rectangles Mutter can describe in one frame; it sends their bounding box beyond this.
const DAMAGE_SLOTS: usize = 32;
/// Mutter offers no larger monitor today; the range only has to contain what it offers.
const MAX_SIDE: u32 = 16384;

/// The latest frame of the recorded monitor, kept up to date by the PipeWire thread.
#[derive(Default)]
pub struct Screen {
    held: Option<Held>,
    pub width: usize,
    pub height: usize,
    /// Frames received so far.
    pub frames: u64,
    /// Changed since the last take; `None` means everything.
    damage: Option<Vec<Rect>>,
}

/// A buffer kept out of the stream while the encoder reads its mapped memory.
struct Held {
    buffer: *mut pw::sys::pw_buffer,
    data: *const u8,
    len: usize,
    stride: usize,
}

// SAFETY: `Held`'s pointers are read only under the screen's lock, and the PipeWire thread takes
// the buffer back under that lock before the stream lets go of its memory.
unsafe impl Send for Screen {}

fn contains(outer: Rect, inner: Rect) -> bool {
    outer.x <= inner.x
        && outer.y <= inner.y
        && inner.x + inner.width <= outer.x + outer.width
        && inner.y + inner.height <= outer.y + outer.height
}

impl Screen {
    /// The newest frame: `height` rows of BGRx or BGRA pixels, `stride` bytes apart, valid while
    /// the lock is held.
    pub fn frame(&self) -> Option<(&[u8], usize)> {
        let held = self.held.as_ref()?;
        // SAFETY: see `Held`; the slice lives no longer than the borrow of the locked screen.
        let bytes = unsafe { std::slice::from_raw_parts(held.data, held.len) };
        Some((bytes, held.stride))
    }

    /// What changed since the last call: `None` means assume everything (no damage metadata, a
    /// resize, or too many rectangles).
    pub fn take_damage(&mut self) -> Option<Vec<Rect>> {
        self.damage.replace(Vec::new())
    }

    /// Counts one frame of `size` and its damage, already cut to the frame.
    fn note(&mut self, (width, height): (usize, usize), damage: Option<&[Rect]>) {
        let whole = Rect {
            x: 0,
            y: 0,
            width,
            height,
        };
        let resized = (self.width, self.height) != (width, height);
        (self.width, self.height) = (width, height);
        self.add_damage(damage.filter(|rects| !resized && !rects.contains(&whole)));
        self.frames += 1;
    }

    /// Keeps `next` for the encoder and returns the buffer it replaces, for the stream.
    fn hold(
        &mut self,
        next: Held,
        size: (usize, usize),
        damage: Option<&[Rect]>,
    ) -> Option<*mut pw::sys::pw_buffer> {
        self.note(size, damage);
        self.held.replace(next).map(|held| held.buffer)
    }

    /// Gives up the held buffer, for the stream, before its memory goes away.
    fn release(&mut self) -> Option<*mut pw::sys::pw_buffer> {
        self.held.take().map(|held| held.buffer)
    }

    fn add_damage(&mut self, rects: Option<&[Rect]>) {
        let (Some(known), Some(rects)) = (self.damage.as_mut(), rects) else {
            self.damage = None;
            return;
        };
        for &rect in rects {
            if known.iter().any(|&outer| contains(outer, rect)) {
                continue;
            }
            if known.len() == MAX_DAMAGE {
                self.damage = None;
                return;
            }
            known.push(rect);
        }
    }
}

fn lock(screen: &Mutex<Screen>) -> MutexGuard<'_, Screen> {
    screen.lock().unwrap_or_else(|e| e.into_inner())
}

/// A damage region in frame pixels, cut to the frame. `None` when nothing of it is inside.
fn clamp(x: i32, y: i32, width: u32, height: u32, frame: (usize, usize)) -> Option<Rect> {
    let (left, top) = (i64::from(x).max(0), i64::from(y).max(0));
    let right = (i64::from(x) + i64::from(width)).min(frame.0 as i64);
    let bottom = (i64::from(y) + i64::from(height)).min(frame.1 as i64);
    (left < right && top < bottom).then(|| Rect {
        x: left as usize,
        y: top as usize,
        width: (right - left) as usize,
        height: (bottom - top) as usize,
    })
}

/// The bytes of a `width` x `height` frame inside a mapped buffer of `len` bytes, or `None` when
/// the chunk cannot hold one.
fn frame(
    len: usize,
    offset: usize,
    size: usize,
    stride: i32,
    width: usize,
    height: usize,
) -> Option<Range<usize>> {
    let stride = usize::try_from(stride).ok()?;
    let row = width.checked_mul(4)?;
    if width == 0 || height == 0 || stride < row {
        return None;
    }
    let needed = stride.checked_mul(height - 1)?.checked_add(row)?;
    let end = offset.checked_add(needed)?;
    (needed <= size && end <= len).then_some(offset..end)
}

/// Stream state on the PipeWire thread.
struct Frames {
    screen: Arc<Mutex<Screen>>,
    size: Option<(usize, usize)>,
    /// The next frame replaces the whole picture: the format changed or a frame was lost.
    whole: bool,
    first: bool,
    /// A bad buffer was reported for this format; the rest would only repeat it.
    warned: bool,
}

impl Frames {
    /// Takes one dequeued buffer. Returns whether the picture changed, and the buffer to give
    /// back to the stream: the one this replaced, or this one if it is not worth keeping.
    ///
    /// # Safety
    /// `raw` must come from this stream's `dequeue_raw_buffer`, with buffers mapped.
    unsafe fn receive(
        &mut self,
        raw: *mut pw::sys::pw_buffer,
    ) -> (bool, Option<*mut pw::sys::pw_buffer>) {
        let Some(size) = self.size else {
            return (false, Some(raw));
        };
        // SAFETY: the caller's contract.
        let Some((held, damage)) = (unsafe { self.inspect(raw, size) }) else {
            return (false, Some(raw));
        };
        let whole = std::mem::take(&mut self.whole);
        let damage = damage.filter(|_| !whole);
        let previous = lock(&self.screen).hold(held, size, damage.as_deref());
        (true, previous)
    }

    /// Reads a buffer's frame and damage from its SPA structures, or `None` to skip it.
    ///
    /// # Safety
    /// As for [`Self::receive`].
    unsafe fn inspect(
        &mut self,
        raw: *mut pw::sys::pw_buffer,
        size: (usize, usize),
    ) -> Option<(Held, Option<Vec<Rect>>)> {
        use spa::sys;
        // SAFETY: a dequeued buffer's structures stay valid until it is queued again.
        let buffer = unsafe { (*raw).buffer.as_ref()? };
        let mut damage = None;
        for index in 0..buffer.n_metas as usize {
            // SAFETY: `metas` holds `n_metas` entries.
            let meta = unsafe { &*buffer.metas.add(index) };
            if meta.data.is_null() {
                continue;
            }
            match meta.type_ {
                sys::SPA_META_Header => {
                    // SAFETY: a header meta holds one `spa_meta_header`.
                    let header = unsafe { &*meta.data.cast::<sys::spa_meta_header>() };
                    if header.flags & sys::SPA_META_HEADER_FLAG_CORRUPTED != 0 {
                        return None;
                    }
                }
                sys::SPA_META_VideoDamage => {
                    let count = meta.size as usize / std::mem::size_of::<sys::spa_meta_region>();
                    // SAFETY: a damage meta holds `size` bytes of regions.
                    let regions = unsafe {
                        std::slice::from_raw_parts(meta.data.cast::<sys::spa_meta_region>(), count)
                    };
                    damage = Some(
                        regions
                            .iter()
                            .filter_map(|r| {
                                let (at, extent) = (r.region.position, r.region.size);
                                clamp(at.x, at.y, extent.width, extent.height, size)
                            })
                            .collect::<Vec<_>>(),
                    );
                }
                _ => {}
            }
        }
        if buffer.n_datas == 0 {
            return None;
        }
        // SAFETY: `datas` holds `n_datas` entries, and each has a chunk.
        let (data, chunk) = unsafe {
            let data = &*buffer.datas;
            (data, data.chunk.as_ref()?)
        };
        // Mutter keeps the damage of a frame it could not record for the next one, and sends an
        // empty chunk for a cursor-only update.
        if chunk.size == 0 || chunk.flags & sys::SPA_CHUNK_FLAG_CORRUPTED as i32 != 0 {
            return None;
        }
        if data.data.is_null() {
            if !std::mem::replace(&mut self.warned, true) {
                tracing::warn!(kind = data.type_, "lens capture buffer is not mapped");
            }
            self.whole = true;
            return None;
        }
        let Some(range) = frame(
            data.maxsize as usize,
            chunk.offset as usize,
            chunk.size as usize,
            chunk.stride,
            size.0,
            size.1,
        ) else {
            if !std::mem::replace(&mut self.warned, true) {
                tracing::warn!(
                    len = data.maxsize,
                    offset = chunk.offset,
                    length = chunk.size,
                    stride = chunk.stride,
                    "lens capture frame does not fit its buffer"
                );
            }
            self.whole = true;
            return None;
        };
        if std::mem::take(&mut self.first) {
            tracing::debug!(
                kind = data.type_,
                stride = chunk.stride,
                offset = chunk.offset,
                length = chunk.size,
                damage = damage.as_ref().map(Vec::len),
                "lens capture first frame"
            );
        }
        let held = Held {
            buffer: raw,
            // SAFETY: `frame` checked the range lies inside the mapped `maxsize` bytes.
            data: unsafe { data.data.cast::<u8>().add(range.start) },
            len: range.len(),
            stride: chunk.stride as usize,
        };
        Some((held, damage))
    }
}

fn serialize(object: Object) -> Result<Vec<u8>> {
    let (cursor, _) = spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &Value::Object(object),
    )
    .map_err(|error| anyhow!("PipeWire parameter not serialized: {error:?}"))?;
    Ok(cursor.into_inner())
}

fn property(key: u32, value: Value) -> Property {
    Property::new(key, value)
}

fn int_choice(choice: ChoiceEnum<i32>) -> Value {
    Value::Choice(ChoiceValue::Int(Choice(ChoiceFlags::empty(), choice)))
}

/// Raw BGRx or BGRA of any size Mutter offers, at most `fps` frames a second. Leaving out the
/// DMA-BUF modifier makes Mutter fall back to shared memory.
fn format(fps: u32) -> Result<Vec<u8>> {
    use spa::param::format::{FormatProperties, MediaSubtype, MediaType};
    use spa::param::video::VideoFormat;
    let fps = Fraction { num: fps, denom: 1 };
    serialize(spa::pod::object!(
        SpaTypes::ObjectParamFormat,
        spa::param::ParamType::EnumFormat,
        spa::pod::property!(FormatProperties::MediaType, Id, MediaType::Video),
        spa::pod::property!(FormatProperties::MediaSubtype, Id, MediaSubtype::Raw),
        spa::pod::property!(
            FormatProperties::VideoFormat,
            Choice,
            Enum,
            Id,
            VideoFormat::BGRx,
            VideoFormat::BGRx,
            VideoFormat::BGRA,
        ),
        spa::pod::property!(
            FormatProperties::VideoSize,
            Choice,
            Range,
            Rectangle,
            Rectangle {
                width: 1920,
                height: 1080
            },
            Rectangle {
                width: 1,
                height: 1
            },
            Rectangle {
                width: MAX_SIDE,
                height: MAX_SIDE
            }
        ),
        spa::pod::property!(
            FormatProperties::VideoFramerate,
            Choice,
            Range,
            Fraction,
            Fraction { num: 0, denom: 1 },
            Fraction { num: 0, denom: 1 },
            Fraction {
                num: 1000,
                denom: 1
            }
        ),
        spa::pod::property!(
            FormatProperties::VideoMaxFramerate,
            Choice,
            Range,
            Fraction,
            fps,
            Fraction { num: 0, denom: 1 },
            fps
        ),
    ))
}

/// Shared memory buffers with a header and up to Mutter's 32 damage rectangles.
fn buffer_params() -> Result<Vec<Vec<u8>>> {
    use spa::sys;
    let memory = (1 << sys::SPA_DATA_MemFd) | (1 << sys::SPA_DATA_MemPtr);
    let meta = |kind: u32, size: Value| Object {
        type_: SpaTypes::ObjectParamMeta.as_raw(),
        id: spa::param::ParamType::Meta.as_raw(),
        properties: vec![
            property(sys::SPA_PARAM_META_type, Value::Id(Id(kind))),
            property(sys::SPA_PARAM_META_size, size),
        ],
    };
    let region = std::mem::size_of::<sys::spa_meta_region>() as i32;
    [
        Object {
            type_: SpaTypes::ObjectParamBuffers.as_raw(),
            id: spa::param::ParamType::Buffers.as_raw(),
            properties: vec![property(
                sys::SPA_PARAM_BUFFERS_dataType,
                int_choice(ChoiceEnum::Flags {
                    default: memory,
                    flags: Vec::new(),
                }),
            )],
        },
        meta(
            sys::SPA_META_Header,
            Value::Int(std::mem::size_of::<sys::spa_meta_header>() as i32),
        ),
        meta(
            sys::SPA_META_VideoDamage,
            int_choice(ChoiceEnum::Range {
                default: region * DAMAGE_SLOTS as i32,
                min: region,
                max: region * DAMAGE_SLOTS as i32,
            }),
        ),
    ]
    .into_iter()
    .map(serialize)
    .collect()
}

fn pods(bytes: &[Vec<u8>]) -> Result<Vec<&Pod>> {
    bytes
        .iter()
        .map(|bytes| Pod::from_bytes(bytes).context("PipeWire parameter not readable"))
        .collect()
}

/// Receives a PipeWire stream until `quit` fires or the stream ends. Sends `Ok` on `ready` once
/// connected; an error returned before that is the caller's to report.
fn run(
    node: u32,
    fps: u32,
    screen: Arc<Mutex<Screen>>,
    wake: Arc<dyn Fn() + Send + Sync>,
    quit: pw::channel::Receiver<()>,
    ready: &std::sync::mpsc::Sender<Result<()>>,
) -> Result<()> {
    pw::init();
    let main = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&main, None)?;
    let core = context
        .connect_rc(None)
        .context("PipeWire is not running")?;
    let stream = pw::stream::StreamBox::new(
        &core,
        "zflow-lens",
        pw::properties::properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Screen",
            *pw::keys::NODE_DONT_RECONNECT => "true",
            // Without this a vanished node could be replaced by the default camera.
            "node.dont-fallback" => "true",
        },
    )?;
    let frames = Frames {
        screen: screen.clone(),
        size: None,
        whole: true,
        first: true,
        warned: false,
    };
    let ended = main.downgrade();
    let _listener = stream
        .add_local_listener_with_user_data(frames)
        .state_changed(move |_, _, old, new| {
            tracing::debug!(?old, ?new, "lens capture stream state");
            if let pw::stream::StreamState::Error(error) = &new {
                tracing::warn!(%error, "lens capture stream failed");
            }
            if matches!(
                new,
                pw::stream::StreamState::Error(_) | pw::stream::StreamState::Unconnected
            ) && let Some(main) = ended.upgrade()
            {
                main.quit();
            }
        })
        .param_changed(|stream, frames, id, param| {
            let Some(param) = param else {
                return;
            };
            if id != spa::param::ParamType::Format.as_raw() {
                return;
            }
            let mut info = spa::param::video::VideoInfoRaw::new();
            if let Err(error) = info.parse(param) {
                tracing::warn!(%error, "lens capture format not readable");
                return;
            }
            let size = info.size();
            let max = info.max_framerate();
            tracing::info!(
                width = size.width,
                height = size.height,
                format = ?info.format(),
                max_fps = %format_args!("{}/{}", max.num, max.denom),
                "lens capture format"
            );
            frames.size = Some((size.width as usize, size.height as usize));
            frames.whole = true;
            if let Some(old) = lock(&frames.screen).release() {
                // SAFETY: a buffer this stream dequeued and the screen held.
                unsafe { stream.queue_raw_buffer(old) };
            }
            frames.first = true;
            frames.warned = false;
            let result = buffer_params()
                .and_then(|bytes| Ok(stream.update_params(&mut pods(&bytes)?)?));
            if let Err(error) = result {
                tracing::warn!(error = %format_args!("{error:#}"), "lens capture buffers not requested");
            }
        })
        .process(move |stream, frames| {
            let mut changed = false;
            // Every buffer is taken in order: each one's damage is relative to the one before.
            loop {
                // SAFETY: the buffer is held by the screen or queued again below.
                let raw = unsafe { stream.dequeue_raw_buffer() };
                if raw.is_null() {
                    break;
                }
                // SAFETY: just dequeued from this stream, which maps its buffers.
                let (landed, back) = unsafe { frames.receive(raw) };
                changed |= landed;
                if let Some(back) = back {
                    // SAFETY: dequeued from this stream and no longer read.
                    unsafe { stream.queue_raw_buffer(back) };
                }
            }
            if changed {
                wake();
            }
        })
        .register()?;
    let format = format(fps)?;
    stream.connect(
        spa::utils::Direction::Input,
        Some(node),
        pw::stream::StreamFlags::AUTOCONNECT
            | pw::stream::StreamFlags::MAP_BUFFERS
            | pw::stream::StreamFlags::DONT_RECONNECT,
        &mut pods(std::slice::from_ref(&format))?,
    )?;
    let stopper = main.clone();
    let _quit = quit.attach(main.loop_(), move |()| stopper.quit());
    let _ = ready.send(Ok(()));
    main.run();
    // The encoder must stop reading the held buffer before the stream frees its memory.
    if let Some(old) = lock(&screen).release() {
        // SAFETY: a buffer this stream dequeued and the screen held.
        unsafe { stream.queue_raw_buffer(old) };
    }
    Ok(())
}

/// A PipeWire thread keeping a [`Screen`] on the recorded monitor's newest frame.
pub struct Capture {
    quit: pw::channel::Sender<()>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Capture {
    /// Connects to `node` and keeps `screen` current, calling `wake` after each frame lands and
    /// once more when the stream ends.
    pub fn start(
        node: u32,
        fps: u32,
        screen: Arc<Mutex<Screen>>,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Capture> {
        ensure!(fps > 0, "capture needs a frame rate");
        let (quit, stop) = pw::channel::channel();
        let (ready, started) = std::sync::mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("zflow-lens-capture".into())
            .spawn(move || {
                let result = run(node, fps, screen, wake.clone(), stop, &ready);
                if let Err(error) = result {
                    let _ = ready.send(Err(error));
                }
                tracing::debug!(node, "lens capture ended");
                wake();
            })?;
        match started.recv() {
            Ok(Ok(())) => Ok(Capture {
                quit,
                thread: Some(thread),
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error.context("PipeWire capture not started"))
            }
            Err(_) => {
                let _ = thread.join();
                Err(anyhow!("the PipeWire capture thread stopped"))
            }
        }
    }

    /// Whether the stream ended, for example because Mutter stopped recording.
    pub fn is_finished(&self) -> bool {
        self.thread
            .as_ref()
            .is_none_or(|thread| thread.is_finished())
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.quit.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
impl Screen {
    /// A screen showing `pixels`, as if the stream had delivered them.
    pub(super) fn showing(pixels: &'static [u8], width: usize, height: usize) -> Self {
        let mut screen = Self::default();
        let held = Held {
            buffer: std::ptr::null_mut(),
            data: pixels.as_ptr(),
            len: pixels.len(),
            stride: width * 4,
        };
        screen.hold(held, (width, height), None);
        screen
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: usize, y: usize, width: usize, height: usize) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn damage_from_two_frames_is_merged_until_taken() {
        let mut screen = Screen::default();
        screen.note((8, 4), None);
        assert_eq!(screen.take_damage(), None);
        let first = [rect(0, 0, 2, 2)];
        let second = [rect(4, 1, 3, 3), rect(0, 0, 1, 1)];
        screen.note((8, 4), Some(&first));
        screen.note((8, 4), Some(&second));
        assert_eq!(
            screen.take_damage(),
            Some(vec![rect(0, 0, 2, 2), rect(4, 1, 3, 3)])
        );
        assert_eq!(screen.take_damage(), Some(Vec::new()));
        assert_eq!(screen.frames, 3);
    }

    #[test]
    fn damage_past_the_limit_collapses_to_everything() {
        let mut screen = Screen::default();
        screen.note((512, 4), None);
        screen.take_damage();
        let many: Vec<Rect> = (0..MAX_DAMAGE + 1).map(|x| rect(x, 0, 1, 1)).collect();
        screen.note((512, 4), Some(&many[..32]));
        assert_eq!(screen.take_damage().map(|rects| rects.len()), Some(32));
        for chunk in many.chunks(32) {
            screen.note((512, 4), Some(chunk));
        }
        assert_eq!(screen.take_damage(), None);
    }

    #[test]
    fn a_rectangle_covering_the_frame_or_a_resize_means_everything() {
        let mut screen = Screen::default();
        screen.note((4, 4), None);
        screen.take_damage();
        screen.note((4, 4), Some(&[rect(1, 1, 1, 1), rect(0, 0, 4, 4)]));
        assert_eq!(screen.take_damage(), None);
        screen.note((3, 3), Some(&[rect(0, 0, 1, 1)]));
        assert_eq!((screen.width, screen.height), (3, 3));
        assert_eq!(screen.take_damage(), None);
    }

    #[test]
    fn the_newest_frame_is_read_in_place_and_the_one_it_replaces_goes_back() {
        let (older, newer) = (vec![1_u8; 64], vec![2_u8; 64]);
        let tag = |n: usize| n as *mut pw::sys::pw_buffer;
        let held = |bytes: &Vec<u8>, n| Held {
            buffer: tag(n),
            data: bytes.as_ptr(),
            len: bytes.len(),
            stride: 16,
        };
        let mut screen = Screen::default();
        assert!(screen.frame().is_none());
        assert_eq!(screen.hold(held(&older, 1), (4, 4), None), None);
        assert_eq!(
            screen.frame().map(|(bytes, stride)| (bytes[0], stride)),
            Some((1, 16))
        );
        assert_eq!(screen.hold(held(&newer, 2), (4, 4), None), Some(tag(1)));
        assert_eq!(screen.frame().map(|(bytes, _)| bytes[63]), Some(2));
        assert_eq!(screen.release(), Some(tag(2)));
        assert!(screen.frame().is_none());
        assert_eq!(screen.frames, 2);
    }

    #[test]
    fn regions_are_cut_to_the_frame() {
        assert_eq!(clamp(-5, -5, 10, 10, (8, 8)), Some(rect(0, 0, 5, 5)));
        assert_eq!(clamp(6, 7, 10, 10, (8, 8)), Some(rect(6, 7, 2, 1)));
        assert_eq!(clamp(8, 0, 4, 4, (8, 8)), None);
        assert_eq!(clamp(0, 0, 0, 4, (8, 8)), None);
        assert_eq!(clamp(i32::MAX, 0, u32::MAX, 1, (8, 8)), None);
    }

    #[test]
    fn a_frame_must_fit_its_chunk_and_buffer() {
        // 4x2 pixels, stride 20, starting 8 bytes in: the last row ends at 8 + 20 + 16.
        assert_eq!(frame(64, 8, 56, 20, 4, 2), Some(8..44));
        assert_eq!(frame(40, 8, 56, 20, 4, 2), None);
        assert_eq!(frame(64, 8, 30, 20, 4, 2), None);
        assert_eq!(frame(64, 0, 64, 12, 4, 2), None);
        assert_eq!(frame(64, 0, 64, -16, 4, 2), None);
        assert_eq!(frame(64, 0, 64, 16, 0, 2), None);
    }

    #[test]
    fn capture_parameters_serialize_into_pods() {
        let format = format(30).unwrap();
        let buffers = buffer_params().unwrap();
        assert_eq!(pods(std::slice::from_ref(&format)).unwrap().len(), 1);
        assert_eq!(pods(&buffers).unwrap().len(), 3);
    }

    async fn exercise(desk: &crate::lens::mutter::Desk) -> Result<()> {
        let screen = Arc::new(Mutex::new(Screen::default()));
        let landed = Arc::new(tokio::sync::Notify::new());
        let wake: Arc<dyn Fn() + Send + Sync> = {
            let landed = landed.clone();
            Arc::new(move || landed.notify_one())
        };
        let capture = Capture::start(desk.node(), 30, screen.clone(), wake)?;
        let frames = |screen: &Mutex<Screen>| lock(screen).frames;
        let started = std::time::Instant::now();
        while frames(&screen) < 1 {
            ensure!(!capture.is_finished(), "capture ended before a frame");
            ensure!(started.elapsed().as_secs() < 5, "no frame within 5 s");
            let _ = tokio::time::timeout(std::time::Duration::from_millis(200), landed.notified())
                .await;
        }
        let first = started.elapsed();
        let (width, height) = {
            let mut screen = lock(&screen);
            ensure!(
                screen.take_damage().is_none(),
                "the first frame should be whole"
            );
            (screen.width, screen.height)
        };
        println!("first frame after {first:?}: {width}x{height}");
        ensure!((640..=8192).contains(&width) && (480..=8192).contains(&height));
        ensure!(
            lock(&screen)
                .frame()
                .is_some_and(|(bytes, stride)| stride >= width * 4
                    && bytes.len() >= stride * (height - 1) + width * 4)
        );

        // Takes damage every 16 ms like an encoder would, and sums up what it saw.
        let watch = async |moving: bool| -> Result<String> {
            let (x, y) = (width as f64 / 2., height as f64 / 2.);
            let started = std::time::Instant::now();
            let before = frames(&screen);
            let (mut whole, mut rects, mut area) = (0, 0, 0);
            for step in 0..60 {
                if moving {
                    let angle = step as f64 / 60. * std::f64::consts::TAU;
                    desk.pointer(x + 200. * angle.cos(), y + 200. * angle.sin())
                        .await?;
                }
                tokio::time::sleep(std::time::Duration::from_millis(16)).await;
                match lock(&screen).take_damage() {
                    Some(taken) => {
                        rects += taken.len();
                        area += taken.iter().map(|r| r.width * r.height).sum::<usize>();
                    }
                    None => whole += 1,
                }
            }
            let applied = frames(&screen) - before;
            let elapsed = started.elapsed().as_secs_f64();
            Ok(format!(
                "{applied} frames in {elapsed:.2} s ({:.1} fps), {whole} whole takes, {rects} rects covering {area} px",
                applied as f64 / elapsed
            ))
        };
        println!("pointer circling: {}", watch(true).await?);
        println!("pointer still: {}", watch(false).await?);
        desk.pointer(width as f64 / 2., height as f64 / 2.).await?;
        let generation = desk.selection_generation();
        println!("selection generation {generation}");
        if generation > 0 {
            let read = desk
                .read_selection(generation - 1, std::time::Duration::from_secs(1))
                .await;
            match read {
                Ok(text) => println!("selection read: {} bytes of text", text.len()),
                Err(error) => println!("selection not read: {error:#}"),
            }
        }
        drop(capture);
        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires a GNOME session and PipeWire"]
    async fn a_live_session_records_the_monitor_and_takes_the_pointer() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter("zflow=debug")
            .with_test_writer()
            .try_init();
        let bus = zbus::Connection::session().await.unwrap();
        let desk = crate::lens::mutter::Desk::start(&bus).await.unwrap();
        let result = exercise(&desk).await;
        desk.stop().await.unwrap();
        result.unwrap();
    }
}
