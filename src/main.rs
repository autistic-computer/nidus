use clap::Parser;

fn main() -> anyhow::Result<()> {
    nid::cli::run(nid::cli::Cli::parse())
}
