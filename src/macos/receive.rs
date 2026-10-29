use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

use anyhow::{Result, bail};
use tokio::sync::{mpsc, oneshot};

use super::{
    CursorPosition, awdl,
    clipboard::{Clipboard, MacPasteboard, Pasteboard},
    handoff_server::{Desk, HandoffServer},
    inject::{self, Injector},
};
use crate::{
    clipboard::Clip,
    config::{PeerConfig, PeerPermissions},
    core::{KeyboardMode, ReceiverEffect, SessionCloseReason},
    desktop::{DesktopRequest, DesktopResponse, Geometry, Point, SharedLayout},
    session::{SessionEvent, SessionEventKind, SessionHandle},
};

pub(crate) const SENDING: &str = "This computer is sending its own input";
pub(crate) const OWNED: &str = "Another computer owns input";
pub(crate) const HANDOFF_ACTIVE: &str = "A desktop handoff is already active";
const NOT_ALLOWED: &str =
    "Desktop control requires the active unlocked local session and an authorized paired peer";
const REJECTED: &str = "receiver effects were rejected before backend application";
const LOCK_CHECK: Duration = Duration::from_millis(250);
const AWDL_LIMIT: Duration = Duration::from_secs(5);

#[derive(Default)]
struct State {
    outbound: bool,
    inbound: Option<(String, u64, u64)>,
    lease: Option<(String, u64)>,
    claims: u64,
}

impl State {
    fn taken(&self, peer: &str, session_id: u64) -> bool {
        let other = |owner: &str, id: u64| owner != peer || id != session_id;
        self.inbound
            .as_ref()
            .is_some_and(|(owner, id, _)| other(owner, *id))
            || self
                .lease
                .as_ref()
                .is_some_and(|(owner, id)| other(owner, *id))
    }

    fn check(&self, peer: &str, session_id: u64) -> Result<(), &'static str> {
        if self.outbound {
            Err(SENDING)
        } else if self.taken(peer, session_id) {
            Err(OWNED)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct Ownership(Arc<Mutex<State>>);

impl Ownership {
    fn state(&self) -> MutexGuard<'_, State> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn begin_outbound(&self) -> Option<OutboundGuard> {
        let mut state = self.state();
        if state.outbound || state.inbound.is_some() || state.lease.is_some() {
            return None;
        }
        state.outbound = true;
        Some(OutboundGuard(self.clone()))
    }

    pub fn claim_inbound(&self, peer: &str, session_id: u64) -> Result<InboundClaim, &'static str> {
        let mut state = self.state();
        state.check(peer, session_id)?;
        state.claims += 1;
        let claim = state.claims;
        state.inbound = Some((peer.to_owned(), session_id, claim));
        Ok(InboundClaim {
            ownership: self.clone(),
            claim,
        })
    }

    pub fn begin_lease(&self, peer: &str, session_id: u64) -> Result<LeaseGuard, &'static str> {
        let mut state = self.state();
        state.check(peer, session_id)?;
        if state.lease.is_some() {
            return Err(HANDOFF_ACTIVE);
        }
        state.lease = Some((peer.to_owned(), session_id));
        Ok(LeaseGuard(self.clone()))
    }

    pub fn desktop_allowed(&self, peer: &str, session_id: u64) -> Result<(), &'static str> {
        self.state().check(peer, session_id)
    }

    pub fn controller(&self) -> Option<String> {
        let state = self.state();
        let inbound = state.inbound.as_ref().map(|(peer, ..)| peer);
        inbound
            .or(state.lease.as_ref().map(|(peer, _)| peer))
            .cloned()
    }
}

pub(crate) struct OutboundGuard(Ownership);

impl Drop for OutboundGuard {
    fn drop(&mut self) {
        self.0.state().outbound = false;
    }
}

pub(crate) struct InboundClaim {
    ownership: Ownership,
    claim: u64,
}

impl Drop for InboundClaim {
    fn drop(&mut self) {
        let mut state = self.ownership.state();
        if state
            .inbound
            .as_ref()
            .is_some_and(|(.., claim)| *claim == self.claim)
        {
            state.inbound = None;
        }
    }
}

pub(crate) struct LeaseGuard(Ownership);

