use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use super::model::{Contents, Document};
use crate::desktop::{Display, Edge, MAX_SHARED_TILES, SharedLayout, Tile};

pub const MAX_MONITORS: usize = MAX_SHARED_TILES;
pub const MAX_COORDINATE: i32 = 100_000;
pub const MAX_DIMENSION: u32 = 16_384;
/// The farthest a move reaches for an edge to snap to.
pub const MAX_SNAP: u32 = 2048;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Layout {
    pub monitors: Vec<Monitor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Monitor {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<Display>,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Transition {
    pub source: usize,
    pub target: usize,
    pub edge: Edge,
    pub source_start: f64,
    pub source_end: f64,
    pub target_start: f64,
    pub target_end: f64,
}

/// Where two screens touch: the line the settings window draws between
/// them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Seam {
    /// The screen left of the line, or above it.
    pub a: String,
    /// The screen right of the line, or below it.
    pub b: String,
    /// Whether the screens sit side by side, so the line stands upright.
    pub vertical: bool,
    /// The line's x when it stands upright, else its y.
    pub at: i32,
    /// Where the line starts and ends along its length.
    pub start: i32,
    pub end: i32,
    /// Whether both screens belong to one computer. The pointer goes
    /// between those as that computer's own display settings say, so it
    /// crosses to another computer only at the other seams.
    pub same_computer: bool,
}

/// What a move would do, for the window to show while a screen is dragged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preview {
    /// Where the dragged screen would land.
    pub x: i32,
    pub y: i32,
    /// The seams of the layout the move would make.
    pub seams: Vec<Seam>,
}

impl Monitor {
    pub fn active(&self) -> bool {
        self.display.as_ref().is_none_or(|d| d.active)
    }

    fn bounds(&self) -> (i64, i64, i64, i64) {
        let (x, y) = (i64::from(self.x), i64::from(self.y));
        (x, y, x + i64::from(self.width), y + i64::from(self.height))
    }

    fn overlaps(&self, other: &Self) -> bool {
        let (left, top, right, bottom) = self.bounds();
        let (other_left, other_top, other_right, other_bottom) = other.bounds();
        left < other_right && right > other_left && top < other_bottom && bottom > other_top
    }

    fn valid_geometry(&self) -> bool {
        (-MAX_COORDINATE..=MAX_COORDINATE).contains(&self.x)
            && (-MAX_COORDINATE..=MAX_COORDINATE).contains(&self.y)
            && (1..=MAX_DIMENSION).contains(&self.width)
            && (1..=MAX_DIMENSION).contains(&self.height)
    }
}

