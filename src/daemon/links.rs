use super::*;
use crate::{
    link::{STABLE_SESSION, retry_delay},
    peer_view::LinkStatus,
};

pub(super) trait Sessions: Send + Sync + 'static {
    fn current(&self, peer: &str) -> impl Future<Output = Option<u64>> + Send;
    fn dial(
        &self,
        peer: &str,
        record: &PeerConfig,
        addresses: &[SocketAddr],
    ) -> impl Future<Output = Result<u64>> + Send;
}

impl Sessions for Shared {
    async fn current(&self, peer: &str) -> Option<u64> {
        self.sessions.lock().await.get(peer).map(SessionHandle::id)
    }

    async fn dial(&self, peer: &str, record: &PeerConfig, addresses: &[SocketAddr]) -> Result<u64> {
        Ok(self.connect(peer, record, addresses).await?.id())
    }
}

pub(super) struct Link {
    record: PeerConfig,
    retry: mpsc::UnboundedSender<()>,
    status: watch::Receiver<Option<LinkStatus>>,
}

impl Shared {
    pub(super) async fn sync_links(self: &Arc<Self>) {
        let wanted = linked_peers(&*self.config.read().await);
        let mut links = self.links.lock().await;
        links.retain(|name, link| {
            wanted.get(name).map(without_addresses) == Some(without_addresses(&link.record))
        });
        for (name, record) in wanted {
            if let std::collections::btree_map::Entry::Vacant(slot) = links.entry(name) {
                let link = start(
                    self.clone(),
                    slot.key().clone(),
                    record,
                    self.session_changes.subscribe(),
                    self.neighbors.subscribe(),
                );
                slot.insert(link);
            }
        }
    }

    pub(super) async fn retry_links(&self) {
        for link in self.links.lock().await.values() {
            let _ = link.retry.send(());
        }
    }

    pub(super) async fn link_status(&self) -> BTreeMap<String, LinkStatus> {
        self.links
            .lock()
            .await
            .iter()
            .filter_map(|(name, link)| Some((name.clone(), link.status.borrow().clone()?)))
            .collect()
    }
}

fn linked_peers(config: &Config) -> BTreeMap<String, PeerConfig> {
    eligible_outbound_peers(config)
        .into_iter()
        .filter_map(|name| {
            let record = config.peers.get(&name)?.clone();
            Some((name, record))
        })
        .collect()
}

fn start<S: Sessions>(
    sessions: Arc<S>,
    peer: String,
    record: PeerConfig,
    changes: watch::Receiver<()>,
    neighbors: watch::Receiver<Neighbors>,
) -> Link {
    let (retry, retries) = mpsc::unbounded_channel();
    let (status_sender, status) = watch::channel(None);
    let task = Task {
        sessions,
        peer,
        key: record.fingerprint_hex().unwrap_or_default(),
        record: record.clone(),
        retries,
        changes,
        neighbors,
    };
    tokio::spawn(run(task, status_sender));
    Link {
        record,
        retry,
        status,
    }
}

#[derive(Debug, PartialEq)]
enum View {
    Session(u64),
    Dial(Vec<SocketAddr>),
}

struct Task<S> {
    sessions: Arc<S>,
    peer: String,
    key: String,
    record: PeerConfig,
    retries: mpsc::UnboundedReceiver<()>,
    changes: watch::Receiver<()>,
    neighbors: watch::Receiver<Neighbors>,
}

impl<S: Sessions> Task<S> {
    async fn view(&self) -> View {
        match self.sessions.current(&self.peer).await {
            Some(id) => View::Session(id),
            None => View::Dial(self.neighbors.borrow().addresses_for_key(&self.key)),
        }
    }

    async fn wait(&mut self, seen: &View, delay: Option<Duration>) -> bool {
        let sleep = async {
            match delay {
                Some(delay) => tokio::time::sleep(delay).await,
                None => std::future::pending().await,
            }
        };
        tokio::pin!(sleep);
        loop {
            if self.view().await != *seen {
                return true;
            }
            tokio::select! {
                () = &mut sleep => return true,
                retry = self.retries.recv() => match retry {
                    None => return false,
                    Some(()) if matches!(seen, View::Session(_)) => {}
                    Some(()) => return true,
                },
                changed = self.changes.changed() => if changed.is_err() {
                    return false;
                },
                changed = self.neighbors.changed() => if changed.is_err() {
                    return false;
                },
            }
        }
    }
}

