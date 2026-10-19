//! Tiles that build to lossless. B tracks what A holds for every 64x64 tile and spends a byte
//! budget on the lens first, then a ring around it, then the rest of the desktop. When the budget
//! cannot cover the lens exactly, its tiles go out as downscaled previews, refined centre-out.

use anyhow::{Result, bail, ensure};

pub const TILE: usize = 64;
pub const MAX_PIXELS: usize = 4096 * 2160;
const HEADER: usize = 24;
/// Band around the lens that is fetched ahead of the pointer, in desktop pixels.
const RING: f64 = 256.;
/// Background tiles compared per round, so a changed desktop never delays the lens for long.
const SCAN: usize = 256;
/// Changing content far from the lens is refreshed at most this often.
const REFRESH_NS: u64 = 1_000_000_000;
/// Changing content in the ring is refreshed at most this often.
const RING_REFRESH_NS: u64 = 250_000_000;
/// Waiting this long counts like being one lens radius closer to the centre, so an animation
/// under the pointer cannot starve the edge of the lens.
const PATIENCE_NS: f64 = 100_000_000.;

/// A damaged region of a capture, in desktop pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl Rect {
    fn overlaps(self, (x, y, width, height): (usize, usize, usize, usize)) -> bool {
        self.x < x + width
            && x < self.x + self.width
            && self.y < y + height
            && y < self.y + self.height
    }
}

#[derive(Clone, Copy, Debug)]
pub struct View {
    pub x: f64,
    pub y: f64,
    pub radius: f64,
}

impl View {
    pub fn includes(self, x: usize, y: usize, width: usize, height: usize) -> bool {
        let dx = self.x - self.x.clamp(x as f64, (x + width) as f64);
        let dy = self.y - self.y.clamp(y as f64, (y + height) as f64);
        dx * dx + dy * dy <= self.radius * self.radius
    }
    fn distance(self, x: usize, y: usize, width: usize, height: usize) -> f64 {
        (x as f64 + width as f64 / 2. - self.x).hypot(y as f64 + height as f64 / 2. - self.y)
    }
}

/// Bytes the next packet may use: the lens may fill the window, the fill only its spare half.
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    pub lens: usize,
    pub fill: usize,
}

pub struct Packet {
    pub body: Option<Vec<u8>>,
    /// Every tile under the lens is exact on A once this packet lands.
    pub exact: bool,
    /// The background scan stopped early; call again before waiting.
    pub more: bool,
}

#[derive(Clone, Copy, Default)]
struct Tile {
    /// 0: A has nothing. 1: exact. 2 or 4: a preview downscaled by that factor.
    level: u8,
    stale: bool,
    checked: u64,
    compared_ns: u64,
    picked: u64,
    sent_ns: u64,
}

#[derive(Clone, Copy, PartialEq)]
enum Need {
    Missing,
    Stale,
    Refine,
    Current,
}

pub struct Encoder {
    width: usize,
    height: usize,
    /// Source pixels behind what A shows, so a preview can be refined without a new capture.
    pixels: Vec<u8>,
    tiles: Vec<Tile>,
    generation: u64,
    round: u64,
    cursor: usize,
    clean: usize,
    /// When the earliest far tile that changed may be refreshed.
    deferred: Option<u64>,
    sequence: u64,
    ratio: f64,
    raw: Vec<u8>,
    compressor: zstd::bulk::Compressor<'static>,
}

pub struct Decoder {
    width: usize,
    height: usize,
    pub pixels: Vec<u8>,
    pub valid: Vec<bool>,
    sequence: u64,
    decompressor: zstd::bulk::Decompressor<'static>,
}

fn dimensions(width: usize, height: usize) -> Result<usize> {
    ensure!(
        width > 0 && height > 0 && width <= 4096 && height <= 2160,
        "unsupported desktop dimensions {width}x{height}"
    );
    Ok(width.div_ceil(TILE) * height.div_ceil(TILE))
}

fn rect(width: usize, height: usize, id: usize) -> (usize, usize, usize, usize) {
    let columns = width.div_ceil(TILE);
    let (x, y) = ((id % columns) * TILE, (id / columns) * TILE);
    (x, y, TILE.min(width - x), TILE.min(height - y))
}

