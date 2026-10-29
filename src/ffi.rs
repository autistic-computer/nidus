use crate::{
    app::NativeApp,
    lens::viewer::{Input, Viewer},
};
use std::{
    ffi::{CStr, CString, c_char},
    path::PathBuf,
    sync::{Arc, mpsc},
    thread::JoinHandle,
    time::Duration,
};

type Reply = mpsc::Sender<String>;
pub struct ZflowApp {
    commands: mpsc::Sender<(String, Reply)>,
    thread: Option<JoinHandle<()>>,
    lens: Arc<Viewer>,
}

fn response(result: anyhow::Result<serde_json::Value>) -> String {
    match result {
        Ok(snapshot) => serde_json::json!({"snapshot":snapshot}),
        Err(error) => serde_json::json!({"error":format!("{error:#}")}),
    }
    .to_string()
}
fn string(value: String) -> *mut c_char {
    CString::new(value)
        .expect("JSON contains no NUL")
        .into_raw()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn zflow_app_create(
    path: *const c_char,
    error: *mut *mut c_char,
) -> *mut ZflowApp {
    if path.is_null() || error.is_null() {
        return std::ptr::null_mut();
    }
    unsafe {
        *error = std::ptr::null_mut();
    }
    let result = std::panic::catch_unwind(|| -> anyhow::Result<ZflowApp> {
        let path = PathBuf::from(unsafe { CStr::from_ptr(path) }.to_str()?);
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "warn,zflow=info".into()),
            )
            .with_writer(std::io::stderr)
            .with_ansi(false)
            .try_init();
        crate::macos::install_exit_handlers();
        let mut app = NativeApp::open(path)?;
        let lens = app.lens();
        let (commands, requests) = mpsc::channel::<(String, Reply)>();
        let thread = std::thread::Builder::new()
            .name("zflow-app".into())
            .spawn(move || {
                loop {
                    match requests.recv_timeout(Duration::from_millis(12)) {
                        Ok((request, reply)) => {
                            let result = serde_json::from_str(&request)
                                .map_err(anyhow::Error::from)
                                .and_then(|request| app.request(request));
                            let _ = reply.send(response(result));
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                    app.tick();
                }
            })?;
        Ok(ZflowApp {
            commands,
            thread: Some(thread),
            lens,
        })
    })
    .unwrap_or_else(|_| Err(anyhow::anyhow!("The sharing engine could not start")));
    match result {
        Ok(app) => Box::into_raw(Box::new(app)),
        Err(problem) => {
            unsafe {
                *error = string(format!("{problem:#}"));
            }
            std::ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn zflow_app_request(
    app: *mut ZflowApp,
    request: *const c_char,
) -> *mut c_char {
    if app.is_null() || request.is_null() {
        return string(response(Err(anyhow::anyhow!("Missing request"))));
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> anyhow::Result<String> {
            let (app, request) = unsafe { (&*app, CStr::from_ptr(request).to_str()?) };
            let (send, receive) = mpsc::channel();
            app.commands.send((request.to_owned(), send))?;
            Ok(receive.recv_timeout(Duration::from_secs(15))?)
        },
    ))
    .unwrap_or_else(|_| Err(anyhow::anyhow!("The sharing engine stopped unexpectedly")));
    string(result.unwrap_or_else(|error| response(Err(error))))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn zflow_string_free(value: *mut c_char) {
    if !value.is_null() {
        unsafe {
            drop(CString::from_raw(value));
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn zflow_app_destroy(app: *mut ZflowApp) {
    if app.is_null() {
        return;
    }
    let mut app = unsafe { Box::from_raw(app) };
    let thread = app.thread.take();
    drop(app);
    if let Some(thread) = thread {
        let _ = thread.join();
    }
}

#[repr(C)]
pub struct ZflowLensStatus {
    pub state: u32,
    pub width: u32,
    pub height: u32,
    pub frames: u64,
}

fn lens<'a>(app: *const ZflowApp) -> Option<&'a Viewer> {
    (!app.is_null()).then(|| unsafe { &*(*app).lens })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn zflow_lens_status(app: *const ZflowApp) -> ZflowLensStatus {
    let status = lens(app).map(Viewer::status).unwrap_or_default();
    ZflowLensStatus {
        state: status.state as u32,
        width: status.width,
        height: status.height,
        frames: status.frames,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn zflow_lens_error(app: *const ZflowApp) -> *mut c_char {
    lens(app)
        .and_then(Viewer::error)
        .map_or(std::ptr::null_mut(), |error| {
            string(error.replace('\0', ""))
        })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn zflow_lens_dirty(
    app: *const ZflowApp,
    x: f64,
    y: f64,
    radius: f64,
    ids: *mut u32,
    capacity: usize,
) -> usize {
    let Some(viewer) = lens(app) else { return 0 };
    if ids.is_null() || capacity == 0 {
        return 0;
    }
    let ids = unsafe { std::slice::from_raw_parts_mut(ids, capacity) };
    viewer.dirty(x, y, radius, ids)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn zflow_lens_chunk(
    app: *const ZflowApp,
    id: u32,
    pixels: *mut u8,
    length: usize,
    stride: usize,
    width: *mut u32,
    height: *mut u32,
) -> bool {
    let Some(viewer) = lens(app) else {
        return false;
    };
    if pixels.is_null() || width.is_null() || height.is_null() {
        return false;
    }
    let pixels = unsafe { std::slice::from_raw_parts_mut(pixels, length) };
    let Some((w, h)) = viewer.chunk(id as usize, pixels, stride) else {
        return false;
    };
    unsafe {
        *width = w as u32;
        *height = h as u32;
    }
    true
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn zflow_lens_move(app: *const ZflowApp, x: f64, y: f64, radius: f64) {
    if let Some(viewer) = lens(app)
        && x.is_finite()
        && y.is_finite()
        && radius.is_finite()
    {
        viewer.send(Input::Move { x, y, radius });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn zflow_lens_button(app: *const ZflowApp, button: u32, pressed: bool) {
    let button = match button {
        1 => 1,
        2 => 3,
        3 => 2,
        4 | 5 => button as u8,
        _ => return,
    };
    if let Some(viewer) = lens(app) {
        viewer.send(Input::Button { button, pressed });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn zflow_lens_key(app: *const ZflowApp, keycode: u16, pressed: bool) {
    if let Some(viewer) = lens(app)
        && let Some(usage) = crate::macos::keys::mac_keycode_to_hid(keycode)
    {
        viewer.send(Input::Key { usage, pressed });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn zflow_lens_scroll(app: *const ZflowApp, dx: f64, dy: f64) {
    if let Some(viewer) = lens(app)
        && dx.is_finite()
        && dy.is_finite()
    {
        viewer.send(Input::Scroll { dx, dy });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn zflow_lens_copy(app: *const ZflowApp) {
    if let Some(viewer) = lens(app) {
        viewer.send(Input::Copy);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn zflow_lens_close(app: *const ZflowApp) {
    if let Some(viewer) = lens(app) {
        viewer.send(Input::Close);
    }
}