impl Drop for LeaseGuard {
    fn drop(&mut self) {
        self.0.state().lease = None;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Admission {
    Inject,
    Refuse,
}

pub(crate) fn admit(
    permissions: Option<&PeerPermissions>,
    post_allowed: bool,
    locked: bool,
) -> Admission {
    let allowed = permissions.is_some_and(|peer| peer.connect && peer.send_normal);
    if allowed && post_allowed && !locked {
        Admission::Inject
    } else {
        Admission::Refuse
    }
}

pub(crate) fn admitted_effects(
    effects: Vec<ReceiverEffect>,
    admission: Admission,
) -> (Vec<ReceiverEffect>, bool) {
    let mut deliver = Vec::new();
    let mut refused = false;
    for effect in effects {
        if !effect.is_injection() || admission == Admission::Inject || is_safety_release(&effect) {
            deliver.push(effect);
        } else {
            refused = true;
        }
    }
    (deliver, refused)
}

pub(crate) fn is_safety_release(effect: &ReceiverEffect) -> bool {
    if let ReceiverEffect::TouchReplaced { state, synthetic } = effect {
        return *synthetic && state.is_empty();
    }
    matches!(
        effect,
        ReceiverEffect::Key {
            pressed: false,
            synthetic: true,
            ..
        } | ReceiverEffect::Button {
            pressed: false,
            synthetic: true,
            ..
        } | ReceiverEffect::ActivationClosed { .. }
    )
}

pub(super) fn answer_while_sending(kind: SessionEventKind) {
    match kind {
        SessionEventKind::Desktop { reply, .. } => {
            let _ = reply.send(DesktopResponse::unavailable(SENDING));
        }
        SessionEventKind::ReceiverEffects {
            effects, applied, ..
        } => {
            let opens = effects
                .iter()
                .any(|effect| matches!(effect, ReceiverEffect::ActivationOpened(_)));
            let (_, refused) = admitted_effects(effects, Admission::Refuse);
            let _ = applied.send(if opens || refused {
                Err(SENDING.into())
            } else {
                Ok(())
            });
        }
        SessionEventKind::OutboundEnded
        | SessionEventKind::Closed { .. }
        | SessionEventKind::Layout { .. }
        | SessionEventKind::Clipboard { .. }
        | SessionEventKind::Lens { .. } => {}
    }
}

pub(super) fn answer_waiting_events(
    events: &mut mpsc::Receiver<SessionEvent>,
    mut layout: impl FnMut(SharedLayout),
    mut clip: impl FnMut(Clip),
) -> Result<()> {
    loop {
        match events.try_recv() {
            Ok(event) => match event.kind {
                SessionEventKind::Closed { reason } => bail!("input session closed: {reason}"),
                SessionEventKind::Layout { layout: shared } => layout(shared),
                SessionEventKind::Clipboard { clip: shared } => clip(shared),
                kind => answer_while_sending(kind),
            },
            Err(mpsc::error::TryRecvError::Empty) => return Ok(()),
            Err(mpsc::error::TryRecvError::Disconnected) => bail!("input session closed"),
        }
    }
}

fn reverse_scrolling(effects: &mut [ReceiverEffect]) {
    for effect in effects {
        if let ReceiverEffect::Motion { delta, .. } = effect {
            delta.scroll_x = -delta.scroll_x;
            delta.scroll_y = -delta.scroll_y;
        }
    }
}

pub(crate) trait Screen: Send + Sync + 'static {
    fn geometry(&self) -> Result<Geometry>;
    fn generation(&self) -> u32;
    fn cursor(&self) -> Result<CursorPosition>;
    fn post_allowed(&self) -> bool;
    fn locked(&self) -> bool;
    fn wake(&self);
}

pub(crate) struct MacScreen;

impl Screen for MacScreen {
    fn geometry(&self) -> Result<Geometry> {
        super::desktop_geometry()
    }

    fn generation(&self) -> u32 {
        super::display_generation()
    }

    fn cursor(&self) -> Result<CursorPosition> {
        super::cursor_position()
    }

    fn post_allowed(&self) -> bool {
        inject::post_allowed()
    }

    fn locked(&self) -> bool {
        inject::session_locked()
    }

    fn wake(&self) {
        if let Err(error) = inject::declare_user_activity() {
            tracing::debug!(%error, "display not woken");
        }
    }
}

#[cfg(test)]
impl Screen for inject::FakeBackend {
    fn geometry(&self) -> Result<Geometry> {
        let monitors = self
            .state()
            .displays
            .iter()
            .map(|display| crate::desktop::Rect {
                x: display.x as i32,
                y: display.y as i32,
                width: display.width as u32,
                height: display.height as u32,
            })
            .collect();
        Ok(Geometry {
            monitors,
            displays: Vec::new(),
        })
    }

