use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Instant,
};

use anyhow::{Result, bail};
use tokio::sync::mpsc;

use super::{
    spread,
    tiles::Decoder,
    viewer::{Input, Viewer},
    wire::{LensMessage, MAX_RADIUS, read, write},
};
use crate::transport::LensStream;

pub fn start(viewer: &Viewer) -> mpsc::UnboundedReceiver<Input> {
    let (input, inputs) = mpsc::unbounded_channel();
    viewer.opening(input);
    inputs
}

pub async fn run(
    open: impl Future<Output = Result<LensStream>>,
    viewer: Arc<Viewer>,
    input: mpsc::UnboundedReceiver<Input>,
    copied: impl Fn(String) + Send + 'static,
) -> Result<()> {
    let trips = Arc::<Mutex<Trips>>::default();
    let served = async { serve(open.await?, viewer.clone(), input, copied, trips.clone()).await };
    let result = served.await;
    viewer.finish(result.as_ref().err().map(|error| format!("{error:#}")));
    let trips = &mut *trips.lock().unwrap_or_else(|e| e.into_inner());
    tracing::info!(
        moves = trips.moves,
        answered_ms = spread(&mut trips.answered, 1_000_000),
        sharp_ms = spread(&mut trips.sharp, 1_000_000),
        "lens round trips (median/95th/max)"
    );
    result
}

#[derive(Default)]
struct Trips {
    moves: u64,
    waiting: VecDeque<(u64, Instant)>,
    blurry: VecDeque<(u64, Instant)>,
    answered: Vec<u64>,
    sharp: Vec<u64>,
}

const TRIPS_WAITING: usize = 256;

impl Trips {
    fn moved(&mut self, id: u64) {
        self.moves += 1;
        let now = Instant::now();
        for queue in [&mut self.waiting, &mut self.blurry] {
            if queue.len() == TRIPS_WAITING {
                queue.pop_front();
            }
            queue.push_back((id, now));
        }
    }

    fn framed(&mut self, request: u64, exact: bool) {
        settle(&mut self.waiting, request, &mut self.answered);
        if exact {
            settle(&mut self.blurry, request, &mut self.sharp);
        }
    }
}

fn settle(queue: &mut VecDeque<(u64, Instant)>, request: u64, into: &mut Vec<u64>) {
    while let Some(&(id, at)) = queue.front()
        && id <= request
    {
        queue.pop_front();
        if id == request {
            into.push(at.elapsed().as_nanos() as u64);
        }
    }
}

async fn serve(
    stream: LensStream,
    viewer: Arc<Viewer>,
    mut input: mpsc::UnboundedReceiver<Input>,
    copied: impl Fn(String) + Send + 'static,
    trips: Arc<Mutex<Trips>>,
) -> Result<()> {
    let LensStream { mut send, receive } = stream;
    let (acks, mut acked) = mpsc::unbounded_channel();
    let reader = tokio::spawn(receive_frames(receive, viewer, acks, copied, trips.clone()));
    let _reader = AbortOnDrop(reader.abort_handle());
    let mut move_id = 0;
    let mut scroll = (0., 0.);
    loop {
        let message = tokio::select! {
            biased;
            serial = acked.recv() => match serial {
                Some(serial) => LensMessage::Ack { serial },
                None => break,
            },
            next = input.recv() => {
                let Some(mut next) = next else { break };
                while let Input::Move { .. } = next {
                    match input.try_recv() {
                        Ok(newer @ Input::Move { .. }) => next = newer,
                        Ok(other) => {
                            send_input(&mut send, &mut move_id, &mut scroll, &trips, next).await?;
                            next = other;
                        }
                        Err(_) => break,
                    }
                }
                if let Input::Close = next {
                    break;
                }
                send_input(&mut send, &mut move_id, &mut scroll, &trips, next).await?;
                continue;
            }
        };
        write(&mut send, message, None).await?;
    }
    let _ = send.finish();
    if reader.is_finished() {
        return reader.await?;
    }
    Ok(())
}

async fn send_input(
    send: &mut quinn::SendStream,
    move_id: &mut u64,
    scroll: &mut (f64, f64),
    trips: &Mutex<Trips>,
    input: Input,
) -> Result<()> {
    let pixel = |value: f64| value.round().clamp(0., u16::MAX as f64) as u16;
    let message = match input {
        Input::Move { x, y, radius } => {
            *move_id += 1;
            trips
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .moved(*move_id);
            LensMessage::Move {
                x: pixel(x),
                y: pixel(y),
                radius: pixel(radius).clamp(1, MAX_RADIUS),
                id: *move_id,
            }
        }
        Input::Button { button, pressed } => LensMessage::Button { button, pressed },
        Input::Key { usage, pressed } => LensMessage::Key { usage, pressed },
        Input::Scroll { dx, dy } => {
            scroll.0 += dx;
            scroll.1 += dy;
            let whole = (scroll.0.trunc(), scroll.1.trunc());
            *scroll = (scroll.0 - whole.0, scroll.1 - whole.1);
            if whole == (0., 0.) {
                return Ok(());
            }
            LensMessage::Scroll {
                dx: whole.0 as i32,
                dy: whole.1 as i32,
            }
        }
        Input::Copy => LensMessage::Copy,
        Input::Close => return Ok(()),
    };
    write(send, message, None).await
}

async fn receive_frames(
    mut receive: quinn::RecvStream,
    viewer: Arc<Viewer>,
    acks: mpsc::UnboundedSender<u64>,
    copied: impl Fn(String),
    trips: Arc<Mutex<Trips>>,
) -> Result<()> {
    let mut decoder = None;
    while let Some((message, body)) = read(&mut receive).await? {
        match message {
            LensMessage::Ready { width, height } => {
                decoder = Some(Decoder::new(width.into(), height.into())?);
                viewer.ready(width.into(), height.into());
            }
            LensMessage::Frame {
                serial,
                request,
                exact,
            } => {
                if let Some(body) = body.filter(|body| !body.is_empty()) {
                    let Some(decoder) = decoder.as_mut() else {
                        bail!("tiles arrived before the desktop size");
                    };
                    let changed = decoder.apply(&body)?;
                    viewer.show(&decoder.pixels, &changed);
                }
                trips
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .framed(request, exact);
                if acks.send(serial).is_err() {
                    break;
                }
            }
            LensMessage::Copied { text } => copied(text),
            LensMessage::Refused { reason } => bail!(reason),
            other => bail!("the viewed computer sent {other:?}"),
        }
    }
    Ok(())
}

struct AbortOnDrop(tokio::task::AbortHandle);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}
