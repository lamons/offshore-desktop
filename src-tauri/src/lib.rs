use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_opener::OpenerExt;

struct SidecarState {
    child: Mutex<Option<Child>>,
    quitting: AtomicBool,
}

impl SidecarState {
    fn new() -> Self {
        Self {
            child: Mutex::new(None),
            quitting: AtomicBool::new(false),
        }
    }

    fn set_child(&self, child: Child) {
        if let Ok(mut guard) = self.child.lock() {
            *guard = Some(child);
        }
    }

    fn mark_quitting(&self) {
        self.quitting.store(true, Ordering::SeqCst);
    }

    fn is_quitting(&self) -> bool {
        self.quitting.load(Ordering::SeqCst)
    }

    fn shutdown(&self) {
        self.mark_quitting();
        let mut child = match self.child.lock() {
            Ok(mut guard) => match guard.take() {
                Some(c) => c,
                None => return,
            },
            Err(_) => return,
        };

        // Dropping stdin signals `--exit-on-stdin-eof` for a clean FastAPI/uvicorn shutdown.
        drop(child.stdin.take());

        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            match child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                Err(_) => break,
            }
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn sidecar_exe_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "offshore-sidecar.exe"
    } else {
        "offshore-sidecar"
    }
}

fn build_sidecar_command(app: &AppHandle) -> Result<Command, String> {
    let serve_args = [
        "serve",
        "--host",
        "127.0.0.1",
        "--ephemeral-port",
        "--ready-line",
        "--exit-on-stdin-eof",
        "--log-level",
        "warning",
    ];

    // 1. Prefer the bundled Nuitka standalone sidecar inside app resources.
    if let Ok(resource_dir) = app.path().resource_dir() {
        let bundled = resource_dir.join("sidecar").join(sidecar_exe_name());
        if bundled.exists() {
            let mut cmd = Command::new(bundled);
            cmd.args(serve_args);
            return Ok(cmd);
        }
    }

    // 2. Also check local build output next to Cargo.toml during development.
    let manifest_sidecar = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("sidecar")
        .join(sidecar_exe_name());
    if manifest_sidecar.exists() {
        let mut cmd = Command::new(manifest_sidecar);
        cmd.args(serve_args);
        return Ok(cmd);
    }

    // 3. Development fallback: run `python -m offshore serve ...` directly.
    if let Ok(py) = std::env::var("OFFSHORE_DEV_PYTHON") {
        let mut cmd = Command::new(py);
        cmd.args(["-m", "offshore"]).args(serve_args);
        return Ok(cmd);
    }

    let venv_rel = if cfg!(target_os = "windows") {
        "../../.offshoredev/Scripts/python.exe"
    } else {
        "../../.offshoredev/bin/python"
    };
    let dev_python = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(venv_rel);
    if dev_python.exists() {
        let mut cmd = Command::new(dev_python);
        cmd.args(["-m", "offshore"]).args(serve_args);
        return Ok(cmd);
    }

    Err(
        "Could not locate the bundled Offshore sidecar. Run scripts/build_sidecar.sh (or set OFFSHORE_DEV_PYTHON for dev)."
            .to_string(),
    )
}