    fn generation(&self) -> u32 {
        self.state().generation
    }

    fn cursor(&self) -> Result<CursorPosition> {
        self.state()
            .cursor
            .ok_or_else(|| anyhow::anyhow!("no cursor"))
    }

    fn post_allowed(&self) -> bool {
        true
    }

    fn locked(&self) -> bool {
        self.state().locked
    }

    fn wake(&self) {
        self.state().wakes += 1;
    }
}

#[derive(Clone, Default)]
struct Sessions(Arc<Mutex<BTreeMap<String, SessionHandle>>>);

impl Sessions {
    fn lock(&self) -> MutexGuard<'_, BTreeMap<String, SessionHandle>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn close(&self, peer: &str, session_id: Option<u64>, reason: SessionCloseReason) {
        let sessions = self.lock();
        if let Some(session) = sessions.get(peer)
            && session_id.is_none_or(|id| id == session.id())
        {
            session.close(reason);
        }
    }
}

pub(crate) struct MacDesk {
    screen: Arc<dyn Screen>,
    injector: Arc<Injector>,
    sessions: Sessions,
}

impl Desk for MacDesk {
    fn geometry(&self) -> Result<Geometry> {
        self.screen.geometry()
    }

    fn generation(&self) -> u32 {
        self.screen.generation()
    }

    fn cursor(&self) -> Result<CursorPosition> {
        self.screen.cursor()
    }

    fn move_to(&self, point: Point) {
        self.injector.move_to(CursorPosition {
            x: f64::from(point.x),
            y: f64::from(point.y),
        });
    }

    fn release_all(&self) {
        self.injector.release_all();
    }

    fn wake(&self) {
        self.screen.wake();
    }

    fn close(&self, peer: &str, session_id: u64) {
        self.sessions
            .close(peer, Some(session_id), SessionCloseReason::LeaseExpired);
    }

    fn current(&self, peer: &str, session_id: u64) -> bool {
        self.sessions
            .lock()
            .get(peer)
            .is_some_and(|session| session.id() == session_id)
    }
}

#[derive(Default)]
struct Policy {
    peers: BTreeMap<String, PeerConfig>,
    accessibility: bool,
    reduce_wifi_latency: bool,
}

pub(crate) struct Receiving {
    ownership: Ownership,
    screen: Arc<dyn Screen>,
    injector: Arc<Injector>,
    handoff: Arc<HandoffServer<MacDesk>>,
    policy: Mutex<Policy>,
    sessions: Sessions,
    clipboard: Arc<Clipboard>,
}

impl Receiving {
    pub fn mac() -> Result<Arc<Self>> {
        let pasteboard = Arc::new(MacPasteboard);
        Ok(Self::new(Injector::mac()?, Arc::new(MacScreen), pasteboard))
    }

    pub fn new(
        injector: Injector,
        screen: Arc<dyn Screen>,
        pasteboard: Arc<dyn Pasteboard>,
    ) -> Arc<Self> {
        let ownership = Ownership::default();
        let injector = Arc::new(injector);
        let sessions = Sessions::default();
        let desk = MacDesk {
            screen: screen.clone(),
            injector: injector.clone(),
            sessions: sessions.clone(),
        };
        let handoff = Arc::new(HandoffServer::new(desk, ownership.clone()));
        let server = Arc::downgrade(&handoff);
        injector.watch(Box::new(move |from, to| {
            server
                .upgrade()
                .is_some_and(|server| server.moved(from, to))
        }));
        Arc::new(Self {
            ownership,
            screen,
            injector,
            handoff,
            policy: Mutex::default(),
            sessions,
            clipboard: Clipboard::new(pasteboard),
        })
    }

    pub fn ownership(&self) -> &Ownership {
        &self.ownership
    }

    pub fn clipboard(&self) -> &Arc<Clipboard> {
        &self.clipboard
    }

    fn policy(&self) -> MutexGuard<'_, Policy> {
        self.policy.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn set_peers(&self, peers: BTreeMap<String, PeerConfig>) {
        let mut policy = self.policy();
        if policy.peers != peers {
            policy.peers = peers;
            self.revoke(&policy);
        }
    }

    pub fn set_accessibility(&self, allowed: bool) {
        let mut policy = self.policy();
        if policy.accessibility != allowed {
            policy.accessibility = allowed;
            self.revoke(&policy);
        }
    }

