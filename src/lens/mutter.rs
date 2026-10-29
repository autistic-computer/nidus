use anyhow::{Context, Result, ensure};
use std::collections::{BTreeSet, HashMap};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::sync::watch;
use zbus::zvariant::{DynamicType, OwnedObjectPath, OwnedValue, Value};

const REMOTE_DESKTOP: &str = "org.gnome.Mutter.RemoteDesktop";
const REMOTE_DESKTOP_PATH: &str = "/org/gnome/Mutter/RemoteDesktop";
const SESSION: &str = "org.gnome.Mutter.RemoteDesktop.Session";
const SCREEN_CAST: &str = "org.gnome.Mutter.ScreenCast";
const SCREEN_CAST_PATH: &str = "/org/gnome/Mutter/ScreenCast";
const CAST_SESSION: &str = "org.gnome.Mutter.ScreenCast.Session";
const STREAM: &str = "org.gnome.Mutter.ScreenCast.Stream";
const CALL: Duration = Duration::from_secs(2);
const NODE: Duration = Duration::from_secs(5);
const MAX_SELECTION: usize = 3 << 20;
const TEXT_TYPES: [&str; 3] = ["text/plain;charset=utf-8", "UTF8_STRING", "text/plain"];
const AXIS_CONTINUOUS: u32 = 8;

#[derive(Default)]
struct Held {
    keys: BTreeSet<u32>,
    buttons: BTreeSet<u32>,
}

#[derive(Clone, Default)]
struct Selection {
    generation: u64,
    types: Vec<String>,
}

pub struct Desk {
    session: zbus::Proxy<'static>,
    stream: OwnedObjectPath,
    node: u32,
    held: Mutex<Held>,
    selection: watch::Receiver<Selection>,
    watcher: tokio::task::JoinHandle<()>,
    stopped: bool,
}

fn lock(held: &Mutex<Held>) -> MutexGuard<'_, Held> {
    held.lock().unwrap_or_else(|e| e.into_inner())
}

async fn call<B, R>(proxy: &zbus::Proxy<'_>, method: &str, body: &B) -> Result<R>
where
    B: serde::Serialize + DynamicType,
    R: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
{
    tokio::time::timeout(CALL, proxy.call(method, body))
        .await
        .with_context(|| format!("Mutter did not answer {method}"))?
        .with_context(|| format!("Mutter refused {method}"))
}

async fn next_message(stream: &mut zbus::MessageStream) -> Option<zbus::Result<zbus::Message>> {
    use zbus::export::futures_core::Stream;
    std::future::poll_fn(|context| std::pin::Pin::new(&mut *stream).poll_next(context)).await
}

async fn signals(
    bus: &zbus::Connection,
    owner: &str,
    path: &OwnedObjectPath,
    interface: &'static str,
    member: &'static str,
) -> Result<zbus::MessageStream> {
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(owner)?
        .path(path.as_str())?
        .interface(interface)?
        .member(member)?
        .build();
    Ok(zbus::MessageStream::for_match_rule(rule, bus, None).await?)
}

fn strings(value: &Value<'_>) -> Vec<String> {
    match value {
        Value::Array(array) => array
            .inner()
            .iter()
            .filter_map(|value| match value {
                Value::Str(text) => Some(text.to_string()),
                _ => None,
            })
            .collect(),
        Value::Structure(fields) => fields.fields().first().map(strings).unwrap_or_default(),
        Value::Value(inner) => strings(inner),
        _ => Vec::new(),
    }
}

fn text_type(types: &[String]) -> Option<&'static str> {
    TEXT_TYPES
        .into_iter()
        .find(|text| types.iter().any(|offered| offered == text))
}

fn owner_changed(mut options: HashMap<String, OwnedValue>) -> Vec<String> {
    let owned = options
        .remove("session-is-owner")
        .and_then(|value| bool::try_from(value).ok())
        .unwrap_or(false);
    match options.remove("mime-types") {
        Some(types) if !owned => strings(&types),
        _ => Vec::new(),
    }
}

async fn watch_selection(mut owners: zbus::MessageStream, selection: watch::Sender<Selection>) {
    while let Some(message) = next_message(&mut owners).await {
        let options = message.map_err(anyhow::Error::from).and_then(|message| {
            let (options,): (HashMap<String, OwnedValue>,) = message.body().deserialize()?;
            Ok(options)
        });
        match options {
            Ok(options) => {
                let types = owner_changed(options);
                selection.send_modify(|selection| {
                    selection.generation += 1;
                    selection.types = types;
                    tracing::debug!(
                        generation = selection.generation,
                        types = selection.types.len(),
                        "remote desktop selection changed"
                    );
                });
            }
            Err(error) => tracing::debug!(%error, "unreadable selection change"),
        }
    }
}