fn record_size(width: usize, height: usize, level: usize) -> usize {
    width.div_ceil(level) * height.div_ceil(level) * 4
}

impl Encoder {
    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    pub fn new(width: usize, height: usize) -> Result<Self> {
        let count = dimensions(width, height)?;
        let mut compressor = zstd::bulk::Compressor::new(1)?;
        compressor.set_parameter(zstd::zstd_safe::CParameter::ChecksumFlag(true))?;
        Ok(Self {
            width,
            height,
            pixels: vec![0; width * height * 4],
            tiles: vec![Tile::default(); count],
            generation: 1,
            round: 0,
            cursor: 0,
            clean: 0,
            deferred: None,
            sequence: 0,
            ratio: 0.35,
            raw: Vec::new(),
            compressor,
        })
    }

    /// A new capture arrived. Tiles inside `damage` must be compared again before they count as
    /// current; without damage, every tile must. Mutter reports up to 32 rectangles per frame.
    pub fn changed(&mut self, damage: Option<&[Rect]>) {
        self.generation += 1;
        self.clean = 0;
        let Some(damage) = damage else { return };
        for id in 0..self.tiles.len() {
            let tile = rect(self.width, self.height, id);
            let touched = damage.iter().any(|d| d.overlaps(tile));
            // Only a tile known current at the previous capture stays current through this one.
            if !touched && self.tiles[id].checked + 1 == self.generation {
                self.tiles[id].checked = self.generation;
            }
        }
    }

    /// Compare a tile with the capture, unless it was compared less than `every` ago.
    fn need(&mut self, id: usize, pixels: &[u8], stride: usize, now: u64, every: u64) -> Need {
        let (x, y, w, h) = rect(self.width, self.height, id);
        if self.tiles[id].level == 0 {
            return Need::Missing;
        }
        if self.tiles[id].checked != self.generation && now >= self.tiles[id].compared_ns + every {
            let stale = (y..y + h).any(|row| {
                let a = row * stride + x * 4;
                let b = (row * self.width + x) * 4;
                pixels[a..a + w * 4] != self.pixels[b..b + w * 4]
            });
            self.tiles[id].checked = self.generation;
            self.tiles[id].compared_ns = now;
            self.tiles[id].stale = stale;
        }
        match self.tiles[id] {
            Tile { stale: true, .. } => Need::Stale,
            Tile { level: 2.., .. } => Need::Refine,
            _ => Need::Current,
        }
    }

    /// Estimated compressed bytes for one tile at a level.
    fn cost(&self, id: usize, level: usize) -> usize {
        let (_, _, w, h) = rect(self.width, self.height, id);
        (record_size(w, h, level) as f64 * self.ratio) as usize + 5
    }