    pub fn set_reduce_wifi_latency(&self, reduce: bool) {
        self.policy().reduce_wifi_latency = reduce;
    }

    fn revoke(&self, policy: &Policy) {
        let Some(peer) = self.ownership.controller() else {
            return;
        };
        let permissions = policy.peers.get(&peer).map(|record| &record.permissions);
        if admit(permissions, policy.accessibility, false) == Admission::Refuse {
            tracing::info!(%peer, "peer may no longer control this Mac");
            self.sessions
                .close(&peer, None, SessionCloseReason::PermissionRevoked);
        }
    }

    fn admission(&self, peer: &str, fresh: bool) -> (Admission, KeyboardMode, bool) {
        let (permissions, keyboard, reverse, accessibility) = {
            let policy = self.policy();
            let record = policy.peers.get(peer);
            (
                record.map(|record| record.permissions),
                record.map_or(KeyboardMode::Standard, |record| record.keyboard),
                record.is_some_and(|record| record.reverse_scroll),
                policy.accessibility,
            )
        };
        let post_allowed = accessibility && (!fresh || self.screen.post_allowed());
        let locked = fresh && self.screen.locked();
        let admission = admit(permissions.as_ref(), post_allowed, locked);
        (admission, keyboard, reverse)
    }

    pub fn inbound(self: &Arc<Self>, peer: &str, session: &SessionHandle) -> Inbound {
        self.sessions
            .lock()
            .insert(peer.to_owned(), session.clone());
        Inbound {
            receiving: self.clone(),
            peer: peer.to_owned(),
            session: session.clone(),
            claim: None,
            wifi: None,
        }
    }

    pub async fn watch(&self) {
        tokio::join!(self.handoff.watch(), self.watch_lock());
    }

    async fn watch_lock(&self) {
        let mut check = tokio::time::interval(LOCK_CHECK);
        check.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            check.tick().await;
            let Some(peer) = self.ownership.controller() else {
                continue;
            };
            if self.screen.locked() {
                tracing::info!(%peer, "this Mac locked; ending control");
                self.injector.release_all();
                self.sessions
                    .close(&peer, None, SessionCloseReason::PermissionRevoked);
            }
        }
    }

    async fn desktop(
        &self,
        peer: &str,
        session_id: u64,
        request: DesktopRequest,
    ) -> DesktopResponse {
        if self.admission(peer, true).0 == Admission::Refuse {
            return DesktopResponse::unavailable(NOT_ALLOWED);
        }
        self.handoff.request(peer, session_id, request).await
    }
}

pub(crate) struct Inbound {
    receiving: Arc<Receiving>,
    peer: String,
    session: SessionHandle,
    claim: Option<InboundClaim>,
    wifi: Option<awdl::Wanted>,
}

impl Inbound {
    pub fn desktop(&self, request: DesktopRequest, reply: oneshot::Sender<DesktopResponse>) {
        let (receiving, peer, id) = (self.receiving.clone(), self.peer.clone(), self.session.id());
        tokio::spawn(async move {
            let _ = reply.send(receiving.desktop(&peer, id, request).await);
        });
    }

    pub fn clipboard(&self) -> Arc<Clipboard> {
        self.receiving.clipboard.clone()
    }

    pub fn controls(&self) -> bool {
        self.receiving
            .ownership
            .controller()
            .is_some_and(|peer| peer == self.peer)
    }

