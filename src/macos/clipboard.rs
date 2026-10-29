use std::{
    collections::BTreeMap,
    ffi::c_char,
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::{Context, Result, bail};

use crate::{
    clipboard::{Clip, ClipKind, Echo, MAX_CLIP_BYTES},
    session::SessionHandle,
};

#[derive(Debug, PartialEq)]
pub(crate) enum Contents {
    Unchanged,
    Empty,
    Clip(ClipKind, Vec<u8>),
    TooLarge(usize),
    NotAllowed,
}

pub(crate) trait Pasteboard: Send + Sync + 'static {
    fn read(&self, ours: Option<i64>) -> Result<Contents>;
    fn write(&self, clip: &Clip) -> Result<i64>;
    fn ask(&self);
}

pub(crate) struct MacPasteboard;

const KIND_EMPTY: u32 = 0;
const KIND_UNCHANGED: u32 = 3;
const KIND_NOT_ALLOWED: u32 = 4;

const NOT_ALLOWED: &str = "Clipboard not shared: allow zflow in System Settings > \
    Privacy & Security > Paste from Other Apps";

impl Pasteboard for MacPasteboard {
    fn read(&self, ours: Option<i64>) -> Result<Contents> {
        let (mut kind, mut data, mut length, mut count) = (0, std::ptr::null_mut(), 0, 0);
        let unchanged_at = ours.as_ref().map_or(std::ptr::null(), |count| count);
        let status = unsafe {
            zflow_mac_pasteboard_read(
                std::ptr::null(),
                unchanged_at,
                MAX_CLIP_BYTES,
                &mut kind,
                &mut data,
                &mut length,
                &mut count,
            )
        };
        if status != 0 {
            bail!("Could not read the Mac clipboard");
        }
        let bytes = (!data.is_null()).then(|| {
            let bytes = unsafe { std::slice::from_raw_parts(data, length) }.to_vec();
            unsafe { zflow_mac_pasteboard_free(data) };
            bytes
        });
        Ok(match (kind, bytes) {
            (KIND_UNCHANGED, _) => Contents::Unchanged,
            (KIND_NOT_ALLOWED, _) => Contents::NotAllowed,
            (KIND_EMPTY, _) => Contents::Empty,
            (_, None) if length > MAX_CLIP_BYTES => Contents::TooLarge(length),
            (_, None) => Contents::Empty,
            (code, Some(bytes)) => {
                let kind = u8::try_from(code).ok().and_then(ClipKind::from_code);
                Contents::Clip(kind.context("Unknown clipboard kind")?, bytes)
            }
        })
    }

    fn write(&self, clip: &Clip) -> Result<i64> {
        let mut count = 0;
        let data = clip.data();
        let status = unsafe {
            zflow_mac_pasteboard_write(
                std::ptr::null(),
                u32::from(clip.kind().code()),
                data.as_ptr(),
                data.len(),
                &mut count,
            )
        };
        if status != 0 {
            bail!("Could not write the Mac clipboard");
        }
        Ok(count)
    }

    fn ask(&self) {
        if cfg!(test) {
            return;
        }
        unsafe { zflow_mac_pasteboard_ask(std::ptr::null()) }
    }
}

#[derive(Debug, PartialEq)]
enum Outgoing {
    Send(Clip),
    TooLarge(usize),
    NotAllowed,
    Nothing,
}

#[derive(Default)]
struct State {
    echoes: BTreeMap<String, Echo>,
    written: Option<(i64, Clip)>,
    notice: Option<String>,
}

impl State {
    fn outgoing(&mut self, peer: &str, contents: Contents) -> Result<Outgoing> {
        let clip = match contents {
            Contents::Unchanged => match &self.written {
                Some((_, clip)) => clip.clone(),
                None => return Ok(Outgoing::Nothing),
            },
            Contents::Empty => return Ok(Outgoing::Nothing),
            Contents::TooLarge(bytes) => return Ok(Outgoing::TooLarge(bytes)),
            Contents::NotAllowed => return Ok(Outgoing::NotAllowed),
            Contents::Clip(_, data) if data.is_empty() => return Ok(Outgoing::Nothing),
            Contents::Clip(_, data) if data.len() > MAX_CLIP_BYTES => {
                return Ok(Outgoing::TooLarge(data.len()));
            }
            Contents::Clip(kind, data) => Clip::new(kind, data)?,
        };
        let echo = self.echoes.entry(peer.to_owned()).or_default();
        Ok(if echo.should_send(&clip) {
            Outgoing::Send(clip)
        } else {
            Outgoing::Nothing
        })
    }
}