    /// Input remains borrowed. Unchanged rows are compared without materializing tiles.
    pub fn encode(
        &mut self,
        pixels: &[u8],
        stride: usize,
        view: Option<View>,
        budget: Budget,
        now: u64,
    ) -> Result<Packet> {
        // The last row needs no padding after it, as in a buffer read in place.
        ensure!(
            stride >= self.width * 4
                && stride
                    .checked_mul(self.height - 1)
                    .and_then(|n| n.checked_add(self.width * 4))
                    .is_some_and(|n| pixels.len() >= n),
            "short input frame"
        );
        if let Some(v) = view {
            ensure!(
                v.x.is_finite() && v.y.is_finite() && v.radius.is_finite() && v.radius > 0.,
                "invalid view"
            );
        }
        self.round += 1;
        if self.deferred.is_some_and(|due| now >= due) {
            self.deferred = None;
            self.clean = 0;
        }
        let count = self.tiles.len();
        let near = |encoder: &Self, view: View| {
            let mut ids: Vec<(f64, usize)> = (0..count)
                .filter_map(|id| {
                    let (x, y, w, h) = rect(encoder.width, encoder.height, id);
                    view.includes(x, y, w, h)
                        .then(|| (view.distance(x, y, w, h), id))
                })
                .collect();
            ids.sort_by(|a, b| a.0.total_cmp(&b.0));
            ids
        };
        let lens = match view {
            Some(view) => near(self, view),
            None => (0..count).map(|id| (0., id)).collect(),
        };
        // Holes under the lens are always filled, as previews when the budget is short. Stale
        // and preview tiles wait for budget, since A already shows something there.
        let mut work = Vec::new();
        for &(distance, id) in &lens {
            match self.need(id, pixels, stride, now, 0) {
                Need::Current => {}
                need => {
                    let waited = now.saturating_sub(self.tiles[id].sent_ns) as f64;
                    let radius = view.map_or(1., |v| v.radius);
                    work.push((distance / radius - waited / PATIENCE_NS, id, need));
                }
            }
        }
        work.sort_by(|a, b| a.0.total_cmp(&b.0));
        let work: Vec<_> = work.into_iter().map(|(_, id, need)| (id, need)).collect();
        self.raw.clear();
        let mut records = 0u32;
        let mut chosen = Vec::with_capacity(work.len());
        let mut left = budget.lens;
        let exact_cost: usize = work.iter().map(|&(id, _)| self.cost(id, 1)).sum();
        let mut exact = true;
        if exact_cost <= left {
            chosen.extend(work.iter().map(|&(id, _)| (id, 1)));
            left -= exact_cost;
        } else {
            let preview = |level, of: Need| {
                work.iter()
                    .filter(|w| w.1 == of)
                    .map(|&(id, _)| self.cost(id, level))
                    .sum::<usize>()
            };
            let level = if preview(2, Need::Missing) + preview(2, Need::Stale) <= left {
                2
            } else {
                4
            };
            left = left.saturating_sub(preview(level, Need::Missing));
            let stale = preview(level, Need::Stale) <= left;
            if stale {
                left -= preview(level, Need::Stale);
            }
            for &(id, need) in &work {
                let previewed = need == Need::Missing || (stale && need == Need::Stale);
                let extra = self.cost(id, 1) - if previewed { self.cost(id, level) } else { 0 };
                if extra <= left {
                    chosen.push((id, 1));
                    left -= extra;
                } else {
                    exact = false;
                    if previewed {
                        chosen.push((id, level));
                    }
                }
            }
        }
        for (id, level) in chosen {
            self.put(id, level, pixels, stride, now);
            records += 1;
        }
        // The ring and the rest of the desktop only get exact tiles, from spare budget.
        let mut more = false;
        if let Some(view) = view {
            let mut fill = budget.fill.min(left);
            let ring = View {
                radius: view.radius + RING,
                ..view
            };
            for (_, id) in near(self, ring) {
                if fill == 0 {
                    break;
                }
                if self.tiles[id].picked != self.round
                    && self.need(id, pixels, stride, now, RING_REFRESH_NS) != Need::Current
                    && self.cost(id, 1) <= fill
                {
                    fill -= self.cost(id, 1);
                    self.put(id, 1, pixels, stride, now);
                    records += 1;
                }
            }
            let mut scanned = 0;
            while self.clean < count && scanned < SCAN {
                let id = self.cursor;
                if self.tiles[id].picked == self.round {
                    // Picked by the lens or ring; a preview still needs another visit.
                    if self.tiles[id].level > 1 {
                        self.clean = 0;
                    } else {
                        self.clean += 1;
                    }
                } else {
                    let need = self.need(id, pixels, stride, now, REFRESH_NS);
                    if self.tiles[id].checked != self.generation {
                        // Compared too recently to look again; come back when it is due.
                        let due = self.tiles[id].compared_ns + REFRESH_NS;
                        self.deferred = Some(self.deferred.map_or(due, |d| d.min(due)));
                    }
                    if need != Need::Current {
                        if self.cost(id, 1) > fill {
                            break;
                        }
                        fill -= self.cost(id, 1);
                        self.put(id, 1, pixels, stride, now);
                        records += 1;
                    }
                    self.clean += 1;
                }
                self.cursor = (id + 1) % count;
                scanned += 1;
            }
            more = scanned == SCAN && self.clean < count;
        }
        if records == 0 {
            return Ok(Packet {
                body: None,
                exact,
                more,
            });
        }
        self.sequence += 1;
        let compressed = self.compressor.compress(&self.raw)?;
        self.ratio = 0.8 * self.ratio + 0.2 * (compressed.len() as f64 / self.raw.len() as f64);
        let mut packet = Vec::with_capacity(HEADER + compressed.len());
        packet.extend_from_slice(b"SPT4");
        packet.extend_from_slice(&self.sequence.to_be_bytes());
        packet.extend_from_slice(&(self.width as u16).to_be_bytes());
        packet.extend_from_slice(&(self.height as u16).to_be_bytes());
        packet.extend_from_slice(&records.to_be_bytes());
        packet.extend_from_slice(&(self.raw.len() as u32).to_be_bytes());
        packet.extend_from_slice(&compressed);
        Ok(Packet {
            body: Some(packet),
            exact,
            more,
        })
    }