fn spawn_sidecar(app: AppHandle) {
    std::thread::spawn(move || {
        let mut cmd = match build_sidecar_command(&app) {
            Ok(c) => c,
            Err(err) => {
                report_startup_error(&app, &err);
                return;
            }
        };

        cmd.env("PYTHONUNBUFFERED", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(err) => {
                report_startup_error(&app, &format!("Failed to start Offshore backend: {err}"));
                return;
            }
        };

        let stdout = match child.stdout.take() {
            Some(s) => s,
            None => {
                let _ = child.kill();
                report_startup_error(&app, "Failed to capture backend stdout.");
                return;
            }
        };
        let stderr = child.stderr.take();

        app.state::<SidecarState>().set_child(child);

        let reader = BufReader::new(stdout);
        let mut ready_url: Option<String> = None;

        for line in reader.lines().map_while(Result::ok) {
            if let Some(url) = line.strip_prefix("OFFSHORE_READY ") {
                let trimmed = url.trim().to_string();
                if !trimmed.is_empty() {
                    ready_url = Some(trimmed);
                    break;
                }
            }
        }

        match ready_url {
            Some(url_str) => {
                if let Ok(parsed) = url_str.parse::<tauri::Url>() {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.navigate(parsed);
                    }
                } else {
                    report_startup_error(&app, &format!("Invalid backend URL: {url_str}"));
                }
            }
            None => {
                let mut detail = String::from("Offshore backend exited before becoming ready.");
                if let Some(err_stream) = stderr {
                    let err_text: String = BufReader::new(err_stream)
                        .lines()
                        .map_while(Result::ok)
                        .collect::<Vec<_>>()
                        .join("\n");
                    if !err_text.trim().is_empty() {
                        detail.push_str("\n\n");
                        detail.push_str(err_text.trim());
                    }
                }
                report_startup_error(&app, &detail);
            }
        }
    });
}

fn report_startup_error(app: &AppHandle, message: &str) {
    if let Some(window) = app.get_webview_window("main") {
        if let Ok(json_msg) = serde_json::to_string(message) {
            let js = format!("window.__offshoreStartupError && window.__offshoreStartupError({json_msg});");
            let _ = window.eval(&js);
        }
    }
}

fn is_internal_url(url: &tauri::Url) -> bool {
    match url.scheme() {
        "tauri" | "about" | "blob" | "data" => true,
        "http" => matches!(
            url.host_str(),
            Some("127.0.0.1" | "localhost" | "[::1]" | "tauri.localhost")
        ),
        _ => false,
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .manage(SidecarState::new())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .setup(|app| {
            let app_handle = app.handle().clone();

            // Build the main window in Rust so we can attach an `on_navigation` guard
            // that opens external links in the user's default browser.
            let nav_handle = app_handle.clone();
            let main_window = WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::App("index.html".into()),
            )
            .title("Offshore")
            .inner_size(1200.0, 820.0)
            .min_inner_size(720.0, 520.0)
            .center()
            .on_navigation(move |url| {
                if is_internal_url(url) {
                    true
                } else {
                    let _ = nav_handle.opener().open_url(url.as_str(), None::<&str>);
                    false
                }
            })
            .build()?;

            // Minimize to menu bar / system tray when the window close button is clicked.
            let close_handle = app_handle.clone();
            main_window.on_window_event(move |event| {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    let state = close_handle.state::<SidecarState>();
                    if !state.is_quitting() {
                        api.prevent_close();
                        if let Some(win) = close_handle.get_webview_window("main") {
                            let _ = win.hide();
                        }
                    }
                }
            });

            // Build system tray / menu bar icon with "Open Offshore" and "Quit Offshore".
            let show_item = MenuItem::with_id(app, "show", "Open Offshore", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit Offshore", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&show_item, &quit_item])?;

            let mut tray_builder = TrayIconBuilder::new()
                .menu(&tray_menu)
                .tooltip("Offshore")
                .show_menu_on_left_click(cfg!(target_os = "macos"))
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main_window(app),
                    "quit" => {
                        app.state::<SidecarState>().shutdown();
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        if !cfg!(target_os = "macos") {
                            show_main_window(tray.app_handle());
                        }
                    }
                });

            if let Some(icon) = app.default_window_icon().cloned() {
                tray_builder = tray_builder.icon(icon);
            }
            tray_builder.build(app)?;

            spawn_sidecar(app_handle);

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Offshore desktop application");

    app.run(|app_handle, event| match event {
        #[cfg(target_os = "macos")]
        RunEvent::Reopen { .. } => {
            show_main_window(app_handle);
        }
        RunEvent::Exit => {
            app_handle.state::<SidecarState>().shutdown();
        }
        _ => {}
    });
}