pub(crate) struct Clipboard {
    pasteboard: Arc<dyn Pasteboard>,
    share: AtomicBool,
    state: Mutex<State>,
}

impl Clipboard {
    pub fn new(pasteboard: Arc<dyn Pasteboard>) -> Arc<Self> {
        Arc::new(Self {
            pasteboard,
            share: AtomicBool::new(false),
            state: Mutex::default(),
        })
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn set_share(&self, share: bool) {
        let shared = self.share.swap(share, Ordering::Relaxed);
        if share && !shared {
            self.pasteboard.ask();
        }
        if !share {
            self.state().notice = None;
        }
    }

    pub fn notice(&self) -> Option<String> {
        self.state().notice.clone()
    }

    pub fn share(self: &Arc<Self>, session: &SessionHandle) {
        if !self.share.load(Ordering::Relaxed) {
            return;
        }
        let (clipboard, session) = (self.clone(), session.clone());
        tokio::task::spawn_blocking(move || {
            if session.is_closed() {
                return;
            }
            clipboard.send_to(session.peer(), |clip| session.send_clipboard(clip));
        });
    }

    fn send_to(&self, peer: &str, send: impl FnOnce(Clip)) {
        let ours = self.state().written.as_ref().map(|(count, _)| *count);
        let outgoing = self
            .pasteboard
            .read(ours)
            .and_then(|contents| self.state().outgoing(peer, contents));
        match outgoing {
            Ok(Outgoing::Send(clip)) => {
                self.state().notice = None;
                tracing::debug!(%peer, kind = ?clip.kind(), bytes = clip.data().len(), "clipboard sent");
                send(clip);
            }
            Ok(Outgoing::TooLarge(bytes)) => {
                tracing::info!(%peer, bytes, "clipboard too large to share");
                self.state().notice = Some(crate::clipboard::too_large(bytes));
            }
            Ok(Outgoing::NotAllowed) => {
                tracing::info!(%peer, "clipboard not shared: macOS does not let zflow read it");
                self.state().notice = Some(NOT_ALLOWED.into());
            }
            Ok(Outgoing::Nothing) => self.state().notice = None,
            Err(error) => {
                tracing::info!(%peer, error = %format_args!("{error:#}"), "clipboard not shared");
            }
        }
    }

    pub fn keep(self: &Arc<Self>, peer: &str, clip: Clip) {
        let (kind, bytes) = (clip.kind(), clip.data().len());
        if !self.share.load(Ordering::Relaxed) {
            tracing::debug!(%peer, ?kind, bytes, "clipboard from peer dropped: sharing is off");
            return;
        }
        self.state()
            .echoes
            .entry(peer.to_owned())
            .or_default()
            .written(&clip);
        let (clipboard, peer) = (self.clone(), peer.to_owned());
        tokio::task::spawn_blocking(move || match clipboard.pasteboard.write(&clip) {
            Ok(count) => {
                tracing::debug!(%peer, ?kind, bytes, "clipboard kept");
                clipboard.state().written = Some((count, clip));
            }
            Err(error) => {
                tracing::info!(%peer, ?kind, bytes, error = %format_args!("{error:#}"), "clipboard not kept");
            }
        });
    }
}

unsafe extern "C" {
    fn zflow_mac_pasteboard_read(
        name: *const c_char,
        unchanged_at: *const i64,
        limit: usize,
        kind: *mut u32,
        data: *mut *mut u8,
        length: *mut usize,
        change_count: *mut i64,
    ) -> i32;
    fn zflow_mac_pasteboard_free(data: *mut u8);
    fn zflow_mac_pasteboard_ask(name: *const c_char);
    fn zflow_mac_pasteboard_write(
        name: *const c_char,
        kind: u32,
        data: *const u8,
        length: usize,
        change_count: *mut i64,
    ) -> i32;
}

#[cfg(test)]
#[derive(Clone, Default)]
pub(crate) struct FakePasteboard(Arc<Mutex<FakeBoard>>);

#[cfg(test)]
#[derive(Default)]
pub(crate) struct FakeBoard {
    pub contents: Option<(ClipKind, Vec<u8>)>,
    pub count: i64,
    pub reads: usize,
    pub writes: Vec<Clip>,
    pub not_allowed: bool,
    pub asks: usize,
}

#[cfg(test)]
impl FakePasteboard {
    pub fn board(&self) -> MutexGuard<'_, FakeBoard> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn copy(&self, kind: ClipKind, data: &[u8]) {
        let mut board = self.board();
        board.contents = Some((kind, data.to_vec()));
        board.count += 1;
    }
}

