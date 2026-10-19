//! Being looked at through a lens. The desktop agent records this screen through a Mutter remote
//! desktop session, sends what the viewer's budget allows, and plays the viewer's pointer,
//! buttons, keys and scroll through the same session, so they land where the viewer sees them.
//!
//! Three things run at once: a reader that takes acknowledgements and moves the moment they
//! arrive, an encoder thread that spends the budget, and an input task that waits on Mutter in
//! order. None of them waits on another.

use std::{
    sync::{Arc, Condvar, Mutex, atomic::Ordering},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use tokio::{net::UnixStream, sync::mpsc};

use super::{
    capture::{Capture, Screen},
    mutter::Desk,
    pace::Pacer,
    spread,
    tiles::{self, Budget, Encoder},
    wire::{LensMessage, read, write},
};
use crate::{
    app::desktop::{FOCUS_KNOWN, FOCUSED_TERMINAL},
    core::{HidUsage, KeyRemap, PointerButton},
    linux::{hid_to_evdev_key, pointer_button_to_evdev},
    peer_view::LensOpened,
};

/// Mutter records only when pixels change, at most this often.
const FPS: u32 = 60;
/// A frame goes out at least this often, so the viewer can tell the lens is alive.
const RECEIPT_EVERY: Duration = Duration::from_secs(1);
/// How long a copy waits for the focused app to take the selection.
const COPY_TIMEOUT: Duration = Duration::from_secs(2);
const LEFT_CONTROL: HidUsage = HidUsage::keyboard(0xe0);
const LEFT_SHIFT: HidUsage = HidUsage::keyboard(0xe1);
const KEY_C: HidUsage = HidUsage::keyboard(0x06);

/// Where the viewer looks, and whether the encoder has something to do.
#[derive(Default)]
struct Work {
    view: Option<(tiles::View, u64)>,
    woken: bool,
    stop: bool,
}

type Shared = Arc<(Mutex<Work>, Condvar)>;

/// What a lens sent, for the line logged when it closes.
#[derive(Debug, Default)]
struct Sent {
    frames: u64,
    bytes: usize,
    /// From the start to the desktop size going out.
    first: Option<Duration>,
    /// Each frame's encoding time and the window it was sent under.
    encode_ns: Vec<u64>,
    windows: Vec<u64>,
}

fn wake(work: &Shared, update: impl FnOnce(&mut Work)) {
    let (lock, ready) = &**work;
    let mut work = lock.lock().unwrap_or_else(|e| e.into_inner());
    update(&mut work);
    work.woken = true;
    ready.notify_one();
}

enum Action {
    Pointer(f64, f64),
    Button(u8, bool),
    Key(HidUsage, bool),
    Scroll(i32, i32),
    Copy,
}

/// Serves one lens on the agent's end of the relay until either side stops.
pub async fn serve(stream: UnixStream, opened: LensOpened, bus: &zbus::Connection) -> Result<()> {
    let desk = Arc::new(
        Desk::start(bus)
            .await
            .context("start the remote desktop session")?,
    );
    let result = run(stream, &opened, desk.clone()).await;
    if let Err(error) = desk.release().await {
        tracing::debug!(%error, "lens could not release its keys");
    }
    if let Some(desk) = Arc::into_inner(desk)
        && let Err(error) = desk.stop().await
    {
        tracing::debug!(%error, "lens session did not stop cleanly");
    }
    result
}

async fn run(stream: UnixStream, opened: &LensOpened, desk: Arc<Desk>) -> Result<()> {
    let (mut receive, mut send) = stream.into_split();
    let work: Shared = Arc::default();
    let pacer = Arc::new(Mutex::new(Pacer::default()));
    let screen = Arc::new(Mutex::new(Screen::default()));
    let (out, mut outgoing) = mpsc::unbounded_channel::<(LensMessage, Option<Vec<u8>>)>();

    let woken = work.clone();
    let capture = Capture::start(
        desk.node(),
        FPS,
        screen.clone(),
        Arc::new(move || wake(&woken, |_| {})),
    )?;
    let encoder = {
        let (work, pacer, screen, out) = (work.clone(), pacer.clone(), screen.clone(), out.clone());
        std::thread::Builder::new()
            .name("zflow-lens".into())
            .spawn(move || {
                let mut sent = Sent::default();
                encode(&work, &pacer, &screen, &out, &mut sent);
                sent
            })?
    };
    let _stop = StopOnDrop(work.clone());

    let writer = async {
        while let Some((message, body)) = outgoing.recv().await {
            write(&mut send, message, body.as_deref()).await?;
        }
        anyhow::Ok(())
    };
    let (actions, pending) = mpsc::unbounded_channel();
    let player = play(desk.clone(), opened, pending, out.clone());
    let mut rtts = Vec::new();
    let reader = async {
        while let Some((message, _)) = read(&mut receive).await? {
            match message {
                LensMessage::Move { x, y, radius, id } => {
                    let view = tiles::View {
                        x: x.into(),
                        y: y.into(),
                        radius: radius.into(),
                    };
                    wake(&work, |work| work.view = Some((view, id)));
                    let _ = actions.send(Action::Pointer(x.into(), y.into()));
                }
                LensMessage::Ack { serial } => {
                    let rtt = pacer
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .acked(serial, monotonic_ns());
                    rtts.extend(rtt);
                    wake(&work, |_| {});
                }
                LensMessage::Button { button, pressed } => {
                    let _ = actions.send(Action::Button(button, pressed));
                }
                LensMessage::Key { usage, pressed } => {
                    let _ = actions.send(Action::Key(usage, pressed));
                }
                LensMessage::Scroll { dx, dy } => {
                    let _ = actions.send(Action::Scroll(dx, dy));
                }
                LensMessage::Copy => {
                    let _ = actions.send(Action::Copy);
                }
                other => bail!("the viewer sent {other:?}"),
            }
        }
        anyhow::Ok(())
    };
    // The recording ends if Mutter stops it, for example when the session locks.
    let recording = async {
        let mut check = tokio::time::interval(Duration::from_millis(250));
        loop {
            check.tick().await;
            if capture.is_finished() {
                bail!("the screen recording stopped");
            }
        }
    };
    let result = tokio::select! {
        result = reader => result,
        result = writer => result,
        result = player => result,
        result = recording => result,
    };
    drop(_stop);
    let mut sent = encoder.join().unwrap_or_default();
    let captured = screen.lock().unwrap_or_else(|e| e.into_inner()).frames;
    tracing::info!(
        captured,
        sent = sent.frames,
        bytes = sent.bytes,
        first_ms = sent.first.map(|first| first.as_millis()),
        ack_ms = spread(&mut rtts, 1_000_000),
        encode_ms = spread(&mut sent.encode_ns, 1_000_000),
        window_kb = spread(&mut sent.windows, 1024),
        "lens served (median/95th/max)"
    );
    result
}

/// Spends the window on the newest frame and view each time something changes.
fn encode(
    work: &Shared,
    pacer: &Mutex<Pacer>,
    screen: &Mutex<Screen>,
    out: &mpsc::UnboundedSender<(LensMessage, Option<Vec<u8>>)>,
    sent: &mut Sent,
) {
    let started = Instant::now();
    let (lock, ready) = &**work;
    let mut encoder: Option<Encoder> = None;
    let mut frames = 0;
    let mut last_request = 0;
    let mut last_sent = Instant::now();
    loop {
        let view = {
            let mut work = lock.lock().unwrap_or_else(|e| e.into_inner());
            if !work.woken && !work.stop {
                work = ready
                    .wait_timeout(work, Duration::from_millis(100))
                    .unwrap_or_else(|e| e.into_inner())
                    .0;
            }
            work.woken = false;
            if work.stop {
                return;
            }
            work.view
        };
        // The viewer moves only once it knows the desktop's size, so the size and a first look
        // at the whole screen go out before any move.
        let (view, request) = view.map_or((None, 0), |(view, request)| (Some(view), request));
        let (budget, window) = {
            let pacer = pacer.lock().unwrap_or_else(|e| e.into_inner());
            let (window, in_flight) = (pacer.window(), pacer.in_flight());
            let budget = Budget {
                lens: window.saturating_sub(in_flight),
                fill: (window / 2).saturating_sub(in_flight),
            };
            (budget, window)
        };
        let packet = {
            let mut screen = screen.lock().unwrap_or_else(|e| e.into_inner());
            if screen.frames == 0 {
                continue;
            }
            let size = (screen.width, screen.height);
            if encoder.as_ref().is_none_or(|e| e.size() != size) {
                match Encoder::new(size.0, size.1) {
                    Ok(new) => encoder = Some(new),
                    Err(error) => {
                        tracing::warn!(%error, "lens cannot show this screen");
                        return;
                    }
                }
                let ready = LensMessage::Ready {
                    width: size.0 as u16,
                    height: size.1 as u16,
                };
                if out.send((ready, None)).is_err() {
                    return;
                }
                sent.first.get_or_insert(started.elapsed());
                frames = 0;
            }
            let encoder = encoder.as_mut().expect("made above");
            if screen.frames != frames {
                frames = screen.frames;
                let damage = screen.take_damage();
                encoder.changed(damage.as_deref());
            }
            let Some((pixels, stride)) = screen.frame() else {
                continue;
            };
            let started = monotonic_ns();
            let packet = encoder.encode(pixels, stride, view, budget, started);
            sent.encode_ns.push(monotonic_ns() - started);
            packet
        };
        let packet = match packet {
            Ok(packet) => packet,
            Err(error) => {
                tracing::warn!(%error, "lens could not encode a frame");
                return;
            }
        };
        if packet.body.is_some() || request != last_request || last_sent.elapsed() >= RECEIPT_EVERY
        {
            let body = packet.body.unwrap_or_default();
            let serial = pacer.lock().unwrap_or_else(|e| e.into_inner()).sent(
                body.len() + 64,
                packet.more,
                monotonic_ns(),
            );
            let frame = LensMessage::Frame {
                serial,
                request,
                exact: packet.exact,
            };
            let bytes = body.len();
            if out.send((frame, Some(body))).is_err() {
                return;
            }
            sent.frames += 1;
            sent.bytes += bytes;
            sent.windows.push(window as u64);
            last_request = request;
            last_sent = Instant::now();
        }
        if packet.more {
            wake(work, |_| {});
        }
    }
}

/// Plays the viewer's input into Mutter in order. Pointer moves that queue up collapse to the
/// newest; keys go through this peer's keyboard mode, as in a crossing.
async fn play(
    desk: Arc<Desk>,
    opened: &LensOpened,
    mut pending: mpsc::UnboundedReceiver<Action>,
    out: mpsc::UnboundedSender<(LensMessage, Option<Vec<u8>>)>,
) -> Result<()> {
    let mut remap = KeyRemap::new(opened.keyboard);
    let mut buttons = 0_u32;
    while let Some(mut action) = pending.recv().await {
        while let Action::Pointer(..) = action {
            match pending.try_recv() {
                Ok(newer @ Action::Pointer(..)) => action = newer,
                Ok(other) => {
                    if let Action::Pointer(x, y) = action {
                        desk.pointer(x, y).await?;
                    }
                    action = other;
                }
                Err(_) => break,
            }
        }
        match action {
            Action::Pointer(x, y) => desk.pointer(x, y).await?,
            Action::Button(button, pressed) => {
                let Ok(code) = pointer_button_to_evdev(PointerButton(button.into())) else {
                    continue;
                };
                let bit = 1 << button.min(31);
                buttons = if pressed {
                    buttons | bit
                } else {
                    buttons & !bit
                };
                let edges = if pressed {
                    remap.pointer(buttons != 0)
                } else {
                    remap.pointer_released(buttons != 0);
                    Vec::new()
                };
                keys(&desk, edges).await?;
                desk.button(code.code().into(), pressed).await?;
            }
            Action::Key(usage, pressed) => {
                if pressed {
                    remap.set_terminal(FOCUSED_TERMINAL.load(Ordering::Relaxed));
                }
                keys(&desk, remap.key(usage, pressed)).await?;
            }
            Action::Scroll(dx, dy) => {
                keys(&desk, remap.pointer(buttons != 0)).await?;
                desk.scroll(dx.into(), dy.into()).await?;
            }
            Action::Copy => {
                let reply = match copy(&desk, &remap).await {
                    Ok(text) => LensMessage::Copied { text },
                    Err(error) => LensMessage::Refused {
                        reason: format!("{error:#}"),
                    },
                };
                if out.send((reply, None)).is_err() {
                    break;
                }
            }
        }
    }
    Ok(())
}

async fn keys(desk: &Desk, edges: Vec<(HidUsage, bool)>) -> Result<()> {
    for (usage, pressed) in edges {
        if let Ok(code) = hid_to_evdev_key(usage) {
            desk.key(code.code().into(), pressed).await?;
        }
    }
    Ok(())
}

/// Presses this desktop's copy shortcut, Ctrl+Shift+C in a terminal and Ctrl+C elsewhere,
/// leaving alone any modifier the viewer already holds, and returns the selection it made.
async fn copy(desk: &Desk, remap: &KeyRemap) -> Result<String> {
    // Ctrl+C in a terminal would interrupt whatever runs there, so guessing is not an option.
    anyhow::ensure!(
        FOCUS_KNOWN.load(Ordering::Relaxed),
        "this desktop cannot tell which app has focus; update its zflow GNOME extension"
    );
    let generation = desk.selection_generation();
    let terminal = FOCUSED_TERMINAL.load(Ordering::Relaxed);
    let held: Vec<HidUsage> = remap.injected().collect();
    let mut pressed = Vec::new();
    for modifier in [LEFT_CONTROL, LEFT_SHIFT] {
        if (modifier == LEFT_SHIFT && !terminal) || held.contains(&modifier) {
            continue;
        }
        pressed.push(modifier);
    }
    let mut edges: Vec<_> = pressed.iter().map(|key| (*key, true)).collect();
    edges.extend([(KEY_C, true), (KEY_C, false)]);
    edges.extend(pressed.iter().rev().map(|key| (*key, false)));
    keys(desk, edges).await?;
    let text = desk.read_selection(generation, COPY_TIMEOUT).await?;
    anyhow::ensure!(
        text.len() <= super::wire::MAX_TEXT_BYTES,
        "the selection is too large to copy"
    );
    Ok(text)
}

struct StopOnDrop(Shared);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        wake(&self.0, |work| work.stop = true);
    }
}

fn monotonic_ns() -> u64 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_nanos() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_desktop_size_and_a_first_look_go_out_before_the_viewer_moves() {
        let pixels = vec![7_u8; 128 * 64 * 4].leak();
        let screen = Mutex::new(Screen::showing(pixels, 128, 64));
        let (work, pacer): (Shared, _) = (Arc::default(), Mutex::new(Pacer::default()));
        let (out, mut sent) = mpsc::unbounded_channel();
        std::thread::scope(|scope| {
            scope.spawn(|| encode(&work, &pacer, &screen, &out, &mut Sent::default()));
            let deadline = Instant::now() + Duration::from_secs(2);
            let mut next = || loop {
                match sent.try_recv() {
                    Ok(message) => return Some(message),
                    Err(_) if Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(_) => return None,
                }
            };
            let ready = next().map(|(message, _)| message);
            let first = next();
            wake(&work, |work| work.stop = true);
            assert_eq!(
                ready,
                Some(LensMessage::Ready {
                    width: 128,
                    height: 64
                })
            );
            let Some((LensMessage::Frame { request: 0, .. }, Some(body))) = first else {
                panic!("expected a first look, got {first:?}");
            };
            assert!(!body.is_empty());
        });
    }
}