impl Layout {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.monitors.len() <= MAX_MONITORS,
            "Use at most {MAX_MONITORS} monitors."
        );
        let mut ids = BTreeSet::new();
        for (index, monitor) in self.monitors.iter().enumerate() {
            for value in [&monitor.id, &monitor.label]
                .into_iter()
                .chain(monitor.peer.iter())
            {
                ensure!(
                    !value.trim().is_empty()
                        && value.len() <= 256
                        && !value.chars().any(char::is_control),
                    "Monitor IDs, labels, and peer names must contain 1 to 256 bytes without control characters."
                );
            }
            ensure!(ids.insert(&monitor.id), "Monitor IDs must be unique.");
            if let Some(display) = &monitor.display {
                display.validate()?;
            }
            ensure!(
                monitor.valid_geometry(),
                "Monitor coordinates must be within ±{MAX_COORDINATE}; dimensions must be between 1 and {MAX_DIMENSION}."
            );
            for other in &self.monitors[..index] {
                ensure!(
                    !monitor.active() || !other.active() || !monitor.overlaps(other),
                    "Monitors {} and {} overlap.",
                    monitor.label,
                    other.label
                );
            }
        }
        Ok(())
    }

    /// The shared form of this layout. `keys` maps each paired computer's
    /// name here to its key fingerprint. A tile for a computer that is not
    /// paired is left out; `SharedLayout::with_others_from` puts back the
    /// ones other computers placed.
    pub fn to_shared(
        &self,
        version: u64,
        own: &str,
        keys: &BTreeMap<String, String>,
    ) -> SharedLayout {
        let mut seen = BTreeSet::new();
        let tiles = self
            .monitors
            .iter()
            .filter_map(|monitor| {
                let key = match &monitor.peer {
                    None => own.to_owned(),
                    Some(name) => keys.get(name)?.clone(),
                };
                seen.insert((key.clone(), monitor.display.as_ref().map(|d| d.id.clone())))
                    .then_some(Tile {
                        key,
                        display: monitor.display.clone(),
                        x: monitor.x,
                        y: monitor.y,
                        width: monitor.width,
                        height: monitor.height,
                    })
            })
            .take(MAX_SHARED_TILES)
            .collect();
        SharedLayout {
            version,
            editor: own.to_owned(),
            tiles,
        }
    }

    /// This computer's view of a shared layout: its own tile is the local
    /// one, and paired computers get their names here. Tiles of computers
    /// that are not paired here are left out.
    pub fn from_shared(
        shared: &SharedLayout,
        own: &str,
        own_label: &str,
        keys: &BTreeMap<String, String>,
    ) -> Self {
        let names: BTreeMap<&str, &str> = keys
            .iter()
            .map(|(name, key)| (key.as_str(), name.as_str()))
            .collect();
        let monitors = shared
            .tiles
            .iter()
            .filter_map(|tile| {
                let (id, label, peer) = if tile.key == own {
                    ("local".to_owned(), own_label.to_owned(), None)
                } else {
                    let name = *names.get(tile.key.as_str())?;
                    (
                        format!("peer:{name}"),
                        name.to_owned(),
                        Some(name.to_owned()),
                    )
                };
                Some(Monitor {
                    id: tile
                        .display
                        .as_ref()
                        .map_or(id.clone(), |d| format!("{id}/{}", d.id)),
                    label: tile
                        .display
                        .as_ref()
                        .map_or(label.clone(), |d| format!("{} · {label}", d.name)),
                    peer,
                    display: tile.display.clone(),
                    x: tile.x,
                    y: tile.y,
                    width: tile.width,
                    height: tile.height,
                })
            })
            .collect();
        Self { monitors }
    }

    pub fn transitions(&self) -> Vec<Transition> {
        if self.validate().is_err() {
            return Vec::new();
        }
        let mut transitions = Vec::new();
        for (source, a) in self.monitors.iter().enumerate() {
            for (target, b) in self.monitors.iter().enumerate() {
                if source == target || a.peer == b.peer || !a.active() || !b.active() {
                    continue;
                }
                let (al, at, ar, ab) = a.bounds();
                let (bl, bt, br, bb) = b.bounds();
                let edge = if ar == bl {
                    Some(Edge::Right)
                } else if al == br {
                    Some(Edge::Left)
                } else if ab == bt {
                    Some(Edge::Bottom)
                } else if at == bb {
                    Some(Edge::Top)
                } else {
                    None
                };
                let Some(edge) = edge else { continue };
                let (a_start, a_end, b_start, b_end) = match edge {
                    Edge::Left | Edge::Right => (at, ab, bt, bb),
                    Edge::Top | Edge::Bottom => (al, ar, bl, br),
                };
                let start = a_start.max(b_start);
                let end = a_end.min(b_end);
                if start < end {
                    // The desk arrangement takes priority even when the OS has
                    // another local display across this edge. Native adapters
                    // intercept these crossings before the cursor leaves it.
                    transitions.push(Transition {
                        source,
                        target,
                        edge,
                        source_start: (start - a_start) as f64 / (a_end - a_start) as f64,
                        source_end: (end - a_start) as f64 / (a_end - a_start) as f64,
                        target_start: (start - b_start) as f64 / (b_end - b_start) as f64,
                        target_end: (end - b_start) as f64 / (b_end - b_start) as f64,
                    });
                }
            }
        }
        transitions
    }

    /// Where screens touch, each pair once. Inactive screens touch nothing.
    pub fn seams(&self) -> Vec<Seam> {
        if self.validate().is_err() {
            return Vec::new();
        }
        let mut seams = Vec::new();
        for (index, one) in self.monitors.iter().enumerate() {
            for other in &self.monitors[index + 1..] {
                if !one.active() || !other.active() {
                    continue;
                }
                let (ol, ot, or, ob) = one.bounds();
                let (tl, tt, tr, tb) = other.bounds();
                let (a, b, vertical, at, start, end) = if or == tl {
                    (one, other, true, or, ot.max(tt), ob.min(tb))
                } else if ol == tr {
                    (other, one, true, ol, ot.max(tt), ob.min(tb))
                } else if ob == tt {
                    (one, other, false, ob, ol.max(tl), or.min(tr))
                } else if ot == tb {
                    (other, one, false, ot, ol.max(tl), or.min(tr))
                } else {
                    continue;
                };
                if start >= end {
                    continue;
                }
                // Valid geometry keeps every edge well inside i32.
                seams.push(Seam {
                    a: a.id.clone(),
                    b: b.id.clone(),
                    vertical,
                    at: at as i32,
                    start: start as i32,
                    end: end as i32,
                    same_computer: one.peer == other.peer,
                });
            }
        }
        seams
    }

    /// Snap to a nearby shared edge, or keep a valid free position. Reject overlap.
    pub fn snap_move(&self, index: usize, x: i32, y: i32, tolerance: i32) -> Option<(i32, i32)> {
        let moving = self.monitors.get(index)?;
        let (dx, dy) = self.snap(
            &[index],
            i64::from(x) - i64::from(moving.x),
            i64::from(y) - i64::from(moving.y),
            tolerance,
        )?;
        Some((moving.x + dx, moving.y + dy))
    }

    /// How far to move the screens at `members`, all alike, to go `(dx, dy)`
    /// or against an edge of another screen within `tolerance` of there, as
    /// [`Layout::snap_move`] does for one. The members keep where they are
    /// to each other, so a computer moves with its own arrangement.
    pub fn snap_group(
        &self,
        members: &[usize],
        dx: i32,
        dy: i32,
        tolerance: i32,
    ) -> Option<(i32, i32)> {
        self.snap(members, i64::from(dx), i64::from(dy), tolerance)
    }

    /// The nearest edge within `tolerance` where nothing overlaps wins, else
    /// `(dx, dy)` itself where nothing overlaps there. Each member is tried
    /// against each side of each other screen, lined up with that side's
    /// start, end or middle when one is within `tolerance` too.
    fn snap(&self, members: &[usize], dx: i64, dy: i64, tolerance: i32) -> Option<(i32, i32)> {
        if !(0..=MAX_COORDINATE).contains(&tolerance) || members.is_empty() {
            return None;
        }
        let moving = members
            .iter()
            .map(|&index| self.monitors.get(index))
            .collect::<Option<Vec<_>>>()?;
        if !moving.iter().any(|monitor| monitor.active()) {
            return None;
        }
        let others: Vec<_> = self
            .monitors
            .iter()
            .enumerate()
            .filter(|(index, other)| !members.contains(index) && other.active())
            .map(|(_, other)| other.bounds())
            .collect();
        let range = i64::from(-MAX_COORDINATE)..=i64::from(MAX_COORDINATE);
        // Bounds, not clones: a drag asks this for every pointer motion.
        let fits = |ex: i64, ey: i64| {
            moving
                .iter()
                .all(|monitor| {
                    let (left, top, right, bottom) = monitor.bounds();
                    let (left, top, right, bottom) = (left + ex, top + ey, right + ex, bottom + ey);
                    range.contains(&left)
                        && range.contains(&top)
                        && monitor.valid_geometry()
                        && (!monitor.active()
                            || others.iter().all(|&(ol, ot, or, ob)| {
                                left >= or || right <= ol || top >= ob || bottom <= ot
                            }))
                })
                .then_some(())?;
            Some((i32::try_from(ex).ok()?, i32::try_from(ey).ok()?))
        };
        let tolerance = i64::from(tolerance);
        let mut best: Option<(i64, (i32, i32))> = None;
        for monitor in moving.iter().filter(|monitor| monitor.active()) {
            let (x, y) = (i64::from(monitor.x), i64::from(monitor.y));
            let (width, height) = (i64::from(monitor.width), i64::from(monitor.height));
            let (to_x, to_y) = (x + dx, y + dy);
            for &(left, top, right, bottom) in &others {
                // Each side of the other screen: where the dragged one would
                // touch it, and the span along that side.
                let sides = [
                    (true, left - width, top, bottom, height, to_y),
                    (true, right, top, bottom, height, to_y),
                    (false, top - height, left, right, width, to_x),
                    (false, bottom, left, right, width, to_x),
                ];
                for (upright, across, start, end, size, dragged) in sides {
                    // Lined up with the side's start, end or middle beats
                    // where it was dragged, when that is near enough.
                    let mut lined_up = [start, end - size, start + (end - start - size) / 2];
                    lined_up.sort_by_key(|along| (along - dragged).abs());
                    let along = lined_up
                        .into_iter()
                        .filter(|along| (along - dragged).abs() <= tolerance)
                        .chain([dragged]);
                    for along in along {
                        if along >= end || along + size <= start {
                            continue;
                        }
                        let (cx, cy) = if upright {
                            (across, along)
                        } else {
                            (along, across)
                        };
                        let (ex, ey) = (cx - x, cy - y);
                        let (off_x, off_y) = (ex - dx, ey - dy);
                        if off_x.abs() > tolerance || off_y.abs() > tolerance {
                            continue;
                        }
                        let Some(delta) = fits(ex, ey) else {
                            continue;
                        };
                        let distance = off_x * off_x + off_y * off_y;
                        if best.is_none_or(|(best_distance, _)| distance < best_distance) {
                            best = Some((distance, delta));
                        }
                        break;
                    }
                }
            }
        }
        best.map(|(_, delta)| delta).or_else(|| fits(dx, dy))
    }

    /// This layout with screen `id` at `(x, y)`, or against an edge within
    /// `tolerance` of there. With `computer`, every screen of its computer
    /// moves alike.
    pub fn moved(&self, id: &str, x: i32, y: i32, tolerance: u32, computer: bool) -> Result<Self> {
        let index = self
            .monitors
            .iter()
            .position(|monitor| monitor.id == id)
            .with_context(|| format!("The layout has no screen {id}"))?;
        let anchor = &self.monitors[index];
        let members: Vec<usize> = if computer {
            (0..self.monitors.len())
                .filter(|&other| self.monitors[other].peer == anchor.peer)
                .collect()
        } else {
            vec![index]
        };
        let (dx, dy) = self
            .snap(
                &members,
                i64::from(x) - i64::from(anchor.x),
                i64::from(y) - i64::from(anchor.y),
                tolerance.min(MAX_SNAP) as i32,
            )
            .context("Screens cannot overlap")?;
        let mut moved = self.clone();
        for member in members {
            moved.monitors[member].x += dx;
            moved.monitors[member].y += dy;
        }
        Ok(moved)
    }

    /// What [`Layout::moved`] would make, without keeping it. None where
    /// the screens cannot go.
    pub fn preview(
        &self,
        id: &str,
        x: i32,
        y: i32,
        tolerance: u32,
        computer: bool,
    ) -> Option<Preview> {
        let moved = self.moved(id, x, y, tolerance, computer).ok()?;
        let screen = moved.monitors.iter().find(|monitor| monitor.id == id)?;
        Some(Preview {
            x: screen.x,
            y: screen.y,
            seams: moved.seams(),
        })
    }
}