impl Desk {
    pub async fn start(bus: &zbus::Connection) -> Result<Desk> {
        let remote =
            zbus::Proxy::new(bus, REMOTE_DESKTOP, REMOTE_DESKTOP_PATH, REMOTE_DESKTOP).await?;
        let path: OwnedObjectPath = call(&remote, "CreateSession", &()).await?;
        let session = zbus::Proxy::new(bus, REMOTE_DESKTOP, path.clone(), SESSION).await?;
        match Self::record(bus, session.clone(), &path).await {
            Ok(desk) => Ok(desk),
            Err(error) => {
                if let Err(error) = call::<_, ()>(&session, "Stop", &()).await {
                    tracing::debug!(error = %format_args!("{error:#}"), "remote desktop session not stopped");
                }
                Err(error)
            }
        }
    }

    async fn record(
        bus: &zbus::Connection,
        session: zbus::Proxy<'static>,
        path: &OwnedObjectPath,
    ) -> Result<Desk> {
        let properties = zbus::Proxy::new(
            bus,
            REMOTE_DESKTOP,
            path.clone(),
            "org.freedesktop.DBus.Properties",
        )
        .await?;
        let id: OwnedValue = call(&properties, "Get", &(SESSION, "SessionId")).await?;
        let id = String::try_from(id).context("Mutter sent a session id that is not text")?;
        let screen_cast = zbus::Proxy::new(bus, SCREEN_CAST, SCREEN_CAST_PATH, SCREEN_CAST).await?;
        let options = HashMap::from([("remote-desktop-session-id", Value::from(id))]);
        let cast_path: OwnedObjectPath = call(&screen_cast, "CreateSession", &(options,)).await?;
        let cast = zbus::Proxy::new(bus, SCREEN_CAST, cast_path, CAST_SESSION).await?;
        let options = HashMap::from([("cursor-mode", Value::U32(0))]);
        let stream: OwnedObjectPath = call(&cast, "RecordMonitor", &("", options)).await?;

        let names = zbus::fdo::DBusProxy::new(bus).await?;
        let caster = names.get_name_owner(SCREEN_CAST.try_into()?).await?;
        let desktop = names.get_name_owner(REMOTE_DESKTOP.try_into()?).await?;
        let mut added = signals(bus, &caster, &stream, STREAM, "PipeWireStreamAdded").await?;
        let owners = signals(bus, &desktop, path, SESSION, "SelectionOwnerChanged").await?;

        call::<_, ()>(&session, "Start", &()).await?;
        let message = tokio::time::timeout(NODE, next_message(&mut added))
            .await
            .context("Mutter did not name the PipeWire node")?
            .context("the session bus closed")??;
        let (node,): (u32,) = message.body().deserialize()?;
        drop(added);

        let (changes, selection) = watch::channel(Selection::default());
        let watcher = tokio::spawn(watch_selection(owners, changes));
        let desk = Desk {
            session,
            stream,
            node,
            held: Mutex::default(),
            selection,
            watcher,
            stopped: false,
        };
        let options = HashMap::<&str, Value>::new();
        call::<_, ()>(&desk.session, "EnableClipboard", &(options,)).await?;
        tracing::info!(
            node,
            stream = desk.stream.as_str(),
            "remote desktop session started"
        );
        Ok(desk)
    }

    pub fn node(&self) -> u32 {
        self.node
    }

    async fn send<B>(&self, method: &str, body: &B) -> Result<()>
    where
        B: serde::Serialize + DynamicType,
    {
        tokio::time::timeout(CALL, self.session.call_noreply(method, body))
            .await
            .with_context(|| format!("Mutter did not take {method}"))?
            .with_context(|| format!("Mutter refused {method}"))
    }

    pub async fn pointer(&self, x: f64, y: f64) -> Result<()> {
        ensure!(
            x.is_finite() && y.is_finite(),
            "pointer position is not finite"
        );
        self.send("NotifyPointerMotionAbsolute", &(self.stream.as_str(), x, y))
            .await
    }

    pub async fn button(&self, code: u32, pressed: bool) -> Result<()> {
        let button = i32::try_from(code).context("button code out of range")?;
        call::<_, ()>(&self.session, "NotifyPointerButton", &(button, pressed)).await?;
        let mut held = lock(&self.held);
        if pressed {
            held.buttons.insert(code);
        } else {
            held.buttons.remove(&code);
        }
        Ok(())
    }

    pub async fn key(&self, code: u32, pressed: bool) -> Result<()> {
        call::<_, ()>(&self.session, "NotifyKeyboardKeycode", &(code, pressed)).await?;
        let mut held = lock(&self.held);
        if pressed {
            held.keys.insert(code);
        } else {
            held.keys.remove(&code);
        }
        Ok(())
    }

