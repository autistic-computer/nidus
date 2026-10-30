#[cfg(target_os = "linux")]
use clap::Parser;

#[cfg(target_os = "linux")]
#[derive(Debug, Parser)]
#[command(name = "nidd", version, about = "Nidus privileged input daemon")]
struct Args {
    #[arg(long, default_value = "/etc/nid/nid.toml")]
    config: std::path::PathBuf,
}

#[cfg(target_os = "linux")]
fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    nid::daemon::run(args.config)
}

#[cfg(not(target_os = "linux"))]
fn main() -> anyhow::Result<()> {
    anyhow::bail!("nidd is currently supported only on Linux")
}
