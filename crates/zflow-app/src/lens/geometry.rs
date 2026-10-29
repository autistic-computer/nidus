#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fit {
    pub scale: f64,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub desktop: (u32, u32),
}

impl Fit {
    pub fn new(desktop: (u32, u32), screen: (f64, f64)) -> Option<Self> {
        let (width, height) = (f64::from(desktop.0), f64::from(desktop.1));
        if width <= 0.0 || height <= 0.0 || screen.0 <= 0.0 || screen.1 <= 0.0 {
            return None;
        }
        let scale = (screen.0 / width).min(screen.1 / height);
        let (shown_width, shown_height) = (width * scale, height * scale);
        Some(Self {
            scale,
            x: (screen.0 - shown_width) / 2.0,
            y: (screen.1 - shown_height) / 2.0,
            width: shown_width,
            height: shown_height,
            desktop,
        })
    }

    pub fn pixel(&self, x: f64, y: f64) -> (f64, f64) {
        let column = ((x - self.x) / self.scale).clamp(0.0, f64::from(self.desktop.0) - 1.0);
        let row = ((y - self.y) / self.scale).clamp(0.0, f64::from(self.desktop.1) - 1.0);
        (column, row)
    }

    #[cfg(test)]
    pub fn point(&self, column: f64, row: f64) -> (f64, f64) {
        (self.x + column * self.scale, self.y + row * self.scale)
    }

    pub fn pixels(&self, points: f64) -> f64 {
        points / self.scale
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grid {
    pub chunk: u32,
    pub columns: u32,
    pub rows: u32,
    pub desktop: (u32, u32),
}

impl Grid {
    pub fn new(desktop: (u32, u32), chunk: u32) -> Self {
        Self {
            chunk,
            columns: desktop.0.div_ceil(chunk),
            rows: desktop.1.div_ceil(chunk),
            desktop,
        }
    }

    pub fn rect(&self, id: u32) -> (u32, u32, u32, u32) {
        let (x, y) = (
            (id % self.columns) * self.chunk,
            (id / self.columns) * self.chunk,
        );
        (
            x,
            y,
            self.chunk.min(self.desktop.0.saturating_sub(x)),
            self.chunk.min(self.desktop.1.saturating_sub(y)),
        )
    }

    #[cfg(test)]
    pub fn around(&self, x: f64, y: f64, radius: f64) -> Vec<u32> {
        let span = |center: f64, size: u32| {
            let low = (center - radius).max(0.0) as u32 / self.chunk;
            let high = ((center + radius).max(0.0) as u32).min(size.saturating_sub(1)) / self.chunk;
            low..=high
        };
        let mut ids = Vec::new();
        for row in span(y, self.desktop.1) {
            for column in span(x, self.desktop.0) {
                ids.push(row * self.columns + column);
            }
        }
        ids
    }
}

pub fn button(number: u8) -> Option<u8> {
    match number {
        0 => Some(1),
        1 => Some(3),
        2 => Some(2),
        3 => Some(4),
        4 => Some(5),
        _ => None,
    }
}

pub fn scroll(dx: f64, dy: f64, precise: bool) -> (f64, f64) {
    let unit = if precise { 1.0 } else { 16.0 };
    (-dx * unit, -dy * unit)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyAction {
    Close,
    Copy,
    Send,
}

pub fn key_down(code: u16, command_only: bool, character: Option<&str>) -> KeyAction {
    const ESCAPE: u16 = 53;
    if code == ESCAPE {
        KeyAction::Close
    } else if command_only && character.is_some_and(|c| c.eq_ignore_ascii_case("c")) {
        KeyAction::Copy
    } else {
        KeyAction::Send
    }
}

pub fn modifier_down(code: u16, flags: u64) -> Option<bool> {
    const COMMAND: u64 = 1 << 20;
    const SHIFT: u64 = 1 << 17;
    const OPTION: u64 = 1 << 19;
    const CONTROL: u64 = 1 << 18;
    const FUNCTION: u64 = 1 << 23;
    let (side, both, family) = match code {
        55 => (0x08, 0x18, COMMAND),
        54 => (0x10, 0x18, COMMAND),
        56 => (0x02, 0x06, SHIFT),
        60 => (0x04, 0x06, SHIFT),
        58 => (0x20, 0x60, OPTION),
        61 => (0x40, 0x60, OPTION),
        59 => (0x01, 0x2001, CONTROL),
        62 => (0x2000, 0x2001, CONTROL),
        63 => return Some(flags & FUNCTION != 0),
        _ => return None,
    };
    Some(if flags & both == 0 {
        flags & family != 0
    } else {
        flags & side != 0
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_desktop_fits_with_bars() {
        let fit = Fit::new((2000, 1000), (1000.0, 1000.0)).unwrap();
        assert!((fit.scale - 0.5).abs() < f64::EPSILON);
        assert!((fit.y - 250.0).abs() < f64::EPSILON && fit.x.abs() < f64::EPSILON);
        assert_eq!(fit.pixel(500.0, 500.0), (1000.0, 500.0));
        assert_eq!(fit.point(1000.0, 500.0), (500.0, 500.0));
        assert_eq!(fit.pixel(500.0, 10.0), (1000.0, 0.0));
        assert_eq!(fit.pixel(5000.0, 5000.0), (1999.0, 999.0));
        assert_eq!(Fit::new((0, 10), (10.0, 10.0)), None);
    }

    #[test]
    fn the_grid_matches_the_engines_chunks() {
        let grid = Grid::new((600, 300), 256);
        assert_eq!((grid.columns, grid.rows), (3, 2));
        assert_eq!(grid.rect(2), (512, 0, 88, 256));
        assert_eq!(grid.rect(5), (512, 256, 88, 44));
        assert_eq!(grid.around(0.0, 0.0, 10.0), vec![0]);
        assert_eq!(grid.around(300.0, 200.0, 100.0), vec![0, 1, 3, 4]);
    }

    #[test]
    fn buttons_scrolls_and_keys_map_as_the_engine_wants() {
        assert_eq!(button(0), Some(1));
        assert_eq!(button(1), Some(3));
        assert_eq!(button(2), Some(2));
        assert_eq!(button(9), None);
        assert_eq!(scroll(1.0, -2.0, true), (-1.0, 2.0));
        assert_eq!(scroll(0.0, 1.0, false), (-0.0, -16.0));
        assert_eq!(key_down(53, false, None), KeyAction::Close);
        assert_eq!(key_down(8, true, Some("c")), KeyAction::Copy);
        assert_eq!(key_down(8, false, Some("c")), KeyAction::Send);
        assert_eq!(modifier_down(55, (1 << 20) | 0x08), Some(true));
        assert_eq!(modifier_down(54, (1 << 20) | 0x08), Some(false));
        assert_eq!(modifier_down(59, 1 << 18), Some(true));
        assert_eq!(modifier_down(57, 0), None);
    }
}