    pub async fn scroll(&self, dx: f64, dy: f64) -> Result<()> {
        ensure!(
            dx.is_finite() && dy.is_finite(),
            "scroll delta is not finite"
        );
        if dx == 0. && dy == 0. {
            return Ok(());
        }
        self.send("NotifyPointerAxis", &(dx, dy, AXIS_CONTINUOUS))
            .await
    }

    pub fn selection_generation(&self) -> u64 {
        self.selection.borrow().generation
    }

    pub async fn read_selection(&self, generation: u64, timeout: Duration) -> Result<String> {
        let deadline = tokio::time::Instant::now() + timeout;
        let mut changes = self.selection.clone();
        let wait = async {
            loop {
                {
                    let selection = changes.borrow_and_update();
                    if selection.generation > generation
                        && let Some(mime) = text_type(&selection.types)
                    {
                        return anyhow::Ok(mime);
                    }
                }
                changes
                    .changed()
                    .await
                    .context("the remote desktop session closed")?;
            }
        };
        let mime = match tokio::time::timeout_at(deadline, wait).await {
            Ok(mime) => mime?,
            Err(_) if self.selection_generation() > generation => {
                anyhow::bail!("the clipboard holds no text")
            }
            Err(_) => anyhow::bail!("nothing new was copied"),
        };
        let fd: zbus::zvariant::OwnedFd =
            tokio::time::timeout_at(deadline, self.session.call("SelectionRead", &(mime,)))
                .await
                .context("Mutter did not answer SelectionRead")?
                .context("Mutter refused SelectionRead")?;
        let mut pipe = tokio::net::unix::pipe::Receiver::from_owned_fd(fd.into())?;
        let mut text = Vec::new();
        tokio::time::timeout_at(
            deadline,
            (&mut pipe)
                .take(MAX_SELECTION as u64 + 1)
                .read_to_end(&mut text),
        )
        .await
        .context("the copying program did not finish sending")??;
        ensure!(
            text.len() <= MAX_SELECTION,
            "copied text exceeds {MAX_SELECTION} bytes"
        );
        let text = String::from_utf8(text).context("copied text is not UTF-8")?;
        tracing::debug!(mime, bytes = text.len(), "remote desktop selection read");
        Ok(text)
    }

    pub async fn release(&self) -> Result<()> {
        let (buttons, keys) = {
            let held = lock(&self.held);
            (held.buttons.clone(), held.keys.clone())
        };
        for button in buttons {
            self.button(button, false).await?;
        }
        for key in keys {
            self.key(key, false).await?;
        }
        Ok(())
    }

    pub async fn stop(mut self) -> Result<()> {
        let released = self.release().await;
        self.stopped = true;
        call::<_, ()>(&self.session, "Stop", &()).await?;
        tracing::info!(node = self.node, "remote desktop session stopped");
        released
    }
}

impl Drop for Desk {
    fn drop(&mut self) {
        self.watcher.abort();
        if self.stopped {
            return;
        }
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let session = self.session.clone();
            runtime.spawn(async move {
                let _ = tokio::time::timeout(CALL, session.call_noreply("Stop", &())).await;
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::{Array, StructureBuilder};

    fn types(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn utf8_text_types_are_preferred_over_plain_text() {
        let offered = types(&["text/plain", "image/png", "UTF8_STRING"]);
        assert_eq!(text_type(&offered), Some("UTF8_STRING"));
        let offered = types(&["text/plain", "text/plain;charset=utf-8"]);
        assert_eq!(text_type(&offered), Some("text/plain;charset=utf-8"));
        assert_eq!(text_type(&types(&["image/png"])), None);
    }

    #[test]
    fn mime_types_are_read_bare_or_wrapped_in_a_tuple() {
        let bare = Value::from(Array::from(vec!["text/plain", "image/png"]));
        assert_eq!(strings(&bare), types(&["text/plain", "image/png"]));
        let wrapped = Value::from(
            StructureBuilder::new()
                .add_field(Array::from(vec!["UTF8_STRING"]))
                .build()
                .unwrap(),
        );
        assert_eq!(strings(&wrapped), types(&["UTF8_STRING"]));
        assert!(strings(&Value::from(7_u32)).is_empty());
    }

    #[test]
    fn a_selection_this_session_owns_offers_nothing_to_read() {
        let options = |owned: bool| {
            HashMap::from([
                (
                    "mime-types".to_string(),
                    OwnedValue::try_from(Value::from(Array::from(vec!["text/plain"]))).unwrap(),
                ),
                ("session-is-owner".to_string(), OwnedValue::from(owned)),
            ])
        };
        assert_eq!(owner_changed(options(false)), types(&["text/plain"]));
        assert!(owner_changed(options(true)).is_empty());
    }
}