#[cfg(test)]
impl Pasteboard for FakePasteboard {
    fn read(&self, ours: Option<i64>) -> Result<Contents> {
        let mut board = self.board();
        board.reads += 1;
        if ours == Some(board.count) {
            return Ok(Contents::Unchanged);
        }
        if board.not_allowed {
            return Ok(Contents::NotAllowed);
        }
        Ok(match &board.contents {
            None => Contents::Empty,
            Some((_, data)) if data.len() > MAX_CLIP_BYTES => Contents::TooLarge(data.len()),
            Some((kind, data)) => Contents::Clip(*kind, data.clone()),
        })
    }

    fn write(&self, clip: &Clip) -> Result<i64> {
        let mut board = self.board();
        board.contents = Some((clip.kind(), clip.data().to_vec()));
        board.count += 1;
        board.writes.push(clip.clone());
        Ok(board.count)
    }

    fn ask(&self) {
        self.board().asks += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(text: &str) -> Clip {
        Clip::new(ClipKind::Text, text.into()).unwrap()
    }

    #[test]
    fn only_a_new_clip_goes_and_a_large_one_is_explained() {
        let mut state = State::default();
        let clip = |data: &str| Contents::Clip(ClipKind::Text, data.into());
        let mut outgoing = |contents| state.outgoing("linux", contents).unwrap();
        assert_eq!(outgoing(clip("one")), Outgoing::Send(text("one")));
        assert_eq!(outgoing(clip("one")), Outgoing::Nothing, "linux has it");
        assert_eq!(outgoing(Contents::Empty), Outgoing::Nothing);
        assert_eq!(outgoing(clip("")), Outgoing::Nothing, "empty text");
        assert_eq!(outgoing(Contents::Unchanged), Outgoing::Nothing);
        assert_eq!(outgoing(Contents::NotAllowed), Outgoing::NotAllowed);
        let over = MAX_CLIP_BYTES + 1;
        assert_eq!(outgoing(Contents::TooLarge(over)), Outgoing::TooLarge(over));
        let large = Contents::Clip(ClipKind::Text, vec![b'a'; over]);
        assert_eq!(outgoing(large), Outgoing::TooLarge(over));
        let fake_png = Contents::Clip(ClipKind::Png, b"GIF89a".to_vec());
        assert!(state.outgoing("linux", fake_png).is_err());
        assert_eq!(
            state.outgoing("desk", clip("one")).unwrap(),
            Outgoing::Send(text("one"))
        );
    }

    #[test]
    fn this_macs_own_write_stands_for_what_it_wrote() {
        let mut state = State::default();
        let two = text("two");
        state
            .echoes
            .entry("linux".into())
            .or_default()
            .written(&two);
        state.written = Some((7, two.clone()));
        let unchanged = state.outgoing("linux", Contents::Unchanged).unwrap();
        assert_eq!(unchanged, Outgoing::Nothing);
        let forwarded = state.outgoing("desk", Contents::Unchanged).unwrap();
        assert_eq!(forwarded, Outgoing::Send(two));
    }

    #[test]
    fn a_clip_from_a_peer_is_dropped_while_sharing_is_off() {
        let fake = FakePasteboard::default();
        fake.copy(ClipKind::Text, b"copied here");
        let clipboard = Clipboard::new(Arc::new(fake.clone()));
        clipboard.keep("linux", text("from linux"));
        let board = fake.board();
        assert_eq!((board.reads, board.writes.len()), (0, 0));
        assert_eq!(
            board.contents,
            Some((ClipKind::Text, b"copied here".to_vec()))
        );
        drop(board);
        assert!(clipboard.state().echoes.is_empty());
    }

    fn blocking(work: impl FnOnce()) {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        {
            let _entered = runtime.enter();
            work();
        }
        runtime.shutdown_timeout(std::time::Duration::from_secs(2));
    }

    #[test]
    fn a_kept_clip_goes_on_the_pasteboard_and_never_back() {
        let fake = FakePasteboard::default();
        let clipboard = Clipboard::new(Arc::new(fake.clone()));
        clipboard.set_share(true);
        blocking(|| clipboard.keep("linux", text("from linux")));
        assert_eq!(fake.board().writes, [text("from linux")]);
        let written = clipboard.state().written.clone();
        assert_eq!(written, Some((1, text("from linux"))));

        let mut sent = Vec::new();
        clipboard.send_to("linux", |clip| sent.push(clip));
        assert!(sent.is_empty(), "linux sent it");
        assert_eq!(fake.board().reads, 1);
        clipboard.send_to("desk", |clip| sent.push(clip));
        assert_eq!(sent, [text("from linux")]);

        fake.copy(ClipKind::Text, b"copied here");
        clipboard.send_to("linux", |clip| sent.push(clip));
        clipboard.send_to("linux", |clip| sent.push(clip));
        assert_eq!(sent[1..], [text("copied here")]);
    }

    #[test]
    fn turning_sharing_on_lets_macos_ask_and_a_refusal_is_explained() {
        let fake = FakePasteboard::default();
        let clipboard = Clipboard::new(Arc::new(fake.clone()));
        clipboard.set_share(true);
        clipboard.set_share(true);
        assert_eq!(fake.board().asks, 1);
        clipboard.set_share(false);
        clipboard.set_share(true);
        assert_eq!(fake.board().asks, 2);

        fake.copy(ClipKind::Text, b"copied here");
        fake.board().not_allowed = true;
        let mut sent = Vec::new();
        clipboard.send_to("linux", |clip| sent.push(clip));
        assert!(sent.is_empty());
        assert_eq!(
            clipboard.notice().as_deref(),
            Some(
                "Clipboard not shared: allow zflow in System Settings > \
                 Privacy & Security > Paste from Other Apps"
            )
        );
        fake.board().not_allowed = false;
        clipboard.send_to("linux", |clip| sent.push(clip));
        assert_eq!(sent, [text("copied here")]);
        assert_eq!(clipboard.notice(), None);

        fake.board().not_allowed = true;
        blocking(|| clipboard.keep("linux", text("from linux")));
        clipboard.send_to("desk", |clip| sent.push(clip));
        assert_eq!(sent[1..], [text("from linux")]);
    }

    #[test]
    fn a_large_clipboard_is_explained_until_the_next_one_goes() {
        let fake = FakePasteboard::default();
        let clipboard = Clipboard::new(Arc::new(fake.clone()));
        clipboard.set_share(true);
        let png = |bytes: usize| {
            let mut data = b"\x89PNG\r\n\x1a\n".to_vec();
            data.resize(bytes, 0);
            data
        };
        fake.copy(ClipKind::Png, &png(5 * 1024 * 1024));
        let mut sent = Vec::new();
        clipboard.send_to("linux", |clip| sent.push(clip));
        assert!(sent.is_empty());
        assert_eq!(
            clipboard.notice().as_deref(),
            Some("Clipboard not shared: 5.0 MB is over the 3 MB limit")
        );
        fake.copy(ClipKind::Png, &png(1024));
        clipboard.send_to("linux", |clip| sent.push(clip));
        assert_eq!(sent.len(), 1);
        assert_eq!(clipboard.notice(), None);
        fake.copy(ClipKind::Png, &png(MAX_CLIP_BYTES + 1));
        clipboard.send_to("linux", |clip| sent.push(clip));
        assert!(clipboard.notice().is_some());
        clipboard.set_share(false);
        assert_eq!(clipboard.notice(), None);
    }
}
