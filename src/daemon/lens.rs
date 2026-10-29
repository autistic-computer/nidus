use super::*;
use crate::{
    lens::wire::{LensMessage, write},
    transport::LensStream,
};
use tokio::net::UnixStream;

const RELAY_BUFFER: usize = 256 * 1024;

impl Shared {
    pub(super) async fn offer_lens(self: &Arc<Self>, agent: UnixStream) {
        *self.lens_host.lock().await = Some(agent);
    }

    pub(super) fn serve_lens(self: &Arc<Self>, peer: String, stream: LensStream) {
        let shared = self.clone();
        tokio::spawn(async move {
            let LensStream { mut send, receive } = stream;
            let refused = match shared.admit_lens(&peer).await {
                Ok((mut agent, opened)) => {
                    tracing::info!(%peer, "lens opened");
                    let relayed = async {
                        crate::control::write_message(&mut agent, &opened).await?;
                        let mut remote = tokio::io::join(receive, send);
                        tokio::io::copy_bidirectional_with_sizes(
                            &mut agent,
                            &mut remote,
                            RELAY_BUFFER,
                            RELAY_BUFFER,
                        )
                        .await?;
                        anyhow::Ok(())
                    }
                    .await;
                    match relayed {
                        Ok(()) => tracing::info!(%peer, "lens closed"),
                        Err(error) => {
                            tracing::info!(%peer, error = %format_args!("{error:#}"), "lens ended");
                        }
                    }
                    return;
                }
                Err(reason) => reason,
            };
            tracing::info!(%peer, %refused, "lens refused");
            let refusal = LensMessage::Refused { reason: refused };
            if write(&mut send, refusal, None).await.is_ok() {
                let _ = send.finish();
                let _ = send.stopped().await;
            }
        });
    }

    async fn admit_lens(
        &self,
        peer: &str,
    ) -> Result<(UnixStream, crate::peer_view::LensOpened), String> {
        let keyboard = {
            let config = self.config.read().await;
            let record = config
                .peers
                .get(peer)
                .ok_or_else(|| "not paired".to_owned())?;
            if !config.daemon.sharing {
                return Err("sharing is paused here".into());
            }
            if !record.share_screen {
                return Err(format!(
                    "this computer does not share its screen with {peer}"
                ));
            }
            record.keyboard
        };
        let agent = self
            .lens_host
            .lock()
            .await
            .take()
            .ok_or_else(|| "the desktop is not ready to be viewed".to_owned())?;
        Ok((
            agent,
            crate::peer_view::LensOpened {
                peer: peer.to_owned(),
                keyboard,
            },
        ))
    }
}
