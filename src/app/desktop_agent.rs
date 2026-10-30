use anyhow::{Result, ensure};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

pub fn run(install: bool) -> Result<()> {
    if install {
        return install_agent();
    }
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,nid=info".into()),
        )
        .try_init();
    if let Err(error) = super::desktop::refresh_extension() {
        tracing::warn!(%error, "could not refresh the bundled GNOME extension");
    }
    let updated = tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
        let state = Arc::new(Mutex::new(super::gnome::State::default()));
        let connection = super::gnome::connect(state.clone()).await?;
        tokio::spawn(super::updates::remind(connection.clone()));
        tokio::spawn(offer_lenses(connection.clone()));
        let mut timer=tokio::time::interval(Duration::from_secs(2));
        let mut terminate=tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        let mut interrupt=tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
        let mut previous=String::new();
        let mut updated=None;
        loop {
            tokio::select! {
                _=terminate.recv() => break,
                _=interrupt.recv() => break,
                _=timer.tick() => {
                    if let Some(path) = std::fs::read_link("/proc/self/exe").ok().and_then(|exe| replaced(&exe)) {
                        tracing::info!("Nidus was updated; restarting the desktop agent");
                        updated=Some(path);
                        break;
                    }
                    let status = {
                        let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
                        if !state.receiver.is_active() { state.receiver.start(&connection); }
                        state.receiver.status()
                    };
                    if status!=previous { tracing::info!(%status,"desktop agent"); previous=status; }
                }
            }
        }
        state.lock().unwrap_or_else(|e| e.into_inner()).receiver.stop();
        anyhow::Ok(updated)
    })?;
    match updated {
        Some(path) => {
            use std::os::unix::process::CommandExt;
            Err(std::process::Command::new(path)
                .args(std::env::args_os().skip(1))
                .exec()
                .into())
        }
        None => Ok(()),
    }
}

async fn offer_lenses(bus: zbus::Connection) {
    loop {
        if let Err(error) = offer_lens(&bus).await {
            tracing::debug!(error = %format_args!("{error:#}"), "lens offer ended");
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }
}

async fn offer_lens(bus: &zbus::Connection) -> Result<()> {
    let mut stream = tokio::net::UnixStream::connect(crate::peer_view::SOCKET_PATH).await?;
    crate::control::write_message(&mut stream, &crate::peer_view::Request::Lens {}).await?;
    let opened: crate::peer_view::LensOpened = crate::control::read_message(&mut stream).await?;
    tracing::info!(peer = %opened.peer, "showing this screen through a lens");
    let result = crate::lens::host::serve(stream, opened, bus).await;
    tracing::info!("lens closed");
    result
}

fn replaced(exe: &std::path::Path) -> Option<std::path::PathBuf> {
    let path = std::path::Path::new(exe.to_str()?.strip_suffix(" (deleted)")?);
    path.is_file().then(|| path.to_owned())
}

fn install_agent() -> Result<()> {
    ensure!(
        !nix::unistd::Uid::effective().is_root(),
        "Run desktop-agent --install as your desktop user, without sudo"
    );
    super::gnome::install()?;
    println!(
        "Adding the Nidus GNOME extension. GNOME may ask to download it from extensions.gnome.org."
    );
    let running = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let session = zbus::Connection::session().await.ok();
            let running = super::desktop::install_extension(session.as_ref()).await?;
            if let Some(connection) = &session {
                super::gnome::start_agent(connection, true).await?;
            }
            Ok::<_, anyhow::Error>(running)
        })?;
    if running {
        println!("GNOME integration is ready. Open Nidus from Applications.");
    } else {
        println!("Log out and back in to finish setting up Nidus in GNOME.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_update_restarts_the_agent_from_the_new_binary() {
        let directory = tempfile::tempdir().unwrap();
        let binary = directory.path().join("nid");
        let deleted = |path: &std::path::Path| format!("{} (deleted)", path.display());
        assert_eq!(
            replaced(deleted(&binary).as_ref()),
            None,
            "removed, not replaced"
        );
        std::fs::write(&binary, b"new").unwrap();
        assert_eq!(replaced(&binary), None, "still running from it");
        assert_eq!(replaced(deleted(&binary).as_ref()), Some(binary));
    }
}