    /// Append one record and remember the source pixels A will be showing.
    fn put(&mut self, id: usize, level: usize, pixels: &[u8], stride: usize, now: u64) {
        let (x, y, w, h) = rect(self.width, self.height, id);
        for row in y..y + h {
            let a = row * stride + x * 4;
            let b = (row * self.width + x) * 4;
            self.pixels[b..b + w * 4].copy_from_slice(&pixels[a..a + w * 4]);
        }
        self.raw.extend_from_slice(&(id as u32).to_be_bytes());
        self.raw.push(level as u8);
        if level == 1 {
            for row in y..y + h {
                let b = (row * self.width + x) * 4;
                self.raw.extend_from_slice(&self.pixels[b..b + w * 4]);
            }
        }
        for by in (0..h).step_by(level).filter(|_| level > 1) {
            for bx in (0..w).step_by(level) {
                // Box-filter one block into a single pixel.
                let mut sum = [0u32; 4];
                let mut n = 0;
                for row in y + by..(y + by + level).min(y + h) {
                    for column in x + bx..(x + bx + level).min(x + w) {
                        let b = (row * self.width + column) * 4;
                        for (s, v) in sum.iter_mut().zip(&self.pixels[b..b + 4]) {
                            *s += *v as u32;
                        }
                        n += 1;
                    }
                }
                self.raw.extend(sum.map(|s| ((s + n / 2) / n) as u8));
            }
        }
        self.tiles[id] = Tile {
            level: level as u8,
            stale: false,
            checked: self.generation,
            compared_ns: now,
            picked: self.round,
            sent_ns: now,
        };
    }
}

impl Decoder {
    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    pub fn covers(&self, view: View) -> bool {
        self.valid.iter().enumerate().all(|(id, valid)| {
            let (x, y, w, h) = rect(self.width, self.height, id);
            *valid || !view.includes(x, y, w, h)
        })
    }

    pub fn new(width: usize, height: usize) -> Result<Self> {
        let count = dimensions(width, height)?;
        Ok(Self {
            width,
            height,
            pixels: vec![0; width * height * 4],
            valid: vec![false; count],
            sequence: 0,
            decompressor: zstd::bulk::Decompressor::new()?,
        })
    }

    /// Parse and validate every record before publishing any changed pixels. Returns the
    /// rectangles that changed.
    pub fn apply(&mut self, packet: &[u8]) -> Result<Vec<(usize, usize, usize, usize)>> {
        ensure!(
            packet.len() > HEADER && &packet[..4] == b"SPT4",
            "invalid tile header"
        );
        let sequence = u64::from_be_bytes(packet[4..12].try_into()?);
        let width = u16::from_be_bytes(packet[12..14].try_into()?) as usize;
        let height = u16::from_be_bytes(packet[14..16].try_into()?) as usize;
        let count = u32::from_be_bytes(packet[16..20].try_into()?) as usize;
        let length = u32::from_be_bytes(packet[20..24].try_into()?) as usize;
        ensure!(sequence == self.sequence + 1, "tile sequence gap");
        ensure!(
            (width, height) == (self.width, self.height),
            "desktop size changed"
        );
        ensure!(count > 0 && count <= self.valid.len(), "invalid tile count");
        ensure!(
            length <= width * height * 4 + self.valid.len() * 5,
            "oversized tile body"
        );
        let raw = self.decompressor.decompress(&packet[HEADER..], length)?;
        ensure!(raw.len() == length, "wrong tile body length");
        let mut records = Vec::with_capacity(count);
        let mut offset = 0;
        let mut seen = vec![false; self.valid.len()];
        for _ in 0..count {
            ensure!(offset + 5 <= raw.len(), "truncated tile id");
            let id = u32::from_be_bytes(raw[offset..offset + 4].try_into()?) as usize;
            let level = raw[offset + 4] as usize;
            offset += 5;
            ensure!(
                id < self.valid.len() && !std::mem::replace(&mut seen[id], true),
                "invalid or repeated tile"
            );
            ensure!(matches!(level, 1 | 2 | 4), "invalid tile level");
            let (_, _, w, h) = rect(width, height, id);
            let size = record_size(w, h, level);
            ensure!(offset + size <= raw.len(), "truncated tile pixels");
            records.push((id, level, offset));
            offset += size;
        }
        if offset != raw.len() {
            bail!("trailing tile data");
        }
        let mut changed = Vec::with_capacity(count);
        for (id, level, start) in records {
            let (x, y, w, h) = rect(width, height, id);
            if level == 1 {
                for row in 0..h {
                    let to = ((y + row) * width + x) * 4;
                    let from = start + row * w * 4;
                    self.pixels[to..to + w * 4].copy_from_slice(&raw[from..from + w * 4]);
                }
            } else {
                self.upscale(&raw[start..], (x, y, w, h), level);
            }
            self.valid[id] = true;
            changed.push((x, y, w, h));
        }
        self.sequence = sequence;
        Ok(changed)
    }

