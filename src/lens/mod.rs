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

fn spread(samples: &mut [u64], unit: u64) -> String {
    samples.sort_unstable();
    let at = |q: f64| {
        let index = ((samples.len().max(1) - 1) as f64 * q).round() as usize;
        samples.get(index).map_or(0, |sample| sample / unit)
    };
    format!("{}/{}/{}", at(0.5), at(0.95), at(1.))
}