async fn run<S: Sessions>(mut task: Task<S>, status: watch::Sender<Option<LinkStatus>>) {
    let mut failures = 0_u32;
    loop {
        let view = task.view().await;
        let session = match &view {
            View::Session(id) => *id,
            View::Dial(addresses) if addresses.is_empty() => {
                status.send_replace(None);
                if !task.wait(&view, None).await {
                    return;
                }
                continue;
            }
            View::Dial(addresses) => {
                if !matches!(*status.borrow(), Some(LinkStatus::Unreachable { .. })) {
                    status.send_replace(Some(LinkStatus::Connecting));
                }
                match task
                    .sessions
                    .dial(&task.peer, &task.record, addresses)
                    .await
                {
                    Ok(id) => id,
                    Err(_) if task.sessions.current(&task.peer).await.is_some() => continue,
                    Err(error) => {
                        failures += 1;
                        let down = Some(LinkStatus::Unreachable {
                            reason: crate::link::reason(&error),
                            needs_fix: crate::link::Fix::of(&error).is_some(),
                        });
                        if *status.borrow() != down {
                            tracing::info!(peer = %task.peer, error = %format_args!("{error:#}"),
                                failures, "input link could not connect");
                        }
                        status.send_replace(down);
                        if !task.wait(&view, Some(retry_delay(failures))).await {
                            return;
                        }
                        continue;
                    }
                }
            }
        };
        status.send_replace(None);
        let opened = tokio::time::Instant::now();
        if !task.wait(&View::Session(session), None).await {
            return;
        }
        failures = if opened.elapsed() >= STABLE_SESSION {
            0
        } else {
            failures + 1
        };
        let view = task.view().await;
        if failures > 0
            && matches!(view, View::Dial(_))
            && !task.wait(&view, Some(retry_delay(failures))).await
        {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::TransportError;
    use tokio::{sync::oneshot, time::Instant as Clock};

    const SAVED: &str = "192.0.2.7:43119";
    const DESK: &[u8] = &[1, 2, 3];
    const OTHER: &[u8] = &[9, 9, 9];

    fn record() -> PeerConfig {
        PeerConfig::from_spki(
            DESK,
            vec![SAVED.parse().unwrap()],
            PeerPermissions {
                connect: true,
                send_normal: true,
                receive_normal: true,
                inject_prelogin: false,
            },
        )
        .unwrap()
    }

    fn around(records: &[(&str, &[u8], &str)]) -> Neighbors {
        let mut neighbors = Neighbors::new(&[0]);
        let hello = crate::hello::make_hello("desk", 43119, Vec::new(), false);
        for &(instance, spki, address) in records {
            let address: SocketAddr = address.parse().unwrap();
            neighbors.instance_seen(instance, vec![address], true, None);
            let now = tokio::time::Instant::now();
            neighbors.hello(spki, address, &hello, Some(instance), now);
        }
        neighbors
    }

    type Dial = (Vec<SocketAddr>, oneshot::Sender<Result<u64>>);

    struct Fake {
        session: std::sync::Mutex<Option<u64>>,
        changes: watch::Sender<()>,
        dials: mpsc::UnboundedSender<Dial>,
    }

    impl Fake {
        fn set_session(&self, id: Option<u64>) {
            *self.session.lock().unwrap() = id;
            self.changes.send_replace(());
        }
    }

    impl Sessions for Fake {
        async fn current(&self, _peer: &str) -> Option<u64> {
            *self.session.lock().unwrap()
        }

        async fn dial(&self, _: &str, _: &PeerConfig, addresses: &[SocketAddr]) -> Result<u64> {
            let (answer, result) = oneshot::channel();
            self.dials.send((addresses.to_vec(), answer)).unwrap();
            let id = result.await.unwrap()?;
            self.set_session(Some(id));
            Ok(id)
        }
    }

    struct Harness {
        fake: Arc<Fake>,
        link: Link,
        neighbors: watch::Sender<Neighbors>,
        dials: mpsc::UnboundedReceiver<Dial>,
    }

    fn harness(neighbors: Neighbors, session: Option<u64>) -> Harness {
        let (dial_sender, dials) = mpsc::unbounded_channel();
        let fake = Arc::new(Fake {
            session: std::sync::Mutex::new(session),
            changes: watch::Sender::new(()),
            dials: dial_sender,
        });
        let neighbors = watch::Sender::new(neighbors);
        let link = start(
            fake.clone(),
            "desk".into(),
            record(),
            fake.changes.subscribe(),
            neighbors.subscribe(),
        );
        Harness {
            fake,
            link,
            neighbors,
            dials,
        }
    }

    fn unreachable(reason: &str, needs_fix: bool) -> Option<LinkStatus> {
        Some(LinkStatus::Unreachable {
            reason: reason.into(),
            needs_fix,
        })
    }

    #[test]
    fn paused_sharing_and_forgotten_peers_get_no_link() {
        let mut config = Config::default();
        config.peers.insert("desk".into(), record());
        let mut one_way = record();
        one_way.permissions.receive_normal = false;
        config.peers.insert("watcher".into(), one_way);
        assert_eq!(
            linked_peers(&config).into_keys().collect::<Vec<_>>(),
            ["desk"]
        );
        config.daemon.sharing = false;
        assert!(linked_peers(&config).is_empty());
        config.daemon.sharing = true;
        config.peers.remove("desk");
        assert!(linked_peers(&config).is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn a_link_backs_off_retries_on_request_and_redials_a_lost_session_at_once() {
        let mut test = harness(around(&[("zf-desk", DESK, SAVED)]), None);
        let (addresses, answer) = test.dials.recv().await.unwrap();
        assert_eq!(addresses, [SAVED.parse().unwrap()]);
        assert_eq!(*test.link.status.borrow(), Some(LinkStatus::Connecting));
        answer
            .send(Err(TransportError::InvalidAlpn.into()))
            .unwrap();

        let failed = Clock::now();
        let (_, answer) = test.dials.recv().await.unwrap();
        assert_eq!(failed.elapsed(), Duration::from_secs(1));
        assert_eq!(
            *test.link.status.borrow(),
            unreachable("Different Nidus version. Update both computers.", true)
        );
        answer.send(Err(anyhow!("timed out"))).unwrap();

        let failed = Clock::now();
        test.link.retry.send(()).unwrap();
        let (_, answer) = test.dials.recv().await.unwrap();
        assert_eq!(failed.elapsed(), Duration::ZERO);
        assert_eq!(*test.link.status.borrow(), unreachable("timed out", false));
        answer.send(Ok(7)).unwrap();
        test.link.status.wait_for(Option::is_none).await.unwrap();

        tokio::time::sleep(STABLE_SESSION).await;
        test.fake.set_session(None);
        let lost = Clock::now();
        let (_, answer) = test.dials.recv().await.unwrap();
        assert_eq!(lost.elapsed(), Duration::ZERO);
        answer.send(Ok(8)).unwrap();

        test.link.status.wait_for(Option::is_none).await.unwrap();
        test.fake.set_session(None);
        let lost = Clock::now();
        let (_, _answer) = test.dials.recv().await.unwrap();
        assert_eq!(lost.elapsed(), Duration::from_secs(1));
    }

    #[tokio::test(start_paused = true)]
    async fn a_link_dials_its_key_wherever_it_is_and_nowhere_else() {
        let mut test = harness(around(&[("zf-other", OTHER, SAVED)]), None);
        test.link.retry.send(()).unwrap();
        tokio::time::sleep(Duration::from_secs(60)).await;
        assert!(
            test.dials.try_recv().is_err(),
            "neither a computer that is not listening nor its namesake address is dialed"
        );
        assert_eq!(*test.link.status.borrow(), None);

        test.fake.set_session(Some(3));
        tokio::time::sleep(Duration::from_secs(60)).await;
        assert!(test.dials.try_recv().is_err());

        test.fake.set_session(None);
        let moved = "192.0.2.70:43119";
        test.neighbors.send_replace(around(&[
            ("zf-other", OTHER, SAVED),
            ("zf-desk", DESK, moved),
        ]));
        let (addresses, answer) = test.dials.recv().await.unwrap();
        assert_eq!(addresses, [moved.parse().unwrap()]);
        answer.send(Ok(4)).unwrap();
    }

    #[tokio::test]
    async fn learning_an_address_restarts_neither_the_link_nor_sessions() {
        let (shared, _kept) = test_daemon();
        let directory = tempfile::tempdir().unwrap();
        let desk = Identity::load_or_create(directory.path()).unwrap();
        let saved: SocketAddr = "127.0.0.1:9".parse().unwrap();
        {
            let _mutation = shared.config_mutation.lock().await;
            let mut config = shared.config.read().await.clone();
            crate::hello::trust_peer(&mut config, desk.spki(), "desk", &[saved]).unwrap();
            shared.apply_config_locked(config, true).await.unwrap();
        }
        let link = shared.links.lock().await["desk"].retry.clone();
        let generation = shared.policy_generation.load(Ordering::Acquire);

        let moved: SocketAddr = "127.0.0.2:9".parse().unwrap();
        shared.learn_address("desk", moved).await;
        let config = shared.config.read().await.clone();
        assert_eq!(config.peers["desk"].addresses, [moved, saved]);
        assert_eq!(Config::load(&shared.config_path).unwrap(), config);
        assert_eq!(shared.policy_generation.load(Ordering::Acquire), generation);
        let mut paused = config;
        paused.switching.pause_at_edges = true;
        let _mutation = shared.config_mutation.lock().await;
        shared.apply_config_locked(paused, true).await.unwrap();
        assert!(shared.links.lock().await["desk"].retry.same_channel(&link));
    }

    #[tokio::test(start_paused = true)]
    async fn a_stopped_link_stops_dialing() {
        let mut test = harness(around(&[("zf-desk", DESK, SAVED)]), None);
        let (_, answer) = test.dials.recv().await.unwrap();
        answer.send(Err(anyhow!("timed out"))).unwrap();
        let down =
            |status: &Option<LinkStatus>| matches!(status, Some(LinkStatus::Unreachable { .. }));
        test.link.status.wait_for(down).await.unwrap();
        let Link {
            retry, mut status, ..
        } = test.link;
        drop(retry);
        while status.changed().await.is_ok() {}
        tokio::time::sleep(Duration::from_secs(60)).await;
        assert!(test.dials.try_recv().is_err());
    }
}
