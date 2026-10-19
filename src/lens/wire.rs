//! What travels on a lens stream. Both directions carry `Lens` wire frames, each behind a four
//! byte length, and a [`LensMessage::Frame`] is followed by its tile packet the same way. The
//! lens has a stream of its own, below the input control stream's priority, so pixels never
//! hold up input.

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::tiles;
use crate::{
    core::HidUsage,
    wire::{self, WireMessage},
};

/// Opens every lens stream, so a stray stream is refused at once.
pub const PREFACE: &[u8; 11] = b"zflow-lens\0";
/// A whole desktop of exact tiles plus their record headers, the most one packet can hold.
pub const MAX_BODY_BYTES: usize = tiles::MAX_PIXELS * 4 + 64 * 1_024;
/// Selections longer than a clip are not copied.
pub const MAX_TEXT_BYTES: usize = crate::clipboard::MAX_CLIP_BYTES;
pub const MAX_REASON_BYTES: usize = 256;
pub const MAX_RADIUS: u16 = 2_048;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LensMessage {
    /// Viewer: the lens and the pointer are centred here, in the viewed computer's capture
    /// pixels. `id` grows with every move, so a frame can say which move it answers.
    Move {
        x: u16,
        y: u16,
        radius: u16,
        id: u64,
    },
    /// Viewer: every frame up to `serial` has been decoded.
    Ack {
        serial: u64,
    },
    /// Viewer: a [`crate::core::PointerButton`]: 1 primary, 2 secondary, 3 middle, 4 back,
    /// 5 forward.
    Button {
        button: u8,
        pressed: bool,
    },
    Key {
        usage: HidUsage,
        pressed: bool,
    },
    /// Viewer: whole pixels; positive `dy` scrolls content up.
    Scroll {
        dx: i32,
        dy: i32,
    },
    /// Viewer: copy the selection and send it back.
    Copy,
    /// Viewed: the recorded monitor's size. Tiles follow.
    Ready {
        width: u16,
        height: u16,
    },
    /// Viewed: a tile packet follows. `request` is the newest move it answers; `exact` says
    /// whether the lens is free of previews once it lands.
    Frame {
        serial: u64,
        request: u64,
        exact: bool,
    },
    Copied {
        text: String,
    },
    /// Viewed: why the lens or a copy cannot go on.
    Refused {
        reason: String,
    },
}

impl LensMessage {
    /// Limits checked before encoding and after decoding.
    pub(crate) fn validate(&self) -> Result<(), String> {
        let check = |ok: bool, what: &str| if ok { Ok(()) } else { Err(what.to_owned()) };
        match self {
            Self::Move { radius, .. } => check(
                (1..=MAX_RADIUS).contains(radius),
                "lens radius out of range",
            ),
            Self::Button { button, .. } => check((1..=5).contains(button), "unknown button"),
            Self::Ready { width, height } => check(
                tiles::Decoder::new(*width as usize, *height as usize).is_ok(),
                "unsupported desktop size",
            ),
            Self::Copied { text } => check(text.len() <= MAX_TEXT_BYTES, "copied text too long"),
            Self::Refused { reason } => check(reason.len() <= MAX_REASON_BYTES, "reason too long"),
            _ => Ok(()),
        }
    }
}

/// Writes one message, and the tile packet that goes with a frame.
pub async fn write(
    stream: &mut (impl AsyncWrite + Unpin),
    message: LensMessage,
    body: Option<&[u8]>,
) -> Result<()> {
    let frame = matches!(message, LensMessage::Frame { .. });
    ensure!(
        frame == body.is_some(),
        "a tile packet goes with a frame and nothing else"
    );
    let bytes = wire::encode(&WireMessage::Lens(message))?;
    let mut packet = Vec::with_capacity(8 + bytes.len() + body.map_or(0, <[u8]>::len));
    packet.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    packet.extend_from_slice(&bytes);
    if let Some(body) = body {
        ensure!(body.len() <= MAX_BODY_BYTES, "tile packet too large");
        packet.extend_from_slice(&(body.len() as u32).to_be_bytes());
        packet.extend_from_slice(body);
    }
    stream.write_all(&packet).await?;
    Ok(())
}

/// Reads one message, and its tile packet if it is a frame. `None` when the stream ends cleanly
/// between messages.
pub async fn read(
    stream: &mut (impl AsyncRead + Unpin),
) -> Result<Option<(LensMessage, Option<Vec<u8>>)>> {
    let mut length = [0; 4];
    match stream.read_exact(&mut length).await {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    let bytes = read_exact(
        stream,
        u32::from_be_bytes(length) as usize,
        MAX_MESSAGE_BYTES,
    )
    .await?;
    let WireMessage::Lens(message) = wire::decode_family(&bytes, wire::Family::Lens)? else {
        unreachable!("decode_family returns the family it was asked for");
    };
    let body = if matches!(message, LensMessage::Frame { .. }) {
        stream
            .read_exact(&mut length)
            .await
            .context("tile packet")?;
        Some(read_exact(stream, u32::from_be_bytes(length) as usize, MAX_BODY_BYTES).await?)
    } else {
        None
    };
    Ok(Some((message, body)))
}

const MAX_MESSAGE_BYTES: usize = wire::MAX_LENS_PAYLOAD_BYTES + 3;

async fn read_exact(
    stream: &mut (impl AsyncRead + Unpin),
    length: usize,
    maximum: usize,
) -> Result<Vec<u8>> {
    ensure!(
        length <= maximum,
        "lens message of {length} bytes exceeds {maximum}"
    );
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes).await?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn messages_and_tile_packets_survive_the_trip() -> Result<()> {
        let (mut near, mut far) = tokio::io::duplex(1 << 16);
        let frame = LensMessage::Frame {
            serial: 7,
            request: 3,
            exact: false,
        };
        write(&mut near, LensMessage::Copy, None).await?;
        write(&mut near, frame.clone(), Some(&[1, 2, 3])).await?;
        write(
            &mut near,
            LensMessage::Key {
                usage: HidUsage::keyboard(0x06),
                pressed: true,
            },
            None,
        )
        .await?;
        drop(near);
        assert_eq!(read(&mut far).await?, Some((LensMessage::Copy, None)));
        assert_eq!(read(&mut far).await?, Some((frame, Some(vec![1, 2, 3]))));
        assert!(matches!(
            read(&mut far).await?,
            Some((LensMessage::Key { pressed: true, .. }, None))
        ));
        assert_eq!(read(&mut far).await?, None);
        Ok(())
    }

    #[tokio::test]
    async fn out_of_range_messages_are_refused_on_both_ends() {
        let (mut near, _far) = tokio::io::duplex(1 << 16);
        let wide = LensMessage::Move {
            x: 1,
            y: 1,
            radius: 0,
            id: 1,
        };
        assert!(write(&mut near, wide, None).await.is_err());
        assert!(
            write(&mut near, LensMessage::Copy, Some(&[1]))
                .await
                .is_err()
        );
        let huge = 9_000_000_u32.to_be_bytes();
        let (mut near, mut far) = tokio::io::duplex(64);
        near.write_all(&huge).await.unwrap();
        assert!(read(&mut far).await.is_err());
    }
}