    /// Blow a preview up into its tile by repeating pixels. The viewer shows B's desktop scaled
    /// down, so a 2x block is about one screen pixel, and this costs no more than a copy.
    fn upscale(&mut self, data: &[u8], (x, y, w, h): (usize, usize, usize, usize), level: usize) {
        let columns = w.div_ceil(level);
        for row in 0..h {
            let (source, _) = data[(row / level) * columns * 4..][..columns * 4].as_chunks::<4>();
            let to = ((y + row) * self.width + x) * 4;
            let (target, _) = self.pixels[to..to + w * 4].as_chunks_mut::<4>();
            for (column, pixel) in target.iter_mut().enumerate() {
                *pixel = source[column / level];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNLIMITED: Budget = Budget {
        lens: usize::MAX,
        fill: usize::MAX,
    };

    fn exact(
        encoder: &mut Encoder,
        pixels: &[u8],
        stride: usize,
        view: Option<View>,
    ) -> Result<Option<Vec<u8>>> {
        Ok(encoder.encode(pixels, stride, view, UNLIMITED, 0)?.body)
    }

    #[test]
    fn idle_is_silent_and_hidden_edits_refresh_on_return() -> Result<()> {
        let (w, h) = (257, 131);
        let mut src = vec![7; w * h * 4];
        let mut enc = Encoder::new(w, h)?;
        let mut dec = Decoder::new(w, h)?;
        let left = Some(View {
            x: 20.,
            y: 50.,
            radius: 30.,
        });
        let right = Some(View {
            x: 250.,
            y: 80.,
            radius: 30.,
        });
        // The first round also fills the rest of the desktop, since the budget allows it.
        dec.apply(&exact(&mut enc, &src, w * 4, left)?.unwrap())?;
        assert_eq!(dec.pixels, src);
        assert!(exact(&mut enc, &src, w * 4, left)?.is_none());
        src[(80 * w + 250) * 4] = 99;
        enc.changed(None);
        let packet = enc.encode(
            &src,
            w * 4,
            left,
            Budget {
                lens: usize::MAX,
                fill: 0,
            },
            0,
        )?;
        assert!(packet.body.is_none());
        dec.apply(&exact(&mut enc, &src, w * 4, right)?.unwrap())?;
        assert_eq!(dec.pixels[(80 * w + 250) * 4], 99);
        assert_eq!(dec.pixels, src);
        assert!(exact(&mut enc, &src, w * 4, None)?.is_none());
        Ok(())
    }

    #[test]
    fn a_short_budget_covers_the_lens_with_previews_then_builds_to_lossless() -> Result<()> {
        let (w, h) = (512, 384);
        let src: Vec<u8> = (0..w * h * 4).map(|i| ((i * 7919) % 251) as u8).collect();
        let mut enc = Encoder::new(w, h)?;
        let mut dec = Decoder::new(w, h)?;
        let view = View {
            x: 256.,
            y: 192.,
            radius: 120.,
        };
        let short = Budget {
            lens: 20_000,
            fill: 0,
        };
        let first = enc.encode(&src, w * 4, Some(view), short, 0)?;
        assert!(!first.exact);
        dec.apply(&first.body.unwrap())?;
        assert!(dec.covers(view), "previews must leave no hole in the lens");
        let mut rounds = 1;
        loop {
            let packet = enc.encode(&src, w * 4, Some(view), short, 0)?;
            if let Some(body) = packet.body {
                dec.apply(&body)?;
            }
            rounds += 1;
            if packet.exact {
                break;
            }
            assert!(rounds < 100, "refinement must finish");
        }
        for y in 0..h {
            for x in 0..w {
                if (x as f64 + 0.5 - view.x).hypot(y as f64 + 0.5 - view.y) <= view.radius {
                    let o = (y * w + x) * 4;
                    assert_eq!(&dec.pixels[o..o + 4], &src[o..o + 4], "{x},{y}");
                }
            }
        }
        Ok(())
    }

    #[test]
    fn spare_budget_fills_the_ring_before_the_rest() -> Result<()> {
        let (w, h) = (1024, 256);
        let src = vec![3u8; w * h * 4];
        let mut enc = Encoder::new(w, h)?;
        let mut dec = Decoder::new(w, h)?;
        let view = View {
            x: 32.,
            y: 32.,
            radius: 20.,
        };
        let budget = Budget {
            lens: usize::MAX,
            fill: enc.cost(0, 1) * 8,
        };
        dec.apply(
            &enc.encode(&src, w * 4, Some(view), budget, 0)?
                .body
                .unwrap(),
        )?;
        let ring = View {
            radius: view.radius + RING,
            ..view
        };
        let filled: Vec<usize> = (0..dec.valid.len()).filter(|&id| dec.valid[id]).collect();
        assert!(filled.len() > 1);
        assert!(filled.iter().all(|&id| {
            let (x, y, tw, th) = rect(w, h, id);
            ring.includes(x, y, tw, th)
        }));
        Ok(())
    }

    #[test]
    fn far_changes_wait_a_second_but_near_ones_do_not() -> Result<()> {
        let (w, h) = (1024, 256);
        let mut src = vec![5u8; w * h * 4];
        let mut enc = Encoder::new(w, h)?;
        let mut dec = Decoder::new(w, h)?;
        let view = Some(View {
            x: 32.,
            y: 32.,
            radius: 20.,
        });
        dec.apply(&enc.encode(&src, w * 4, view, UNLIMITED, 0)?.body.unwrap())?;
        let far = (200 * w + 1000) * 4;
        src[far] = 77;
        enc.changed(None);
        assert!(
            enc.encode(&src, w * 4, view, UNLIMITED, 500_000_000)?
                .body
                .is_none()
        );
        dec.apply(
            &enc.encode(&src, w * 4, view, UNLIMITED, REFRESH_NS)?
                .body
                .unwrap(),
        )?;
        assert_eq!(dec.pixels[far], 77);
        let near = (40 * w + 40) * 4;
        src[near] = 88;
        enc.changed(None);
        dec.apply(
            &enc.encode(&src, w * 4, view, UNLIMITED, REFRESH_NS + 1)?
                .body
                .unwrap(),
        )?;
        assert_eq!(dec.pixels[near], 88);
        Ok(())
    }

    #[test]
    fn damage_limits_comparison_to_what_changed() -> Result<()> {
        let (w, h) = (256, 128);
        let mut src = vec![1u8; w * h * 4];
        let mut enc = Encoder::new(w, h)?;
        let mut dec = Decoder::new(w, h)?;
        dec.apply(&exact(&mut enc, &src, w * 4, None)?.unwrap())?;
        // A change outside the reported damage is not looked for, so it is not sent.
        src[(10 * w + 10) * 4] = 9;
        src[(100 * w + 200) * 4] = 9;
        let damage = Rect {
            x: 190,
            y: 90,
            width: 20,
            height: 20,
        };
        enc.changed(Some(&[damage]));
        dec.apply(&exact(&mut enc, &src, w * 4, None)?.unwrap())?;
        assert_eq!(dec.pixels[(100 * w + 200) * 4], 9);
        assert_eq!(dec.pixels[(10 * w + 10) * 4], 1);
        // Without damage, everything is compared.
        enc.changed(None);
        dec.apply(&exact(&mut enc, &src, w * 4, None)?.unwrap())?;
        assert_eq!(dec.pixels, src);
        Ok(())
    }

    #[test]
    fn malformed_batch_cannot_partially_modify_cache() -> Result<()> {
        let mut enc = Encoder::new(128, 65)?;
        let src = vec![42; 128 * 65 * 4];
        let packet = exact(&mut enc, &src, 128 * 4, None)?.unwrap();
        let mut dec = Decoder::new(128, 65)?;
        let before = dec.pixels.clone();
        for cut in [0, 3, HEADER, packet.len() - 1] {
            assert!(dec.apply(&packet[..cut]).is_err());
            assert_eq!(dec.pixels, before);
        }
        let length = u32::from_be_bytes(packet[20..24].try_into()?) as usize;
        let mut raw = zstd::bulk::decompress(&packet[HEADER..], length)?;
        // Keep a valid compressed stream, but corrupt the final record after valid ones.
        let last_id = raw.len() - 64 * 4 - 5;
        raw[last_id..last_id + 4].copy_from_slice(&999u32.to_be_bytes());
        let mut malformed = packet[..HEADER].to_vec();
        malformed.extend(zstd::bulk::compress(&raw, 1)?);
        assert!(dec.apply(&malformed).is_err());
        assert_eq!(dec.pixels, before);
        assert_eq!(dec.sequence(), 0);
        assert!(dec.valid.iter().all(|valid| !valid));
        dec.apply(&packet)?;
        assert!(dec.apply(&packet).is_err());
        assert_eq!(dec.pixels, src);
        Ok(())
    }

    #[test]
    fn padded_input_and_edge_tiles_round_trip() -> Result<()> {
        let (w, h, stride) = (73, 67, 320);
        let mut src = vec![255u8; stride * h];
        for y in 0..h {
            for x in 0..w * 4 {
                src[y * stride + x] = ((x + y * 13) % 251) as u8;
            }
        }
        let mut enc = Encoder::new(w, h)?;
        let mut dec = Decoder::new(w, h)?;
        dec.apply(&exact(&mut enc, &src, stride, None)?.unwrap())?;
        for y in 0..h {
            assert_eq!(
                &src[y * stride..y * stride + w * 4],
                &dec.pixels[y * w * 4..(y + 1) * w * 4]
            );
        }
        enc.changed(None);
        assert!(exact(&mut enc, &src, stride, None)?.is_none());
        Ok(())
    }

    #[test]
    fn every_visible_pixel_is_exact_after_edits_jumps_and_unaligned_scrolls() -> Result<()> {
        let (width, height) = (257, 131);
        let mut pixels: Vec<u8> = (0..width * height * 4).map(|i| (i % 251) as u8).collect();
        let mut encoder = Encoder::new(width, height)?;
        let mut decoder = Decoder::new(width, height)?;
        let mut random = 0x74b821c1u32;
        let mut next = || {
            random ^= random << 13;
            random ^= random >> 17;
            random ^= random << 5;
            random as usize
        };
        for frame in 0..300 {
            if frame % 17 == 0 {
                pixels.rotate_left(13 * width * 4);
            }
            for _ in 0..40 {
                let index = next() % pixels.len();
                pixels[index] = next() as u8;
            }
            encoder.changed(None);
            let view = View {
                x: (next() % width) as f64,
                y: (next() % height) as f64,
                radius: (next() % 65 + 1) as f64,
            };
            let budget = Budget {
                lens: usize::MAX,
                fill: 0,
            };
            if let Some(packet) = encoder
                .encode(&pixels, width * 4, Some(view), budget, 0)?
                .body
            {
                decoder.apply(&packet)?;
            }
            for y in 0..height {
                for x in 0..width {
                    if (x as f64 + 0.5 - view.x).powi(2) + (y as f64 + 0.5 - view.y).powi(2)
                        <= view.radius.powi(2)
                    {
                        let offset = (y * width + x) * 4;
                        assert_eq!(
                            &decoder.pixels[offset..offset + 4],
                            &pixels[offset..offset + 4],
                            "frame {frame} at {x},{y}"
                        );
                    }
                }
            }
            assert_eq!(decoder.sequence(), encoder.sequence());
        }
        Ok(())
    }
}
