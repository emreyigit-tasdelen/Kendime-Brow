//! UI ↔ shell bridge on top of the CEF message router (architecture §3.3, task 1.8).
//!
//! - Renderer side: `window.kbQuery` / `window.kbQueryCancel` are injected ONLY into frames
//!   whose URL is `kb://…`; web pages never see them.
//! - Browser side: every query is checked again against the frame URL before it runs.

use std::sync::{Arc, Mutex, OnceLock};

use cef::wrapper::message_router::*;
use cef::*;

use crate::protocol::{self, Command};
use crate::shell;

pub fn config() -> MessageRouterConfig {
    MessageRouterConfig {
        js_query_function: "kbQuery".into(),
        js_cancel_function: "kbQueryCancel".into(),
        ..Default::default()
    }
}

// ---------------------------------------------------------------- browser process

static BROWSER_ROUTER: OnceLock<Arc<BrowserSideRouter>> = OnceLock::new();

type Subscriber = (i64, Arc<Mutex<dyn BrowserSideCallback>>);
static SUBSCRIBERS: Mutex<Vec<Subscriber>> = Mutex::new(Vec::new());

/// Creates the browser-side router and its handler. Call once on the UI thread.
pub fn init_browser_side() {
    let router = BROWSER_ROUTER.get_or_init(|| BrowserSideRouter::new(config()));
    router.add_handler(Arc::new(ShellQueryHandler), false);
}

pub fn browser_router() -> Option<&'static Arc<BrowserSideRouter>> {
    BROWSER_ROUTER.get()
}

/// Pushes the current state to every subscribed `kb://` page.
pub fn broadcast(state: &protocol::State) {
    let json = state.to_json();
    if let Ok(subs) = SUBSCRIBERS.lock() {
        for (_, cb) in subs.iter() {
            if let Ok(cb) = cb.lock() {
                cb.success_str(&json);
            }
        }
    }
}

struct ShellQueryHandler;

impl BrowserSideHandler for ShellQueryHandler {
    fn on_query_str(
        &self,
        _browser: Option<Browser>,
        frame: Option<Frame>,
        query_id: i64,
        request: &str,
        persistent: bool,
        callback: Arc<Mutex<dyn BrowserSideCallback>>,
    ) -> bool {
        let fail = |code: i32, msg: &str| {
            if let Ok(cb) = callback.lock() {
                cb.failure(code, msg);
            }
            true
        };
        let url = frame
            .map(|f| CefString::from(&f.url()).to_string())
            .unwrap_or_default();
        if !protocol::is_trusted_origin(&url) {
            return fail(403, "bridge is only available to kb:// pages");
        }
        let cmd = match protocol::parse(request) {
            Ok(c) => c,
            Err(e) => return fail(400, &e),
        };
        match cmd {
            Command::Subscribe => {
                if !persistent {
                    return fail(400, "subscribe must be a persistent query");
                }
                if let Ok(cb) = callback.lock() {
                    cb.success_str(&shell::state().to_json());
                }
                if let Ok(mut subs) = SUBSCRIBERS.lock() {
                    subs.push((query_id, callback));
                }
            }
            Command::Actions { query } => {
                if let Ok(cb) = callback.lock() {
                    cb.success_str(&crate::actions::list_json(&query));
                }
            }
            Command::Profile { history_query } => {
                let json = shell::profile_json(&history_query);
                if let Ok(cb) = callback.lock() {
                    cb.success_str(&json);
                }
            }
            Command::PdfOpen { path } => {
                std::thread::spawn(move || {
                    let r = crate::pdf::read_pdf(&path).and_then(crate::pdf::open);
                    if let Ok(cb) = callback.lock() {
                        match r {
                            Ok(json) => cb.success_str(&String::from_utf8_lossy(&json)),
                            Err(e) => cb.failure(500, &e),
                        }
                    }
                });
            }
            Command::PdfExtract { path, pages } => {
                let json = serde_json::json!({ "pages": pages }).to_string();
                spawn_pdf_edit(callback, move || {
                    crate::pdf::edit_file(&path, "pdf.extract", json, "pages")
                });
            }
            Command::PdfRotate {
                path,
                pages,
                degrees,
            } => {
                let json = serde_json::json!({ "pages": pages, "degrees": degrees }).to_string();
                spawn_pdf_edit(callback, move || {
                    crate::pdf::edit_file(&path, "pdf.rotate", json, "rotated")
                });
            }
            Command::PdfMerge { paths } => {
                spawn_pdf_edit(callback, move || crate::pdf::merge_files(&paths));
            }
            Command::PdfFields => {
                std::thread::spawn(move || {
                    let r = crate::pdf::call_host("pdf.fields", Vec::new());
                    if let Ok(cb) = callback.lock() {
                        match r {
                            Ok(json) => cb.success_str(&String::from_utf8_lossy(&json)),
                            Err(e) => cb.failure(500, &e),
                        }
                    }
                });
            }
            Command::PdfFill { path, values } => {
                let json = serde_json::to_string(&values).unwrap_or_default();
                spawn_pdf_edit(callback, move || {
                    crate::pdf::edit_file(&path, "pdf.fill", json, "filled")
                });
            }
            Command::PdfText => {
                std::thread::spawn(move || {
                    let r = crate::pdf::call_host("pdf.text", Vec::new());
                    if let Ok(cb) = callback.lock() {
                        match r {
                            Ok(json) => cb.success_str(&String::from_utf8_lossy(&json)),
                            Err(e) => cb.failure(500, &e),
                        }
                    }
                });
            }
            Command::PdfRender { page, dpi } => {
                std::thread::spawn(move || {
                    let r = crate::pdf::render(page, dpi.unwrap_or(96.0));
                    if let Ok(cb) = callback.lock() {
                        match r {
                            Ok(png) => {
                                use base64::Engine;
                                cb.success_str(
                                    &base64::engine::general_purpose::STANDARD.encode(png),
                                )
                            }
                            Err(e) => cb.failure(500, &e),
                        }
                    }
                });
            }
            Command::Tasks => {
                let json = serde_json::to_string(&crate::tasks::list()).unwrap_or_default();
                if let Ok(cb) = callback.lock() {
                    cb.success_str(&json);
                }
            }
            Command::State => {
                if let Ok(cb) = callback.lock() {
                    cb.success_str(&shell::state().to_json());
                }
            }
            other => {
                // Answer first, then run the command in a separate UI task: running it here
                // re-enters the router (e.g. on_before_browse of a new tab) while it is busy
                // with this query, which deadlocks.
                if let Ok(cb) = callback.lock() {
                    cb.success_str("{\"ok\":true}");
                }
                let mut task = RunCommandTask::new(Arc::new(Mutex::new(Some(other))));
                post_task(ThreadId::UI, Some(&mut task));
            }
        }
        true
    }