pub type LayoutDocument = Document<Layout>;

impl Contents for Layout {
    const NAME: &'static str = "layout";

    fn missing(_: &Path) -> Self {
        Self::default()
    }

    fn check_read(&self) -> Result<()> {
        self.validate()
    }
}

impl LayoutDocument {
    /// Opens the layout saved beside the configuration as `NAME.layout.toml`.
    pub fn beside(config_path: &Path) -> Result<Self> {
        let mut file_name = config_path
            .file_name()
            .context("Configuration path needs a filename")?
            .to_os_string();
        file_name.push(".layout.toml");
        Self::open(config_path.with_file_name(file_name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn monitor(id: &str, x: i32, y: i32, width: u32, height: u32) -> Monitor {
        Monitor {
            display: None,
            id: id.into(),
            label: id.into(),
            peer: (id != "local").then(|| id.into()),
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn a_shared_layout_reads_the_same_arrangement_on_both_computers() {
        let (mac, ubuntu) = (format!("{:064x}", 1), format!("{:064x}", 2));
        // The Mac's layout, with a computer it paired and one it forgot.
        let on_mac = Layout {
            monitors: vec![
                monitor("local", 0, 0, 3008, 1692),
                monitor("ubuntu", 3008, 0, 2560, 1440),
                monitor("gone", -1000, 0, 1000, 800),
            ],
        };
        let mac_keys = BTreeMap::from([("ubuntu".to_owned(), ubuntu.clone())]);
        let shared = on_mac.to_shared(7, &mac, &mac_keys);
        shared.validate().unwrap();
        assert_eq!((shared.version, &shared.editor), (7, &mac));
        assert_eq!(shared.tiles.len(), 2, "the forgotten computer is left out");

        // Ubuntu calls the Mac "MacBook"; its own tile becomes the local one.
        let ubuntu_keys = BTreeMap::from([("MacBook".to_owned(), mac.clone())]);
        let on_ubuntu = Layout::from_shared(&shared, &ubuntu, "This computer", &ubuntu_keys);
        let local = on_ubuntu
            .monitors
            .iter()
            .find(|m| m.peer.is_none())
            .unwrap();
        assert_eq!(
            (local.id.as_str(), local.x, local.width),
            ("local", 3008, 2560)
        );
        let peer = on_ubuntu
            .monitors
            .iter()
            .find(|m| m.peer.is_some())
            .unwrap();
        assert_eq!(
            (peer.id.as_str(), peer.label.as_str(), peer.x),
            ("peer:MacBook", "MacBook", 0)
        );
        assert_eq!(
            on_ubuntu.transitions().len(),
            2,
            "one edge in each direction"
        );

        // A computer Ubuntu has not paired is not placed there.
        let stranger = Layout::from_shared(&shared, &ubuntu, "This computer", &BTreeMap::new());
        assert_eq!(stranger.monitors.len(), 1);
    }

    #[test]
    fn a_peers_layout_adds_no_trust() {
        // A peer placed a computer this one has only found on its shelf.
        let [own, desk, found] = [1, 2, 3].map(|n| format!("{n:064x}"));
        let tile = |key: &String, x| Tile {
            display: None,
            key: key.clone(),
            x,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let from_desk = SharedLayout {
            version: 4,
            editor: desk.clone(),
            tiles: vec![tile(&own, 0), tile(&desk, 1920), tile(&found, -1920)],
        };
        let keys = BTreeMap::from([("desk".to_owned(), desk)]);
        // Its tile is not shown here, so no crossing leads to it. The next
        // layout this computer writes keeps it where the peer put it.
        let here = Layout::from_shared(&from_desk, &own, "This computer", &keys);
        let ids: Vec<&str> = here.monitors.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["local", "peer:desk"]);
        let written = here.to_shared(5, &own, &keys);
        assert!(written.tiles.iter().all(|tile| tile.key != found));
        let written = written.with_others_from(&from_desk, |key| {
            key == own || keys.values().any(|k| k == key)
        });
        assert!(written.tiles.contains(&tile(&found, -1920)));
    }

    #[test]
    fn touching_edges_have_reciprocal_normalized_ranges() {
        let layout = Layout {
            monitors: vec![
                monitor("local", 0, 0, 100, 100),
                monitor("peer", 100, 50, 200, 100),
            ],
        };
        let links = layout.transitions();
        assert_eq!(
            links,
            vec![
                Transition {
                    source: 0,
                    target: 1,
                    edge: Edge::Right,
                    source_start: 0.5,
                    source_end: 1.0,
                    target_start: 0.0,
                    target_end: 0.5
                },
                Transition {
                    source: 1,
                    target: 0,
                    edge: Edge::Left,
                    source_start: 0.0,
                    source_end: 0.5,
                    target_start: 0.5,
                    target_end: 1.0
                },
            ]
        );
        let source_position = 0.75;
        let link = &links[0];
        let mapped = link.target_start
            + (source_position - link.source_start) / (link.source_end - link.source_start)
                * (link.target_end - link.target_start);
        assert_eq!(mapped, 0.25);
    }

    #[test]
    fn stacked_monitors_split_an_edge_and_skip_same_owner() {
        let mut layout = Layout {
            monitors: vec![
                monitor("local", 0, 0, 100, 100),
                monitor("a", 100, 0, 100, 50),
                monitor("b", 100, 50, 100, 50),
            ],
        };
        layout.monitors[2].peer = Some("a".into());
        let links = layout.transitions();
        assert_eq!(links.len(), 4);
        assert_eq!((links[0].source_start, links[0].source_end), (0.0, 0.5));
        assert_eq!((links[1].source_start, links[1].source_end), (0.5, 1.0));
        layout.monitors[1].peer = None;
        layout.monitors[2].peer = None;
        assert!(layout.transitions().is_empty());
    }

    #[test]
    fn vertical_links_and_gaps_and_corners() {
        let mut layout = Layout {
            monitors: vec![
                monitor("local", -50, -100, 100, 100),
                monitor("a", 0, 0, 100, 100),
            ],
        };
        let links = layout.transitions();
        assert_eq!(links[0].edge, Edge::Bottom);
        assert_eq!(links[1].edge, Edge::Top);
        assert_eq!(links[0].source_start, 0.5);
        layout.monitors[1].y = 1;
        assert!(layout.transitions().is_empty());
        layout.monitors[1].y = 0;
        layout.monitors[1].x = 50;
        assert!(layout.transitions().is_empty());
    }

    #[test]
    fn invalid_geometry_names_and_overlaps_fail_validation() {
        let good = Layout {
            monitors: vec![
                monitor("local", 0, 0, 100, 100),
                monitor("peer", 100, 0, 100, 100),
            ],
        };
        for bad in [
            Monitor {
                x: 99,
                ..good.monitors[1].clone()
            },
            Monitor {
                x: i32::MIN,
                ..good.monitors[1].clone()
            },
            Monitor {
                width: u32::MAX,
                ..good.monitors[1].clone()
            },
            Monitor {
                height: 0,
                ..good.monitors[1].clone()
            },
            Monitor {
                id: "local".into(),
                ..good.monitors[1].clone()
            },
            Monitor {
                label: " ".into(),
                ..good.monitors[1].clone()
            },
            Monitor {
                peer: Some("".into()),
                ..good.monitors[1].clone()
            },
        ] {
            let layout = Layout {
                monitors: vec![good.monitors[0].clone(), bad],
            };
            assert!(layout.validate().is_err());
            assert!(layout.transitions().is_empty());
        }
        let mut same_labels = good;
        same_labels.monitors[1].label = "local".into();
        same_labels.validate().unwrap();
    }

    #[test]
    fn snapping_uses_nearest_edge_and_rejects_overlap() {
        let layout = Layout {
            monitors: vec![
                monitor("local", 0, 0, 100, 100),
                monitor("peer", 200, 0, 100, 100),
            ],
        };
        assert_eq!(layout.snap_move(1, 106, 20, 10), Some((100, 20)));
        assert_eq!(layout.snap_move(1, -108, 20, 10), Some((-100, 20)));
        assert_eq!(layout.snap_move(1, 20, 108, 10), Some((20, 100)));
        assert_eq!(layout.snap_move(1, 20, -108, 10), Some((20, -100)));
        assert_eq!(layout.snap_move(1, 96, 20, 10), Some((100, 20)));
        assert_eq!(layout.snap_move(1, 50, 0, 10), None);
        assert_eq!(layout.snap_move(1, 200, 200, 10), Some((200, 200)));
        assert_eq!(layout.snap_move(1, i32::MAX, 0, 10), None);
        assert_eq!(layout.snap_move(1, i32::MIN, i32::MIN, 10), None);
        assert_eq!(layout.snap_move(2, 0, 0, 10), None);
        assert_eq!(layout.snap_move(1, 100, 0, -1), None);
    }

    #[test]
    fn snapping_lines_up_middles() {
        let layout = Layout {
            monitors: vec![
                monitor("local", 0, 0, 300, 200),
                monitor("laptop", 1000, 1000, 100, 60),
            ],
        };
        // Under the monitor, a little off its middle.
        assert_eq!(layout.snap_move(1, 104, 205, 10), Some((100, 200)));
        // Beside it, level with its middle.
        assert_eq!(layout.snap_move(1, 303, 66, 10), Some((300, 70)));
    }

    #[test]
    fn seams_are_where_screens_touch() {
        let mut layout = Layout {
            monitors: vec![
                monitor("local", 0, 0, 100, 100),
                monitor("local/2", 100, 0, 100, 100),
                monitor("desk", 200, 50, 100, 100),
                monitor("laptop", 50, 100, 100, 50),
            ],
        };
        layout.monitors[1].peer = None;
        let seams = layout.seams();
        let seam = |a: &str, b: &str, vertical, at, start, end, same_computer| Seam {
            a: a.into(),
            b: b.into(),
            vertical,
            at,
            start,
            end,
            same_computer,
        };
        assert_eq!(
            seams,
            [
                seam("local", "local/2", true, 100, 0, 100, true),
                seam("local", "laptop", false, 100, 50, 100, false),
                seam("local/2", "desk", true, 200, 50, 100, false),
                seam("local/2", "laptop", false, 100, 100, 150, false),
            ]
        );
        // An unplugged screen keeps its place but touches nothing.
        layout.monitors[1].display = Some(Display {
            id: "2".into(),
            name: "Studio Display".into(),
            bounds: crate::desktop::Rect {
                x: 0,
                y: 0,
                width: 100,
                height: 100,
            },
            width_mm: 0,
            height_mm: 0,
            active: false,
        });
        assert_eq!(layout.seams().len(), 1);
    }

    #[test]
    fn a_computer_moves_with_its_own_arrangement() {
        // This computer has two screens side by side; the laptop is apart.
        let mut layout = Layout {
            monitors: vec![
                monitor("local", 0, 0, 100, 100),
                monitor("local/2", 100, 20, 100, 60),
                monitor("laptop", 500, 500, 80, 50),
            ],
        };
        layout.monitors[1].peer = None;
        // Dragged by its first screen, the second one lands against the
        // laptop and pulls the first along.
        let moved = layout.moved("local", 296, 470, 10, true).unwrap();
        let at = |layout: &Layout, id: &str| {
            let screen = layout.monitors.iter().find(|m| m.id == id).unwrap();
            (screen.x, screen.y)
        };
        assert_eq!(at(&moved, "local/2"), (400, 490));
        assert_eq!(at(&moved, "local"), (300, 470));
        assert_eq!(at(&moved, "laptop"), (500, 500));
        assert_eq!(
            layout.snap_group(&[0, 1], 296, 470, 10),
            Some((300, 470)),
            "the same, by index"
        );
        // One screen alone leaves its neighbor where it was.
        let alone = layout.moved("local/2", 420, 20, 0, false).unwrap();
        assert_eq!(
            (at(&alone, "local"), at(&alone, "local/2")),
            ((0, 0), (420, 20))
        );
        // The whole computer cannot go where any of its screens overlaps.
        let error = layout.moved("local", 420, 450, 0, true).unwrap_err();
        assert!(error.to_string().contains("overlap"), "{error}");
        let error = layout.moved("gone", 0, 0, 0, true).unwrap_err();
        assert!(error.to_string().contains("no screen gone"), "{error}");
        assert_eq!(layout.snap_group(&[0, 9], 1, 1, 0), None);
        assert_eq!(layout.snap_group(&[], 1, 1, 0), None);
    }

    #[test]
    fn a_preview_shows_the_landing_and_its_seams_without_moving() {
        let layout = Layout {
            monitors: vec![
                monitor("local", 0, 0, 100, 100),
                monitor("desk", 300, 0, 100, 100),
            ],
        };
        let preview = layout.preview("desk", 104, 3, 10, true).unwrap();
        assert_eq!((preview.x, preview.y), (100, 0));
        assert_eq!(preview.seams.len(), 1);
        assert!(!preview.seams[0].same_computer);
        assert_eq!(layout.monitors[1].x, 300, "nothing moved");
        assert_eq!(layout.preview("desk", 50, 0, 0, true), None);
    }

    #[test]
    fn a_step_shorter_than_the_tolerance_leaves_a_seam() {
        // Arrow keys move 100 units and snap within 99, so a step away from
        // the seam is not pulled back to it, and a step into a neighbor
        // stops against it.
        let layout = Layout {
            monitors: vec![
                monitor("local", 0, 0, 100, 100),
                monitor("desk", 100, 0, 100, 100),
            ],
        };
        let away = layout.moved("desk", 200, 0, 99, true).unwrap();
        assert_eq!(away.monitors[1].x, 200);
        assert!(away.seams().is_empty());
        let back = away.moved("desk", 130, 0, 99, true).unwrap();
        assert_eq!(back.monitors[1].x, 100);
        // The old tolerance snapped the step straight back.
        assert_eq!(
            layout.moved("desk", 200, 0, 150, true).unwrap().monitors[1].x,
            100
        );
    }

    #[test]
    fn sidecar_is_explicit_and_round_trips_without_touching_config() {
        let directory = tempfile::tempdir().unwrap();
        let config = directory.path().join("new/zflow.toml");
        let mut document = LayoutDocument::beside(&config).unwrap();
        assert!(document.is_new());
        assert!(!document.is_dirty());
        assert!(!config.parent().unwrap().exists());
        document
            .draft
            .monitors
            .push(monitor("revoked-peer", 0, 0, 100, 100));
        assert!(document.is_dirty());
        document.save().unwrap();
        assert_eq!(document.path.file_name().unwrap(), "zflow.toml.layout.toml");
        assert!(!config.exists());
        assert!(!document.is_new());
        assert!(!document.is_dirty());
        assert_eq!(
            LayoutDocument::beside(&config).unwrap().draft,
            document.draft
        );
        document.draft.monitors.clear();
        document.reload().unwrap();
        assert_eq!(document.draft.monitors.len(), 1);
    }

    #[test]
    fn external_create_edit_delete_and_malformed_reload_preserve_draft() {
        let directory = tempfile::tempdir().unwrap();
        let config = directory.path().join("zflow.toml");
        let mut document = LayoutDocument::beside(&config).unwrap();
        fs::write(&document.path, "monitors = []\n").unwrap();
        assert!(document.save().is_err());
        document.reload().unwrap();
        document
            .draft
            .monitors
            .push(monitor("local", 0, 0, 100, 100));
        fs::write(&document.path, "monitors = [unfinished").unwrap();
        assert!(LayoutDocument::beside(&config).is_err());
        assert!(document.reload().is_err());
        assert!(document.save().is_err());
        assert_eq!(document.draft.monitors.len(), 1);
        fs::remove_file(&document.path).unwrap();
        assert!(document.save().is_err());
        assert!(!document.path.exists());
    }

    #[test]
    fn invalid_save_preserves_existing_contents() {
        let directory = tempfile::tempdir().unwrap();
        let mut document = LayoutDocument::beside(&directory.path().join("zflow.toml")).unwrap();
        document.save().unwrap();
        let before = fs::read(&document.path).unwrap();
        document.draft.monitors.push(monitor("local", 0, 0, 0, 100));
        assert!(document.save().is_err());
        assert_eq!(fs::read(&document.path).unwrap(), before);
    }
}
