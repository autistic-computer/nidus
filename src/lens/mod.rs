//! The lens: a circle around this computer's pointer that shows another computer's screen live.
//! The computer being viewed streams lossless 64x64 tiles, the lens first, then a ring around it,
//! then the rest of its desktop, paced by acknowledgements so a reply to the pointer never waits
//! behind a queue. Short budgets send downscaled previews that are refined to exact pixels.

#[cfg(target_os = "linux")]
pub mod capture;
pub mod client;
#[cfg(target_os = "linux")]
pub mod host;
#[cfg(target_os = "linux")]
pub mod mutter;
pub mod pace;
pub mod tiles;
pub mod viewer;
pub mod wire;

/// The median, 95th percentile and largest of `samples` divided by `unit`, for a log line.
fn spread(samples: &mut [u64], unit: u64) -> String {
    samples.sort_unstable();
    let at = |q: f64| {
        let index = ((samples.len().max(1) - 1) as f64 * q).round() as usize;
        samples.get(index).map_or(0, |sample| sample / unit)
    };
    format!("{}/{}/{}", at(0.5), at(0.95), at(1.))
}