    pub async fn effects(
        &mut self,
        effects: Vec<ReceiverEffect>,
        received_at: Instant,
    ) -> Result<(), String> {
        let opens = effects
            .iter()
            .any(|effect| matches!(effect, ReceiverEffect::ActivationOpened(_)));
        let closes = effects.iter().rev().find_map(|effect| match effect {
            ReceiverEffect::ActivationOpened(_) => Some(false),
            ReceiverEffect::ActivationClosed { .. } => Some(true),
            _ => None,
        }) == Some(true);
        let (mut admission, keyboard, reverse) = self.receiving.admission(&self.peer, opens);
        if opens {
            if admission == Admission::Refuse {
                self.release();
                return Err(NOT_ALLOWED.into());
            }
            let claim = self
                .receiving
                .ownership
                .claim_inbound(&self.peer, self.session.id());
            match claim {
                Ok(claim) => {
                    let first = self.claim.replace(claim).is_none();
                    if self.receiving.admission(&self.peer, false).0 == Admission::Refuse {
                        self.release();
                        return Err(NOT_ALLOWED.into());
                    }
                    if first {
                        tracing::info!(peer = %self.peer, "peer took control of this Mac");
                        self.take_control();
                    }
                }
                Err(reason) => {
                    self.release();
                    return Err(reason.into());
                }
            }
        } else if self.claim.is_none() {
            admission = Admission::Refuse;
        }
        let (mut deliver, refused) = admitted_effects(effects, admission);
        if reverse {
            reverse_scrolling(&mut deliver);
        }
        let applied = if deliver.is_empty() {
            Ok(())
        } else {
            let keyboard = opens.then_some(keyboard);
            match self.receiving.injector.apply(deliver, keyboard).await {
                Ok(Ok(applied_at)) => {
                    self.session
                        .record_receive_to_inject(received_at, applied_at);
                    Ok(())
                }
                Ok(Err(error)) => Err(format!("{error:#}")),
                Err(_) => Err("the Mac input thread stopped".into()),
            }
        };
        if refused || applied.is_err() {
            self.release();
        } else if closes && self.claim.take().is_some() {
            self.wifi = None;
            tracing::info!(peer = %self.peer, "peer let go of this Mac");
            self.receiving.clipboard.share(&self.session);
        }
        if refused {
            return Err(REJECTED.into());
        }
        applied
    }

    fn take_control(&mut self) {
        self.receiving.screen.wake();
        if self.receiving.policy().reduce_wifi_latency {
            self.wifi = Some(awdl::shared().want());
            let peer = self.peer.clone();
            tokio::spawn(async move { awdl::shared().down(&peer, AWDL_LIMIT).await });
        }
    }

    fn release(&mut self) {
        if let Some(claim) = self.claim.take() {
            self.receiving.injector.release_all();
            drop(claim);
            self.wifi = None;
            tracing::info!(peer = %self.peer, "peer lost control of this Mac");
        }
    }
}