    fn on_query_canceled(&self, _browser: Option<Browser>, _frame: Option<Frame>, query_id: i64) {
        if let Ok(mut subs) = SUBSCRIBERS.lock() {
            subs.retain(|(id, _)| *id != query_id);
        }
    }
}

/// Runs a PDF edit on a worker thread and answers with the output path (or the error).
fn spawn_pdf_edit(
    callback: Arc<Mutex<dyn BrowserSideCallback>>,
    job: impl FnOnce() -> Result<String, String> + Send + 'static,
) {
    std::thread::spawn(move || {
        let r = job();
        if let Ok(cb) = callback.lock() {
            match r {
                Ok(path) => cb.success_str(&serde_json::json!({ "path": path }).to_string()),
                Err(e) => cb.failure(500, &e),
            }
        }
    });
}

wrap_task! {
    struct RunCommandTask {
        cmd: Arc<Mutex<Option<Command>>>,
    }

    impl Task {
        fn execute(&self) {
            if let Some(cmd) = self.cmd.lock().ok().and_then(|mut c| c.take()) {
                shell::run_bridge_command(cmd);
            }
        }
    }
}

// ---------------------------------------------------------------- renderer process

wrap_render_process_handler! {
    pub struct KbRenderProcessHandler {
        router: Arc<RendererSideRouter>,
    }

    impl RenderProcessHandler {
        fn on_context_created(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            context: Option<&mut V8Context>,
        ) {
            let Some(frame) = frame else { return };
            let url = CefString::from(&frame.url()).to_string();
            if std::env::var_os("KB_DEBUG").is_some() {
                eprintln!("[kb] renderer context created: {url}");
            }
            if !protocol::is_trusted_origin(&url) {
                // Web content: fingerprinting protection, never kbQuery.
                if let Some(ctx) = context
                    && let Some(js) = fingerprint_script(&url)
                {
                    let mut retval = None;
                    let mut exception = None;
                    let ok = ctx.eval(
                        Some(&CefString::from(js.as_str())),
                        Some(&CefString::from("kb://fingerprint")),
                        0,
                        Some(&mut retval),
                        Some(&mut exception),
                    );
                    if std::env::var_os("KB_DEBUG").is_some() {
                        let msg = exception
                            .map(|e| CefString::from(&e.message()).to_string())
                            .unwrap_or_default();
                        eprintln!("[kb] fingerprint script on {url}: ok={ok} {msg}");
                    }
                }
                return;
            }
            self.router.on_context_created(
                browser.cloned(),
                Some(frame.clone()),
                context.cloned(),
            );
        }

        fn on_context_released(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            context: Option<&mut V8Context>,
        ) {
            self.router
                .on_context_released(browser.cloned(), frame.cloned(), context.cloned());
        }

        fn on_process_message_received(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            source_process: ProcessId,
            message: Option<&mut ProcessMessage>,
        ) -> i32 {
            self.router.on_process_message_received(
                browser.cloned(),
                frame.cloned(),
                Some(source_process),
                message.cloned(),
            ) as i32
        }
    }
}

static RENDERER_ROUTER: OnceLock<Arc<RendererSideRouter>> = OnceLock::new();

/// (session seed, level) passed by the browser process on this renderer's command line.
fn fp_config() -> (u64, kb_privacy::fingerprint::Level) {
    static CFG: OnceLock<(u64, kb_privacy::fingerprint::Level)> = OnceLock::new();
    *CFG.get_or_init(|| {
        let cl = command_line_get_global();
        let get = |name: &str| {
            cl.as_ref()
                .map(|c| CefString::from(&c.switch_value(Some(&CefString::from(name)))).to_string())
                .unwrap_or_default()
        };
        let seed = get(crate::privacy_flags::SWITCH_FP_SEED)
            .parse()
            .unwrap_or(1);
        let level =
            kb_privacy::fingerprint::Level::parse(&get(crate::privacy_flags::SWITCH_FP_LEVEL));
        (seed, level)
    })
}

/// The protection script for a web frame URL (`None` for non-http(s) or level off).
fn fingerprint_script(url: &str) -> Option<String> {
    let host = kb_privacy::url_clean::host_of(url)?;
    let (seed, level) = fp_config();
    let js =
        kb_privacy::fingerprint::script(level, kb_privacy::fingerprint::site_seed(seed, &host));
    (!js.is_empty()).then_some(js)
}

/// Render process handler sharing one router per process.
pub fn new_render_process_handler() -> RenderProcessHandler {
    let router = RENDERER_ROUTER.get_or_init(|| RendererSideRouter::new(config()));
    KbRenderProcessHandler::new(router.clone())
}
