use super::api::Request;
use anyhow::Result;
use serde_json::Value;
use std::path::Path;

pub struct Client {
    #[cfg(target_os = "macos")]
    engine: super::host::Engine,
    #[cfg(target_os = "linux")]
    bus: zbus::blocking::Connection,
    #[cfg(windows)]
    pipe: super::windows_view::Pipe,
}

impl Client {
    pub fn connect(config: &Path) -> Result<Self> {
        #[cfg(target_os = "macos")]
        return Ok(Self {
            engine: super::host::Engine::open(config.to_path_buf())?,
        });
        #[cfg(target_os = "linux")]
        {
            let _ = config;
            Ok(Self {
                bus: session_bus()?,
            })
        }
        #[cfg(windows)]
        return Ok(Self {
            pipe: super::windows_view::Pipe::new(config)?,
        });
        #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
        {
            let _ = config;
            anyhow::bail!("Nidus does not run on this system")
        }
    }

    pub fn call(&self, request: &Request) -> Result<Value> {
        #[cfg(target_os = "macos")]
        return self.engine.request(request.clone());
        #[cfg(target_os = "linux")]
        {
            let reply: String = self
                .bus
                .call_method(
                    Some(super::gnome::BUS),
                    super::gnome::PATH,
                    Some(super::gnome::BUS),
                    "Call",
                    &serde_json::to_string(request)?,
                )
                .map_err(agent_error)?
                .body()
                .deserialize()?;
            Ok(serde_json::from_str(&reply)?)
        }
        #[cfg(windows)]
        return self.pipe.call(request);
        #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
        {
            let _ = request;
            anyhow::bail!("Nidus does not run on this system")
        }
    }

    #[cfg(target_os = "macos")]
    pub fn lens(&self) -> &std::sync::Arc<crate::lens::viewer::Viewer> {
        self.engine.lens()
    }
}

#[cfg(target_os = "linux")]
static BUS_RUNTIME: std::sync::LazyLock<Option<tokio::runtime::Runtime>> =
    std::sync::LazyLock::new(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("nid-bus")
            .enable_all()
            .build()
            .ok()
    });

#[cfg(target_os = "linux")]
pub fn on_bus_runtime<T>(work: impl FnOnce() -> Result<T>) -> Result<T> {
    let runtime = BUS_RUNTIME
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Could not start the D-Bus runtime"))?;
    let _inside = runtime.enter();
    work()
}

#[cfg(target_os = "linux")]
pub fn session_bus() -> Result<zbus::blocking::Connection> {
    on_bus_runtime(|| Ok(zbus::blocking::Connection::session()?))
}

#[cfg(target_os = "linux")]
pub fn system_bus() -> Result<zbus::blocking::Connection> {
    on_bus_runtime(|| Ok(zbus::blocking::Connection::system()?))
}

pub const ANSWERS_WITH_SNAPSHOT: bool = !cfg!(target_os = "linux");

#[cfg(target_os = "linux")]
fn agent_error(error: zbus::Error) -> anyhow::Error {
    match error {
        zbus::Error::MethodError(_, Some(message), _) => anyhow::anyhow!(message),
        zbus::Error::MethodError(name, None, _) if name.as_str().ends_with("ServiceUnknown") => {
            anyhow::anyhow!("Nidus's desktop service is not running")
        }
        error => error.into(),
    }
}

#[cfg(target_os = "linux")]
pub const AGENT_API: u32 = super::gnome::API;
