use std::{
    collections::{BTreeMap, BTreeSet},
    net::{IpAddr, SocketAddr},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::time::Instant;

use crate::{
    config::Config,
    hello::{hello_addresses, peer_name},
    identity::encode_hex,
    wire::{Hello, Os},
};

pub const MAX_INSTANCES: usize = 64;
pub const MAX_NEIGHBORS: usize = 32;
pub const MAX_ADDED: usize = 8;
pub const NEIGHBOR_TTL: Duration = Duration::from_secs(60);
pub const FRESH_HELLO: Duration = Duration::from_secs(30);
const HELLO_RETRIES: [Duration; 3] = [
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(15),
];
const HELLO_RETRY_LAST: Duration = Duration::from_secs(30);
const ASK_AGAIN: Duration = Duration::from_secs(15);
const HELLO_LOST: Duration = Duration::from_secs(30);
const HELLOS_PER_IP: u32 = 5;
const HELLO_WINDOW: Duration = Duration::from_secs(30);
const MAX_TRACKED_IPS: usize = 64;
const ADDED_PREFIX: &str = "address:";

pub fn fingerprint(spki: &[u8]) -> String {
    encode_hex(&Sha256::digest(spki))
}

pub fn mark(spki: &[u8]) -> String {
    let digest = Sha256::new()
        .chain_update(b"zflow mark v1")
        .chain_update(spki)
        .finalize();
    encode_hex(&digest[..3])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Via {
    Mdns,
    Address,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unplaced {
    pub id: String,
    pub name: String,
    pub os: Option<Os>,
    pub mark: Option<String>,
    pub version: Option<String>,
    pub state: UnplacedState,
    pub via: Via,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnplacedState {
    Identifying,
    Ready,
    DifferentVersion,
    DuplicateName,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Strangers {
    pub present: Vec<Stranger>,
    pub unidentified: usize,
    pub turned_away: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stranger {
    pub key: String,
    pub name: String,
    pub mark: String,
    pub answered: bool,
    pub fresh: bool,
    pub duplicate_name: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Neighbor {
    pub spki: Vec<u8>,
    pub name: String,
    pub os: Os,
    pub version: String,
    pub mark: String,
    pub addresses: Vec<SocketAddr>,
    pub via: Via,
    last_hello: Instant,
    last_seen: Instant,
}

#[derive(Debug, Clone)]
struct Instance {
    addresses: Vec<SocketAddr>,
    compatible: bool,
    name: Option<String>,
    key: Option<String>,
    asking: Asking,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Asking {
    Due,
    Waiting { again: Instant, unanswered: u32 },
    Answered { at: Instant },
}

#[derive(Debug)]
pub struct Neighbors {
    own: String,
    own_record: Option<String>,
    instances: BTreeMap<String, Instance>,
    neighbors: BTreeMap<String, Neighbor>,
    added: Vec<String>,
    hellos: BTreeMap<IpAddr, (Instant, u32)>,
    turned_away: bool,
}

impl Neighbors {
    pub fn new(own_spki: &[u8]) -> Self {
        Self {
            own: fingerprint(own_spki),
            own_record: None,
            instances: BTreeMap::new(),
            neighbors: BTreeMap::new(),
            added: Vec::new(),
            hellos: BTreeMap::new(),
            turned_away: false,
        }
    }

    pub fn turned_one_away(&mut self) {
        self.turned_away = true;
    }

    pub fn own_record(&mut self, instance: Option<String>) {
        if let Some(instance) = &instance {
            self.instances.remove(instance);
        }
        self.own_record = instance;
    }

    pub fn instance_seen(
        &mut self,
        instance: &str,
        addresses: Vec<SocketAddr>,
        compatible: bool,
        name: Option<String>,
    ) {
        if self.own_record.as_deref() == Some(instance) {
            return;
        }
        if let Some(known) = self.instances.get_mut(instance) {
            if known.addresses != addresses {
                known.asking = Asking::Due;
            }
            known.addresses = addresses;
            known.compatible = compatible;
            known.name = name;
        } else if self.instances.len() >= MAX_INSTANCES {
            self.turned_one_away();
        } else {
            self.instances.insert(
                instance.to_owned(),
                Instance {
                    addresses,
                    compatible,
                    name,
                    key: None,
                    asking: Asking::Due,
                },
            );
        }
    }

    pub fn instance_gone(&mut self, instance: &str, now: Instant) {
        if let Some(Instance { key: Some(key), .. }) = self.instances.remove(instance)
            && let Some(neighbor) = self.neighbors.get_mut(&key)
        {
            neighbor.last_seen = now;
        }
    }

    pub fn add_address(&mut self, address: SocketAddr) {
        let id = format!("{ADDED_PREFIX}{address}");
        self.added.retain(|added| *added != id);
        self.added.push(id.clone());
        if self.added.len() > MAX_ADDED {
            let oldest = self.added.remove(0);
            self.instances.remove(&oldest);
        }
        self.instances.insert(
            id,
            Instance {
                addresses: vec![address],
                compatible: true,
                name: None,
                key: None,
                asking: Asking::Due,
            },
        );
    }

    pub fn keep_fresh(&mut self, now: Instant) {
        for instance in self.instances.values_mut() {
            if let Asking::Answered { at } = instance.asking
                && now.duration_since(at) >= ASK_AGAIN
            {
                instance.asking = Asking::Due;
            }
        }
    }

    pub fn take_due_hellos(&mut self, room: usize, now: Instant) -> Vec<(String, Vec<SocketAddr>)> {
        let mut due = Vec::new();
        for (id, instance) in &mut self.instances {
            if due.len() == room {
                break;
            }
            let unanswered = match instance.asking {
                Asking::Due => 0,
                Asking::Waiting { again, unanswered } if now >= again => unanswered,
                _ => continue,
            };
            if instance.compatible {
                let again = now + HELLO_LOST;
                instance.asking = Asking::Waiting { again, unanswered };
                due.push((id.clone(), instance.addresses.clone()));
            }
        }
        due
    }

    pub fn no_answer(&mut self, instance: &str, now: Instant) {
        let Some(instance) = self.instances.get_mut(instance) else {
            return;
        };
        if let Asking::Waiting { unanswered, .. } = instance.asking {
            let wait = HELLO_RETRIES
                .get(unanswered as usize)
                .copied()
                .unwrap_or(HELLO_RETRY_LAST);
            instance.asking = Asking::Waiting {
                again: now + wait,
                unanswered: unanswered + 1,
            };
        }
    }

    pub fn allow_hello(&mut self, ip: IpAddr, local: &[IpAddr], now: Instant) -> bool {
        let ip = ip.to_canonical();
        if ip.is_loopback() || local.contains(&ip) {
            return false;
        }
        self.hellos
            .retain(|_, (start, _)| now.duration_since(*start) < HELLO_WINDOW);
        if !self.hellos.contains_key(&ip) && self.hellos.len() >= MAX_TRACKED_IPS {
            self.turned_one_away();
            return false;
        }
        let (_, count) = self.hellos.entry(ip).or_insert((now, 0));
        *count += 1;
        *count <= HELLOS_PER_IP
    }

    pub fn hello(
        &mut self,
        spki: &[u8],
        remote: SocketAddr,
        hello: &Hello,
        instance: Option<&str>,
        now: Instant,
    ) -> Option<String> {
        let key = fingerprint(spki);
        let instance = instance.and_then(|id| Some((id, self.instances.get_mut(id)?)));
        let via = match &instance {
            Some((id, _)) if id.starts_with(ADDED_PREFIX) => Via::Address,
            _ => Via::Mdns,
        };
        if let Some((_, instance)) = instance {
            instance.key = Some(key.clone());
            instance.asking = Asking::Answered { at: now };
        }
        if key == self.own {
            return None;
        }
        self.expire(now);
        if !self.neighbors.contains_key(&key) && self.neighbors.len() >= MAX_NEIGHBORS {
            self.turned_one_away();
            return None;
        }
        let neighbor = self
            .neighbors
            .entry(key.clone())
            .or_insert_with(|| Neighbor {
                spki: spki.to_vec(),
                name: String::new(),
                os: hello.os,
                version: String::new(),
                mark: mark(spki),
                addresses: Vec::new(),
                via,
                last_hello: now,
                last_seen: now,
            });
        neighbor.name = peer_name(Some(&hello.name));
        neighbor.os = hello.os;
        neighbor.version = hello
            .version
            .chars()
            .filter(char::is_ascii_graphic)
            .collect();
        neighbor.addresses = hello_addresses(remote, hello);
        if via == Via::Address {
            neighbor.via = via;
        }
        neighbor.last_hello = now;
        neighbor.last_seen = now;
        Some(key)
    }

    pub fn neighbor(&self, key: &str) -> Option<&Neighbor> {
        self.neighbors.get(key)
    }

    pub fn addresses_for_key(&self, key: &str) -> Vec<SocketAddr> {
        let mut addresses: Vec<SocketAddr> = self
            .instances
            .values()
            .filter(|instance| instance.key.as_deref() == Some(key))
            .flat_map(|instance| instance.addresses.iter().copied())
            .collect();
        if let Some(neighbor) = self.neighbors.get(key) {
            addresses.extend(&neighbor.addresses);
        }
        let mut seen = BTreeSet::new();
        addresses.retain(|address| seen.insert(*address));
        addresses
    }

    pub fn expire(&mut self, now: Instant) {
        let mapped: BTreeSet<&String> = self
            .instances
            .values()
            .filter_map(|instance| instance.key.as_ref())
            .collect();
        self.neighbors.retain(|key, neighbor| {
            mapped.contains(key)
                || now.duration_since(neighbor.last_hello.max(neighbor.last_seen)) < NEIGHBOR_TTL
        });
    }

    pub fn unplaced(&self, config: &Config) -> Vec<Unplaced> {
        let trusted = trusted_keys(config);
        let strangers = self.strangers_of(config, &trusted, None);
        let mut tiles: Vec<Unplaced> = self
            .instances
            .iter()
            .filter(|(_, instance)| instance.key.is_none())
            .map(|(id, instance)| Unplaced {
                id: format!("instance:{id}"),
                name: instance
                    .name
                    .clone()
                    .or_else(|| id.strip_prefix(ADDED_PREFIX).map(str::to_owned))
                    .unwrap_or_else(|| "Computer".into()),
                os: None,
                mark: None,
                version: None,
                state: if instance.compatible {
                    UnplacedState::Identifying
                } else {
                    UnplacedState::DifferentVersion
                },
                via: if id.starts_with(ADDED_PREFIX) {
                    Via::Address
                } else {
                    Via::Mdns
                },
            })
            .collect();
        tiles.extend(strangers.iter().map(|stranger| {
            let neighbor = &self.neighbors[&stranger.key];
            Unplaced {
                id: format!("key:{}", stranger.key),
                name: neighbor.name.clone(),
                os: Some(neighbor.os),
                mark: Some(neighbor.mark.clone()),
                version: Some(neighbor.version.clone()),
                state: if stranger.duplicate_name {
                    UnplacedState::DuplicateName
                } else {
                    UnplacedState::Ready
                },
                via: neighbor.via,
            }
        }));
        tiles.sort_by(|left, right| (&left.name, &left.id).cmp(&(&right.name, &right.id)));
        tiles
    }

    pub fn strangers(&self, config: &Config, now: Instant) -> Strangers {
        let trusted = trusted_keys(config);
        Strangers {
            present: self.strangers_of(config, &trusted, Some(now)),
            unidentified: self
                .instances
                .values()
                .filter(|instance| instance.compatible && instance.key.is_none())
                .count(),
            turned_away: self.turned_away,
        }
    }

    fn strangers_of(
        &self,
        config: &Config,
        trusted: &BTreeSet<String>,
        now: Option<Instant>,
    ) -> Vec<Stranger> {
        let untrusted: Vec<(&String, &Neighbor)> = self
            .neighbors
            .iter()
            .filter(|(key, _)| !trusted.contains(*key))
            .collect();
        let answered: BTreeSet<&String> = self
            .instances
            .values()
            .filter_map(|instance| instance.key.as_ref())
            .collect();
        untrusted
            .iter()
            .map(|(key, neighbor)| {
                let namesakes = untrusted
                    .iter()
                    .filter(|(_, other)| other.name == neighbor.name)
                    .count();
                let duplicate_name = namesakes > 1
                    || config
                        .peers
                        .keys()
                        .any(|name| peer_name(Some(name)) == neighbor.name);
                Stranger {
                    key: (*key).clone(),
                    name: neighbor.name.clone(),
                    mark: neighbor.mark.clone(),
                    answered: answered.contains(*key),
                    fresh: now
                        .is_some_and(|now| now.duration_since(neighbor.last_hello) < FRESH_HELLO),
                    duplicate_name,
                }
            })
            .collect()
    }
}

impl Neighbors {
    pub fn introduced(
        &self,
        config: &Config,
        layout: &crate::desktop::SharedLayout,
        forgotten: &crate::forgotten::Forgotten,
    ) -> Vec<String> {
        let trusted = trusted_keys(config);
        self.strangers_of(config, &trusted, None)
            .into_iter()
            .filter(|stranger| {
                stranger.answered
                    && !forgotten.contains(&stranger.key)
                    && layout.tiles.iter().any(|tile| tile.key == stranger.key)
            })
            .map(|stranger| stranger.key)
            .collect()
    }
}

fn trusted_keys(config: &Config) -> BTreeSet<String> {
    config
        .peers
        .values()
        .filter_map(|peer| peer.fingerprint_hex().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::PeerConfig, hello::make_hello};

    fn spki(byte: u8) -> Vec<u8> {
        vec![byte; 91]
    }

    fn at(address: &str) -> SocketAddr {
        address.parse().unwrap()
    }

    fn trusting(config: &mut Config, name: &str, key: &[u8]) {
        let record =
            PeerConfig::from_spki(key, Vec::new(), crate::config::PeerPermissions::default())
                .unwrap();
        config.peers.insert(name.into(), record);
    }

    #[test]
    fn marks_are_short_stable_and_differ_by_key() {
        assert_eq!(mark(&spki(1)), mark(&spki(1)));
        assert_eq!(mark(&spki(1)).len(), 6);
        assert_ne!(mark(&spki(1)), mark(&spki(2)));
        assert_eq!(fingerprint(&spki(1)).len(), 64);
    }

    #[tokio::test(start_paused = true)]
    async fn a_hello_maps_its_record_to_the_key_and_links_find_it_by_key() {
        let mut around = Neighbors::new(&spki(0));
        let desk = vec![at("192.0.2.7:43119"), at("[2001:db8::7]:43119")];
        around.instance_seen("zf-desk", desk.clone(), true, Some("desk".into()));
        around.instance_seen("zf-old", vec![at("192.0.2.9:43119")], false, None);
        let now = Instant::now();
        assert_eq!(
            around.take_due_hellos(4, now),
            [("zf-desk".to_owned(), desk.clone())]
        );
        assert!(around.take_due_hellos(4, now).is_empty());

        let hello = make_hello("desk", 43119, vec![at("100.64.0.7:43119")], true);
        let key = around
            .hello(&spki(1), desk[0], &hello, Some("zf-desk"), now)
            .unwrap();
        assert_eq!(key, fingerprint(&spki(1)));
        assert_eq!(
            around.addresses_for_key(&key),
            [desk[0], desk[1], at("100.64.0.7:43119")]
        );
        let neighbor = around.neighbor(&key).unwrap();
        assert_eq!(neighbor.name, "desk");
        assert_eq!(neighbor.mark, mark(&spki(1)));
        assert!(around.addresses_for_key(&fingerprint(&spki(2))).is_empty());

        let moved = vec![at("192.0.2.70:43119")];
        around.instance_seen("zf-desk", moved.clone(), true, Some("desk".into()));
        assert_eq!(around.take_due_hellos(4, now), [("zf-desk".into(), moved)]);

        around.instance_seen("zf-me", vec![at("192.0.2.2:43119")], true, None);
        around.take_due_hellos(4, now);
        assert_eq!(
            around.hello(&spki(0), at("192.0.2.2:43119"), &hello, Some("zf-me"), now),
            None
        );
        let shelf = around.unplaced(&Config::default());
        assert_eq!(
            shelf
                .iter()
                .map(|tile| tile.id.as_str())
                .collect::<Vec<_>>(),
            ["instance:zf-old", format!("key:{key}").as_str()]
        );
        assert_eq!(shelf[0].name, "Computer");
        assert_eq!(shelf[0].state, UnplacedState::DifferentVersion);
        assert_eq!(shelf[1].state, UnplacedState::Ready);
        assert!(hello.trusts_you);
        let tile = serde_json::to_value(&shelf[1]).unwrap();
        assert!(tile.get("trusts_you").is_none(), "{tile}");
    }

    #[tokio::test(start_paused = true)]
    async fn computers_a_peer_placed_are_introduced_unless_forgotten_here() {
        let mut around = Neighbors::new(&spki(0));
        let now = Instant::now();
        let mut found = |byte: u8, name: &str, asked: bool| {
            let address = SocketAddr::from(([192, 0, 2, byte], 43119));
            let instance = format!("zf-{name}");
            if asked {
                around.instance_seen(&instance, vec![address], true, Some(name.into()));
                around.take_due_hellos(4, now);
            }
            let hello = make_hello(name, 43119, Vec::new(), false);
            around
                .hello(
                    &spki(byte),
                    address,
                    &hello,
                    asked.then_some(&instance),
                    now,
                )
                .unwrap()
        };
        let placed = found(1, "xps", true);
        let unplaced = found(2, "desk", true);
        let came_in = found(3, "laptop", false);
        let tile = |key: &str, x| crate::desktop::Tile {
            key: key.to_owned(),
            display: None,
            x,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let layout = crate::desktop::SharedLayout {
            version: 3,
            editor: fingerprint(&spki(9)),
            tiles: vec![tile(&placed, 0), tile(&came_in, 1920)],
        };
        let directory = tempfile::tempdir().unwrap();
        let mut forgotten = crate::forgotten::Forgotten::load(directory.path());
        let mut config = Config::default();
        assert_eq!(
            around.introduced(&config, &layout, &forgotten),
            std::slice::from_ref(&placed)
        );
        assert!(
            !around
                .introduced(&config, &layout, &forgotten)
                .contains(&unplaced)
        );

        trusting(&mut config, "xps", &spki(1));
        assert!(around.introduced(&config, &layout, &forgotten).is_empty());
        forgotten.forget(&placed).unwrap();
        assert!(
            around
                .introduced(&Config::default(), &layout, &forgotten)
                .is_empty(),
            "a person here forgot it"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn unanswered_records_are_asked_again_sooner_then_later_and_in_batches() {
        let mut around = Neighbors::new(&spki(0));
        for index in 1..=6 {
            let address = SocketAddr::from(([192, 0, 2, index], 43119));
            around.instance_seen(&format!("zf-{index}"), vec![address], true, None);
        }
        let start = Instant::now();
        assert_eq!(around.take_due_hellos(4, start).len(), 4);
        assert_eq!(around.take_due_hellos(4, start).len(), 2);
        assert!(around.take_due_hellos(4, start).is_empty());
        assert_eq!(around.strangers(&Config::default(), start).unidentified, 6);
        assert_eq!(around.take_due_hellos(10, start + HELLO_LOST).len(), 6);

        let mut around = Neighbors::new(&spki(0));
        around.instance_seen("zf-desk", vec![at("192.0.2.7:43119")], true, None);
        let mut now = Instant::now();
        around.take_due_hellos(1, now);
        for wait in [2, 5, 15, 30, 30].map(Duration::from_secs) {
            around.no_answer("zf-desk", now);
            let early = now + wait - Duration::from_millis(1);
            assert!(around.take_due_hellos(1, early).is_empty(), "{wait:?}");
            now += wait;
            assert_eq!(around.take_due_hellos(1, now).len(), 1, "{wait:?}");
        }

        around.no_answer("zf-desk", now);
        let moved = at("192.0.2.70:43119");
        around.instance_seen("zf-desk", vec![moved], true, None);
        assert_eq!(around.take_due_hellos(1, now).len(), 1);
        around.no_answer("zf-desk", now);
        now += HELLO_RETRIES[0];
        assert_eq!(around.take_due_hellos(1, now).len(), 1);
        let hello = make_hello("desk", 43119, Vec::new(), false);
        around.hello(&spki(1), moved, &hello, Some("zf-desk"), now);
        around.no_answer("zf-desk", now);
        assert!(around.take_due_hellos(1, now + HELLO_LOST * 10).is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn this_computers_own_record_is_never_asked_and_holds_nothing_up() {
        let mut around = Neighbors::new(&spki(0));
        let own = vec![at("192.0.2.2:43119"), at("[2001:db8::2]:43119")];
        around.instance_seen("zf-me", own.clone(), true, Some("me".into()));
        around.own_record(Some("zf-me".into()));
        around.instance_seen("zf-me", own, true, Some("me".into()));
        around.instance_seen("zf-desk", vec![at("192.0.2.7:43119")], true, None);
        let now = Instant::now();
        let due = around.take_due_hellos(4, now);
        assert_eq!(due, [("zf-desk".into(), vec![at("192.0.2.7:43119")])]);
        let hello = make_hello("desk", 43119, Vec::new(), false);
        around.hello(
            &spki(1),
            at("192.0.2.7:43119"),
            &hello,
            Some("zf-desk"),
            now,
        );
        let strangers = around.strangers(&Config::default(), now);
        assert_eq!((strangers.present.len(), strangers.unidentified), (1, 0));
        assert!(
            around
                .unplaced(&Config::default())
                .iter()
                .all(|tile| tile.name != "me")
        );
    }

    #[tokio::test(start_paused = true)]
    async fn computers_expire_a_minute_after_their_record_and_hello() {
        let mut around = Neighbors::new(&spki(0));
        around.instance_seen("zf-desk", vec![at("192.0.2.7:43119")], true, None);
        let hello = make_hello("desk", 43119, Vec::new(), false);
        let key = around
            .hello(
                &spki(1),
                at("192.0.2.7:43119"),
                &hello,
                Some("zf-desk"),
                Instant::now(),
            )
            .unwrap();
        tokio::time::advance(NEIGHBOR_TTL * 2).await;
        around.expire(Instant::now());
        assert!(around.neighbor(&key).is_some());

        around.instance_gone("zf-desk", Instant::now());
        tokio::time::advance(NEIGHBOR_TTL - Duration::from_millis(1)).await;
        around.expire(Instant::now());
        assert!(around.neighbor(&key).is_some());
        tokio::time::advance(Duration::from_millis(1)).await;
        around.expire(Instant::now());
        assert!(around.neighbor(&key).is_none());
        assert!(around.unplaced(&Config::default()).is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn computers_and_hellos_are_capped() {
        let mut around = Neighbors::new(&spki(0));
        let now = Instant::now();
        let hello = make_hello("x", 43119, Vec::new(), false);
        for byte in 1..=MAX_NEIGHBORS as u8 {
            assert!(
                around
                    .hello(&spki(byte), at("192.0.2.7:43119"), &hello, None, now)
                    .is_some()
            );
        }
        assert!(
            around
                .hello(&spki(200), at("192.0.2.7:43119"), &hello, None, now)
                .is_none()
        );
        assert!(
            around
                .hello(&spki(1), at("192.0.2.7:43119"), &hello, None, now)
                .is_some()
        );

        let ip: IpAddr = "192.0.2.7".parse().unwrap();
        for _ in 0..HELLOS_PER_IP {
            assert!(around.allow_hello(ip, &[], now));
        }
        assert!(!around.allow_hello(ip, &[], now));
        assert!(around.allow_hello("192.0.2.8".parse().unwrap(), &[], now));
        tokio::time::advance(HELLO_WINDOW).await;
        assert!(around.allow_hello(ip, &[], Instant::now()));

        for index in 0..=MAX_INSTANCES {
            let address = SocketAddr::from(([198, 51, 100, (index % 250) as u8], 43119));
            around.instance_seen(&format!("zf-{index}"), vec![address], true, None);
        }
        assert_eq!(around.instances.len(), MAX_INSTANCES);
    }

    #[test]
    fn hellos_from_this_computer_are_never_answered() {
        let mut around = Neighbors::new(&spki(0));
        let now = Instant::now();
        let own: IpAddr = "192.0.2.2".parse().unwrap();
        for ip in [
            "127.0.0.1",
            "127.0.0.53",
            "::1",
            "::ffff:127.0.0.1",
            "192.0.2.2",
            "::ffff:192.0.2.2",
        ] {
            assert!(
                !around.allow_hello(ip.parse().unwrap(), &[own], now),
                "{ip}"
            );
        }
        assert!(around.allow_hello("192.0.2.3".parse().unwrap(), &[own], now));
    }

    #[tokio::test(start_paused = true)]
    async fn only_a_key_found_by_this_computers_own_hello_has_answered() {
        let mut around = Neighbors::new(&spki(0));
        let now = Instant::now();
        let desk = at("192.0.2.7:43119");
        let hello = make_hello("desk", 43119, Vec::new(), false);
        let answered =
            |around: &Neighbors| around.strangers(&Config::default(), now).present[0].answered;
        let key = around.hello(&spki(1), desk, &hello, None, now).unwrap();
        assert!(!answered(&around));
        assert_eq!(
            around.unplaced(&Config::default())[0].id,
            format!("key:{key}")
        );
        around.instance_seen("zf-desk", vec![desk], true, Some("desk".into()));
        around.take_due_hellos(4, now);
        around.hello(&spki(1), desk, &hello, Some("zf-desk"), now);
        assert!(answered(&around));
        around.instance_gone("zf-desk", now);
        assert!(!answered(&around));
        around.add_address(desk);
        let added = format!("address:{desk}");
        around.hello(&spki(1), desk, &hello, Some(&added), now);
        assert!(answered(&around));
    }

    #[tokio::test(start_paused = true)]
    async fn a_full_table_says_something_was_turned_away() {
        let now = Instant::now();
        let turned_away =
            |around: &Neighbors| around.strangers(&Config::default(), now).turned_away;
        let hello = make_hello("x", 43119, Vec::new(), false);
        let remote = at("192.0.2.7:43119");

        let mut around = Neighbors::new(&spki(0));
        for index in 0..MAX_INSTANCES {
            let address = SocketAddr::from(([198, 51, 100, (index % 250) as u8], 43119));
            around.instance_seen(&format!("zf-{index}"), vec![address], true, None);
        }
        around.instance_seen("zf-0", vec![remote], true, None);
        assert!(!turned_away(&around));
        around.instance_seen("zf-new", vec![remote], true, None);
        assert!(turned_away(&around));

        let mut around = Neighbors::new(&spki(0));
        for byte in 1..=MAX_NEIGHBORS as u8 {
            around.hello(&spki(byte), remote, &hello, None, now);
        }
        assert!(!turned_away(&around));
        around.hello(&spki(200), remote, &hello, None, now);
        assert!(turned_away(&around));

        let mut around = Neighbors::new(&spki(0));
        for host in 0..MAX_TRACKED_IPS as u32 {
            let ip = std::net::Ipv4Addr::from(0xc633_6400 + host);
            assert!(around.allow_hello(ip.into(), &[], now));
        }
        let first: IpAddr = "198.51.100.0".parse().unwrap();
        for _ in 0..HELLOS_PER_IP {
            around.allow_hello(first, &[], now);
        }
        assert!(!turned_away(&around));
        assert!(!around.allow_hello("192.0.2.99".parse().unwrap(), &[], now));
        assert!(turned_away(&around));

        let mut around = Neighbors::new(&spki(0));
        around.turned_one_away();
        assert!(turned_away(&around));
    }

    #[tokio::test(start_paused = true)]
    async fn the_shelf_leaves_out_trusted_keys_and_flags_namesakes() {
        let mut around = Neighbors::new(&spki(0));
        let now = Instant::now();
        let say = |name: &str| make_hello(name, 43119, Vec::new(), false);
        let remote = at("192.0.2.7:43119");
        let desk = around
            .hello(&spki(1), remote, &say("desk"), None, now)
            .unwrap();
        let twin = around
            .hello(&spki(2), remote, &say("desk"), None, now)
            .unwrap();
        let laptop = around
            .hello(&spki(3), remote, &say("laptop"), None, now)
            .unwrap();
        let mut config = Config::default();
        let shelf = around.unplaced(&config);
        assert_eq!(shelf.len(), 3);
        let state = |shelf: &[Unplaced], key: &str| {
            shelf
                .iter()
                .find(|tile| tile.id == format!("key:{key}"))
                .map(|tile| tile.state)
        };
        assert_eq!(state(&shelf, &desk), Some(UnplacedState::DuplicateName));
        assert_eq!(state(&shelf, &laptop), Some(UnplacedState::Ready));

        trusting(&mut config, "desk", &spki(1));
        let shelf = around.unplaced(&config);
        assert_eq!(state(&shelf, &desk), None);
        assert_eq!(state(&shelf, &twin), Some(UnplacedState::DuplicateName));
        config.peers.clear();
        assert_eq!(around.unplaced(&config).len(), 3);
        let strangers = around.strangers(&config, now);
        assert_eq!(strangers.present.len(), 3);
        assert!(strangers.present.iter().all(|stranger| stranger.fresh));
        tokio::time::advance(FRESH_HELLO).await;
        let stale = around.strangers(&config, Instant::now());
        assert!(stale.present.iter().all(|stranger| !stranger.fresh));
    }

    #[tokio::test(start_paused = true)]
    async fn an_open_window_asks_a_record_again_before_its_hello_goes_stale() {
        let mut around = Neighbors::new(&spki(0));
        let desk = at("192.168.1.20:43119");
        around.instance_seen("zf-desk", vec![desk], true, None);
        let start = Instant::now();
        around.take_due_hellos(4, start);
        let hello = make_hello("desk", 43119, Vec::new(), false);
        around.hello(&spki(1), desk, &hello, Some("zf-desk"), start);
        let later = start + Duration::from_secs(60);
        assert!(around.take_due_hellos(4, later).is_empty());
        assert!(!around.strangers(&Config::default(), later).present[0].fresh);

        around.keep_fresh(start + ASK_AGAIN - Duration::from_millis(1));
        assert!(around.take_due_hellos(4, later).is_empty());
        around.keep_fresh(later);
        assert_eq!(
            around.take_due_hellos(4, later),
            [("zf-desk".to_owned(), vec![desk])]
        );
        around.hello(&spki(1), desk, &hello, Some("zf-desk"), later);
        let strangers = around.strangers(&Config::default(), later);
        assert!(strangers.present[0].fresh && strangers.present[0].answered);
        assert_eq!(strangers.unidentified, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn an_added_address_is_asked_like_a_record_and_found_by_key() {
        let mut around = Neighbors::new(&spki(0));
        let tailnet = at("100.64.0.7:43119");
        around.add_address(tailnet);
        let shelf = around.unplaced(&Config::default());
        assert_eq!(shelf[0].name, "100.64.0.7:43119");
        assert_eq!(shelf[0].via, Via::Address);
        let now = Instant::now();
        let due = around.take_due_hellos(4, now);
        assert_eq!(due, [(format!("address:{tailnet}"), vec![tailnet])]);
        let hello = make_hello("far", 43119, Vec::new(), false);
        let key = around
            .hello(&spki(1), tailnet, &hello, Some(&due[0].0), now)
            .unwrap();
        assert_eq!(around.neighbor(&key).unwrap().via, Via::Address);
        assert_eq!(around.addresses_for_key(&key), [tailnet]);

        for host in 1..=MAX_ADDED as u8 {
            around.add_address(SocketAddr::from(([100, 64, 1, host], 43119)));
        }
        assert!(around.addresses_for_key(&key).contains(&tailnet));
        assert!(!around.instances.contains_key(&format!("address:{tailnet}")));
    }
}