impl Drop for Inbound {
    fn drop(&mut self) {
        self.release();
        let id = self.session.id();
        {
            let mut sessions = self.receiving.sessions.lock();
            if sessions
                .get(&self.peer)
                .is_some_and(|session| session.id() == id)
            {
                sessions.remove(&self.peer);
            }
        }
        self.receiving.handoff.closed(&self.peer, id);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use tokio::sync::oneshot;

    use super::*;
    use crate::{
        core::{
            ActivationId, HidUsage, MotionDelta, MotionSequence, SessionCloseReason,
            SessionContext, SessionEpoch, TransportGeneration,
        },
        desktop::DesktopRequest,
    };

    #[test]
    fn sending_and_being_controlled_exclude_each_other() {
        let ownership = Ownership::default();
        let outbound = ownership.begin_outbound().unwrap();
        assert!(
            ownership.begin_outbound().is_none(),
            "one crossing at a time"
        );
        assert!(ownership.claim_inbound("linux", 1).is_err());
        assert_eq!(ownership.begin_lease("linux", 1).err(), Some(SENDING));
        assert_eq!(ownership.desktop_allowed("linux", 1), Err(SENDING));
        assert_eq!(ownership.controller(), None);
        drop(outbound);

        let claim = ownership.claim_inbound("linux", 1).unwrap();
        assert_eq!(ownership.controller().as_deref(), Some("linux"));
        assert!(ownership.begin_outbound().is_none());
        drop(claim);
        assert_eq!(ownership.controller(), None);
        assert!(ownership.begin_outbound().is_some());
    }

    #[test]
    fn one_session_controls_at_a_time_and_may_claim_again() {
        let ownership = Ownership::default();
        let first = ownership.claim_inbound("linux", 1).unwrap();
        assert!(
            ownership.claim_inbound("linux", 2).is_err(),
            "another session"
        );
        assert!(ownership.claim_inbound("desk", 1).is_err(), "another peer");
        assert_eq!(ownership.desktop_allowed("desk", 1), Err(OWNED));
        assert_eq!(ownership.begin_lease("desk", 1).err(), Some(OWNED));
        assert_eq!(ownership.desktop_allowed("linux", 1), Ok(()));

        let second = ownership.claim_inbound("linux", 1).unwrap();
        drop(first);
        assert_eq!(ownership.controller().as_deref(), Some("linux"));
        assert!(ownership.claim_inbound("linux", 2).is_err());
        drop(second);
        assert!(ownership.claim_inbound("linux", 2).is_ok());
    }

    #[test]
    fn a_desktop_handoff_keeps_the_mac_for_its_session() {
        let ownership = Ownership::default();
        let lease = ownership.begin_lease("linux", 1).unwrap();
        assert_eq!(ownership.controller().as_deref(), Some("linux"));
        assert_eq!(
            ownership.begin_lease("linux", 1).err(),
            Some(HANDOFF_ACTIVE)
        );
        assert!(ownership.begin_outbound().is_none());
        assert!(ownership.claim_inbound("desk", 1).is_err());
        let claim = ownership.claim_inbound("linux", 1).unwrap();
        drop(lease);
        assert_eq!(ownership.controller().as_deref(), Some("linux"));
        drop(claim);
        assert_eq!(ownership.controller(), None);
        assert!(ownership.begin_lease("desk", 3).is_ok());
    }

    fn context() -> SessionContext {
        SessionContext {
            session_epoch: SessionEpoch([7; 16]),
            transport_generation: TransportGeneration(1),
            activation_id: ActivationId(1),
        }
    }

    fn key(pressed: bool, synthetic: bool) -> ReceiverEffect {
        ReceiverEffect::Key {
            key: HidUsage::keyboard(4),
            pressed,
            synthetic,
        }
    }

    fn closed() -> ReceiverEffect {
        ReceiverEffect::ActivationClosed {
            session: context(),
            reason: SessionCloseReason::LocalRelease,
        }
    }

    #[test]
    fn only_an_allowed_peer_on_an_unlocked_mac_injects() {
        let peer = PeerPermissions {
            connect: true,
            send_normal: true,
            ..PeerPermissions::default()
        };
        assert_eq!(admit(Some(&peer), true, false), Admission::Inject);
        assert_eq!(admit(Some(&peer), false, false), Admission::Refuse);
        assert_eq!(admit(Some(&peer), true, true), Admission::Refuse);
        assert_eq!(admit(None, true, false), Admission::Refuse);
        for peer in [
            PeerPermissions {
                send_normal: false,
                ..peer
            },
            PeerPermissions {
                connect: false,
                ..peer
            },
        ] {
            assert_eq!(admit(Some(&peer), true, false), Admission::Refuse);
        }

        let effects = || {
            vec![
                key(true, false),
                ReceiverEffect::Motion {
                    delta: MotionDelta::default(),
                    through_sequence: MotionSequence(1),
                },
                key(false, false),
                key(false, true),
                closed(),
            ]
        };
        let (denied, refused) = admitted_effects(effects(), Admission::Refuse);
        assert!(refused);
        assert_eq!(denied.len(), 2);
        assert!(denied.iter().all(is_safety_release));
        let (injected, refused) = admitted_effects(effects(), Admission::Inject);
        assert!(!refused);
        assert_eq!(injected.len(), 5);
    }

    fn apply(effects: Vec<ReceiverEffect>) -> Result<(), String> {
        let (applied, answer) = oneshot::channel();
        answer_while_sending(SessionEventKind::ReceiverEffects {
            effects,
            touch_captured_at: None,
            received_at: Instant::now(),
            applied,
        });
        answer.blocking_recv().unwrap()
    }

    #[test]
    fn while_sending_only_releases_succeed() {
        assert_eq!(apply(vec![key(false, true), closed()]), Ok(()));
        assert_eq!(apply(vec![key(true, false)]), Err(SENDING.into()));
        assert_eq!(
            apply(vec![ReceiverEffect::ActivationOpened(context())]),
            Err(SENDING.into()),
            "an activation cannot open, which closes the session"
        );
    }

    #[tokio::test]
    async fn idle_events_are_answered_and_a_closed_session_refuses_the_crossing() {
        let event = |kind| SessionEvent {
            session_id: 1,
            peer: "linux".into(),
            kind,
        };
        let (sender, mut events) = mpsc::channel(8);
        let (reply, desktop) = oneshot::channel();
        sender
            .send(event(SessionEventKind::OutboundEnded))
            .await
            .unwrap();
        sender
            .send(event(SessionEventKind::Desktop {
                request: DesktopRequest::Snapshot,
                reply,
            }))
            .await
            .unwrap();
        answer_waiting_events(&mut events, drop, drop).unwrap();
        assert_eq!(
            desktop.await.unwrap(),
            DesktopResponse::unavailable(SENDING)
        );
        sender
            .send(event(SessionEventKind::Closed {
                reason: "lost".into(),
            }))
            .await
            .unwrap();
        assert!(answer_waiting_events(&mut events, drop, drop).is_err());
        drop(sender);
        assert!(answer_waiting_events(&mut events, drop, drop).is_err());
    }
}
