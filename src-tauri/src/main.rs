#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod config;
mod files;
mod gemini;
mod hotkeys;
mod input;
mod store;
mod system;
mod tools;
mod whatsapp;

use config::Config;
use image::RgbaImage;
use serde_json::{json, Value};
use std::{io::Cursor, sync::Mutex, thread, time::Duration};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, PhysicalPosition, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent,
};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    Dictation,
    Command,
}

#[derive(Default)]
struct AppState {
    cfg: Mutex<Config>,
    mode: Mutex<Option<Mode>>,
    rec: Mutex<Option<audio::Recording>>,
    shot: Mutex<Option<RgbaImage>>,
    circle: Mutex<Option<[f64; 4]>>,
    /// Island's interactive area in physical px relative to its window; everything else is click-through.
    hit: Mutex<[f64; 4]>,
    /// Conversation memory shared by typed chat and voice commands.
    history: Mutex<Vec<Value>>,
    /// One agent turn at a time, so the history stays consistent.
    agent_lock: Mutex<()>,
}

// ---------- island ----------

fn island(app: &AppHandle, state: &str, text: &str) {
    let _ = app.emit("island", json!({ "state": state, "text": text }));
}

fn gemini(app: &AppHandle) -> gemini::Gemini {
    let cfg = app.state::<AppState>().cfg.lock().unwrap().clone();
    gemini::Gemini::new(&cfg.api_key, &cfg.model)
}

/// Must match the args of the windows in tauri.conf.json (WebView2 shares one browser process per arg set).
const BROWSER_ARGS: &str = "--disable-gpu --renderer-process-limit=1 --autoplay-policy=no-user-gesture-required --disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection";

/// Settings is created on demand and destroyed on close, so it costs no RAM while hidden.
fn open_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.unminimize();
        let _ = w.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
        .title("Shiki")
        .inner_size(640.0, 640.0)
        .resizable(false)
        .maximizable(false)
        .center()
        .additional_browser_args(BROWSER_ARGS)
        .build();
    if let Err(e) = built {
        eprintln!("settings window: {e}");
    }
}

// ---------- circle overlay ----------

fn capture_primary() -> Option<RgbaImage> {
    let monitors = xcap::Monitor::all().ok()?;
    let m = monitors.into_iter().find(|m| m.is_primary().unwrap_or(false))?;
    m.capture_image().ok()
}

fn show_overlay(app: &AppHandle) {
    let st = app.state::<AppState>();
    *st.shot.lock().unwrap() = capture_primary();
    *st.circle.lock().unwrap() = None;
    if let Some(w) = app.get_webview_window("overlay") {
        if let Ok(Some(m)) = w.primary_monitor() {
            let _ = w.set_position(*m.position());
            let _ = w.set_size(*m.size());
        }
        let _ = w.emit("overlay-reset", ());
        let _ = w.show();
    }
}

/// Hides the overlay and returns the circled region as PNG, if any.
fn finish_overlay(app: &AppHandle) -> Option<Vec<u8>> {
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.hide();
    }
    let st = app.state::<AppState>();
    let shot = st.shot.lock().unwrap().take()?;
    let [x0, y0, x1, y1] = st.circle.lock().unwrap().take()?;
    let (w, h) = (shot.width() as f64, shot.height() as f64);
    let (x0, y0) = ((x0 - 12.0).max(0.0), (y0 - 12.0).max(0.0));
    let (x1, y1) = ((x1 + 12.0).min(w), (y1 + 12.0).min(h));
    if x1 - x0 < 16.0 || y1 - y0 < 16.0 {
        return None;
    }
    let mut img = image::DynamicImage::ImageRgba8(shot).crop_imm(x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32);
    if img.width().max(img.height()) > 1600 {
        img = img.thumbnail(1600, 1600);
    }
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

// ---------- hotkey handlers ----------

fn on_hold(app: &AppHandle, event: hotkeys::Hold) {
    use hotkeys::Hold;
    match event {
        Hold::Press => start_mic(app),
        Hold::Confirm(mode) => on_start(app, mode),
        Hold::Cancel => {
            // Not push-to-talk after all: throw the audio away.
            if let Some(rec) = app.state::<AppState>().rec.lock().unwrap().take() {
                let _ = rec.stop();
            }
        }
        Hold::Release(mode) => on_stop(app, mode),
    }
}

/// Opens the mic as soon as the keys go down, so words spoken right away aren't clipped.
fn start_mic(app: &AppHandle) {
    let st = app.state::<AppState>();
    if st.cfg.lock().unwrap().api_key.is_empty() || st.rec.lock().unwrap().is_some() {
        return;
    }
    let a = app.clone();
    match audio::start(move |level| {
        let _ = a.emit("level", level);
    }) {
        Ok(rec) => *st.rec.lock().unwrap() = Some(rec),
        Err(e) => island(app, "error", &e),
    }
}

fn on_start(app: &AppHandle, mode: Mode) {
    let st = app.state::<AppState>();
    if st.cfg.lock().unwrap().api_key.is_empty() {
        island(app, "error", "Isi Gemini API key dulu di Settings.");
        open_settings(app);
        return;
    }
    if st.rec.lock().unwrap().is_none() {
        return; // mic failed to open; the error is already showing
    }
    *st.mode.lock().unwrap() = Some(mode);
    let combo = {
        let cfg = st.cfg.lock().unwrap();
        if mode == Mode::Dictation { cfg.dictation_hotkey.clone() } else { cfg.command_hotkey.clone() }
    };
    if combo.iter().any(|k| k == "win" || k == "windows") {
        input::suppress_start_menu();
    }
    match mode {
        Mode::Command => {
            show_overlay(app);
            island(app, "listening", "Mendengarkan… lingkari layar kalau perlu");
        }
        Mode::Dictation => island(app, "listening", "Mendengarkan…"),
    }
}

fn on_stop(app: &AppHandle, mode: Mode) {
    let st = app.state::<AppState>();
    let wav = st.rec.lock().unwrap().take().and_then(|r| r.stop());
    if st.mode.lock().unwrap().take() != Some(mode) {
        return;
    }
    let app = app.clone();
    match mode {
        Mode::Dictation => {
            thread::spawn(move || dictate(&app, wav));
        }
        Mode::Command => {
            let image = finish_overlay(&app);
            if wav.is_none() && image.is_none() {
                return island(&app, "hide", "");
            }
            let text = wav.is_none().then(|| "Explain what is in this image.".to_string());
            thread::spawn(move || command(&app, wav, image, text));
        }
    }
}

fn dictate(app: &AppHandle, wav: Option<Vec<u8>>) {
    let Some(wav) = wav else { return island(app, "hide", "") };
    island(app, "thinking", "Merapikan…");
    match gemini(app).dictate(&wav) {
        Ok(text) => {
            if text.is_empty() {
                return island(app, "error", "Gemini tidak menangkap suara apa pun. Coba lebih dekat ke mikrofon.");
            }
            island(app, "hide", "");
            hotkeys::wait_modifiers_released();
            if let Err(e) = input::paste(&text) {
                config::log(&format!("paste failed: {e}"));
                island(app, "error", &e);
            }
        }
        Err(e) => {
            config::log(&format!("dictation error: {e}"));
            island(app, "error", &e);
        }
    }
}

fn command(app: &AppHandle, wav: Option<Vec<u8>>, image: Option<Vec<u8>>, text: Option<String>) {
    // Transcribe first, so the user can see what was heard and the agent works from clean text.
    let text = match wav {
        Some(w) => {
            island(app, "thinking", "Menyimak…");
            match gemini(app).transcribe(&w, &transcription_hints(app)) {
                Ok(t) if !t.is_empty() => t,
                Ok(_) => return island(app, "error", "Suaranya nggak kedengeran. Coba lebih dekat ke mikrofon."),
                Err(e) => return island(app, "error", &e),
            }
        }
        None => text.unwrap_or_default(),
    };
    let mut parts = Vec::new();
    if let Some(img) = &image {
        parts.push(gemini::inline("image/png", img));
    }
    parts.push(json!({ "text": text }));
    let label = if image.is_some() { format!("\u{1F4F7} {text}") } else { format!("\u{1F3A4} {text}") };
    run_agent(app, parts, &label, true, true);
}

/// Names the user is likely to say: their most used apps and their last few requests.
fn transcription_hints(app: &AppHandle) -> String {
    let apps: Vec<String> = tools::app_list().into_iter().take(30).collect();
    let history = app.state::<AppState>().history.lock().unwrap().clone();
    let recent: Vec<String> = history
        .iter()
        .rev()
        .filter(|c| c["role"] == "user")
        .filter_map(|c| c["parts"].as_array()?.iter().find_map(|p| p["text"].as_str()).map(str::to_string))
        .take(3)
        .collect();
    format!("Apps on this PC: {}\nRecent requests: {}", apps.join(", "), recent.join(" | "))
}

/// Runs one agent turn and reports it to both the chat panel and the notch.
/// `spoken`: the request came by voice (or a schedule), so the reply is spoken if a voice is set.
fn run_agent(app: &AppHandle, parts: Vec<Value>, label: &str, remember: bool, spoken: bool) {
    let _ = app.emit("chat", json!({ "kind": "user", "text": label }));
    island(app, "thinking", "Berpikir…");
    let st = app.state::<AppState>();
    let _turn = st.agent_lock.lock().unwrap();
    whatsapp::TURN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let cfg = st.cfg.lock().unwrap().clone();
    let mut history = if remember { st.history.lock().unwrap().clone() } else { Vec::new() };
    let result = gemini(app).command(&mut history, parts, &cfg.personality, &cfg.abilities, |name, args| {
        let _ = app.emit("chat", json!({ "kind": "tool", "name": name, "args": args }));
        // Typing and shortcuts must land in the user's app, not in the island.
        if matches!(name, "press_keys" | "type_text") {
            system::restore_focus();
            thread::sleep(Duration::from_millis(150));
        }
        let out = tools::execute(name, args);
        let _ = app.emit("tasks-changed", ());
        out
    });
    match result {
        Ok(reply) => {
            if remember {
                *st.history.lock().unwrap() = history;
            }
            let _ = app.emit("chat", json!({ "kind": "assistant", "text": reply }));
            island(app, "done", &reply);
            if whatsapp::is_pending() {
                let _ = app.emit("confirm-send", ()); // chat shows Kirim / Batal buttons
            }
            if !cfg.voice.is_empty() && (spoken || cfg.speak_chat) {
                let app = app.clone();
                thread::spawn(move || {
                    if let Ok(wav) = gemini::speak(&cfg.api_key, &cfg.voice, &cfg.personality, &reply) {
                        let _ = app.emit_to("island", "speak", wav_url(&wav));
                    }
                });
            }
        }
        Err(e) => {
            let _ = app.emit("chat", json!({ "kind": "error", "text": e }));
            island(app, "error", &e);
        }
    }
}

/// When a newer Shiki.exe is put where this one was started from (a running exe can be renamed
/// aside and replaced), restart into it as soon as Shiki is idle.
fn watch_for_update(app: AppHandle) {
    let Ok(exe) = std::env::current_exe() else { return };
    let modified = |p: &std::path::Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let Some(started_as) = modified(&exe) else { return };
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(5));
        let Some(now) = modified(&exe) else { continue };
        if now <= started_as {
            continue;
        }
        thread::sleep(Duration::from_secs(2)); // let the copy finish
        let st = app.state::<AppState>();
        let idle = st.mode.lock().unwrap().is_none() && st.agent_lock.try_lock().is_ok();
        if idle && std::process::Command::new(&exe).spawn().is_ok() {
            app.exit(0);
            return;
        }
    });
}

/// Island window size in logical px; the panel inside is at most this big.
const ISLAND_SIZE: (f64, f64) = (820.0, 560.0);

/// Sizes the island for the primary monitor's scale and centres it on the top edge.
/// Moving a window onto a monitor with a different scale makes Windows resize it,
/// so move first, then size, then centre with the final size.
fn place_island(w: &tauri::WebviewWindow) {
    let Ok(Some(m)) = w.primary_monitor() else { return };
    let (mx, my, mw) = (m.position().x, m.position().y, m.size().width as i32);
    let _ = w.set_position(PhysicalPosition::new(mx + mw / 4, my));
    let scale = m.scale_factor();
    let size = tauri::PhysicalSize::new((ISLAND_SIZE.0 * scale).round() as u32, (ISLAND_SIZE.1 * scale).round() as u32);
    let _ = w.set_size(size);
    let width = w.outer_size().map(|s| s.width as i32).unwrap_or(size.width as i32);
    let _ = w.set_position(PhysicalPosition::new(mx + (mw - width) / 2, my));
}

/// Keeps the island click-through except over its visible content, and tells it when a fullscreen app is up.
fn track_island(app: AppHandle) {
    thread::spawn(move || {
        let (mut ignoring, mut fullscreen, mut tick) = (true, false, 0u32);
        let mut last_cursor = (i32::MIN, i32::MIN);
        loop {
            if let Some(w) = app.get_webview_window("island") {
                let [x, y, wd, ht] = *app.state::<AppState>().hit.lock().unwrap();
                if let Ok(pos) = w.inner_position() {
                    let (cx, cy) = system::cursor_pos();
                    let (rx, ry) = ((cx - pos.x) as f64, (cy - pos.y) as f64);
                    // Shiki's eyes follow the cursor; only send when it actually moved.
                    if !fullscreen && ((cx - last_cursor.0).abs() > 2 || (cy - last_cursor.1).abs() > 2) {
                        last_cursor = (cx, cy);
                        let _ = app.emit_to("island", "cursor", (rx, ry));
                    }
                    let inside = rx >= x && ry >= y && rx <= x + wd && ry <= y + ht;
                    if inside == ignoring {
                        ignoring = !inside;
                        let _ = w.set_ignore_cursor_events(ignoring);
                    }
                }
                if tick % 4 == 0 {
                    let fs = system::track_foreground();
                    if fs != fullscreen {
                        fullscreen = fs;
                        let _ = app.emit("fullscreen", fs);
                    }
                }
            }
            tick = tick.wrapping_add(1);
            thread::sleep(Duration::from_millis(40));
        }
    });
}

fn run_scheduler(app: AppHandle) {
    thread::spawn(move || loop {
        let (commands, reminders) = store::take_due();
        for cmd in commands {
            let app = app.clone();
            thread::spawn(move || {
                if app.state::<AppState>().cfg.lock().unwrap().api_key.is_empty() {
                    return island(&app, "error", &format!("Jadwal '{cmd}' gagal: API key belum diisi."));
                }
                let text = format!("(Scheduled task; the user may not be at the keyboard.) {cmd}");
                run_agent(&app, vec![json!({ "text": text })], &format!("⏰ {cmd}"), false, true);
            });
        }
        for r in reminders {
            island(&app, "done", &format!("Pengingat: {r}"));
            thread::sleep(Duration::from_secs(4));
        }
        thread::sleep(Duration::from_secs(15));
    });
}

// ---------- commands for the web UI ----------

#[tauri::command]
fn set_circle(state: tauri::State<AppState>, bbox: [f64; 4]) {
    *state.circle.lock().unwrap() = Some(bbox);
}

#[tauri::command]
fn get_config(state: tauri::State<AppState>) -> Config {
    state.cfg.lock().unwrap().clone()
}

#[tauri::command]
fn save_config(app: AppHandle, state: tauri::State<AppState>, cfg: Config) {
    config::save(&cfg);
    let _ = app.emit("config-changed", &cfg); // e.g. the island switches mask
    *state.cfg.lock().unwrap() = cfg;
}

fn wav_url(wav: &[u8]) -> String {
    use base64::Engine;
    format!("data:audio/wav;base64,{}", base64::engine::general_purpose::STANDARD.encode(wav))
}

/// Loadout › Voice: a sample line in `voice` with the current personality, as a playable data URL.
#[tauri::command]
async fn preview_voice(app: AppHandle, voice: String) -> Result<String, String> {
    let cfg = app.state::<AppState>().cfg.lock().unwrap().clone();
    if cfg.api_key.is_empty() {
        return Err("Isi Gemini API key dulu.".into());
    }
    let line = match cfg.personality.as_str() {
        "santai" => "Yo! Gw Shiki. Mau dibukain apa nih?",
        "sarkas" => "Oh, kamu butuh bantuan lagi? Tentu saja. Aku Shiki.",
        "formal" => "Selamat datang. Saya Shiki, siap membantu Anda.",
        "samurai" => "Hamba Shiki, siap melayani Tuan sampai akhir.",
        "iblis" => "Perjanjian telah dibuat. Aku Shiki, dan aku menunggu perintahmu.",
        _ => "Halo, aku Shiki. Ada yang bisa aku bantu?",
    };
    tauri::async_runtime::spawn_blocking(move || gemini::speak(&cfg.api_key, &voice, &cfg.personality, line))
        .await
        .map_err(|e| e.to_string())?
        .map(|wav| wav_url(&wav))
}

#[tauri::command]
fn get_todos() -> Vec<store::Todo> {
    store::todos()
}

#[tauri::command]
fn add_todo(text: String) -> Vec<store::Todo> {
    store::add_todo(&text, "");
    store::todos()
}

#[tauri::command]
fn complete_todo(index: usize) -> Vec<store::Todo> {
    store::complete_todo(index + 1);
    store::todos()
}

#[tauri::command]
fn get_tasks() -> Vec<store::Task> {
    store::tasks()
}

#[tauri::command]
fn cancel_task(index: usize) -> Vec<store::Task> {
    store::cancel_task(index + 1);
    store::tasks()
}

#[tauri::command]
fn open_url(url: String) {
    let _ = open::that_detached(url);
}

#[tauri::command]
fn set_hit_rect(state: tauri::State<AppState>, rect: [f64; 4]) {
    *state.hit.lock().unwrap() = rect;
}

// Commands that may create a window must be async: on Windows, building a window inside a
// synchronous command deadlocks the event loop and leaves a frozen window.
#[tauri::command]
async fn chat_send(app: AppHandle, text: String) {
    if app.state::<AppState>().cfg.lock().unwrap().api_key.is_empty() {
        let _ = app.emit("chat", json!({ "kind": "error", "text": "Isi Gemini API key dulu di Settings." }));
        return open_settings(&app);
    }
    thread::spawn(move || run_agent(&app, vec![json!({ "text": text })], &text, true, false));
}

#[tauri::command]
fn chat_reset(state: tauri::State<AppState>) {
    state.history.lock().unwrap().clear();
}

#[tauri::command]
fn restore_focus() {
    system::restore_focus();
}

#[tauri::command]
async fn show_settings(app: AppHandle) {
    open_settings(&app);
}

#[tauri::command]
async fn media_info() -> system::Media {
    tauri::async_runtime::spawn_blocking(system::media).await.unwrap_or_default()
}

#[tauri::command]
async fn media_action(action: String) {
    let _ = tauri::async_runtime::spawn_blocking(move || system::media_action(&action)).await;
}

#[tauri::command]
async fn media_seek(seconds: f64) {
    let _ = tauri::async_runtime::spawn_blocking(move || system::media_seek(seconds, "to")).await;
}

#[tauri::command]
fn get_stats() -> system::Stats {
    system::stats()
}

#[tauri::command]
fn clipboard_list() -> Vec<String> {
    system::clips()
}

#[tauri::command]
fn clipboard_clear() {
    system::clear_clips();
}

/// Puts `text` back into the app the user was in.
#[tauri::command]
async fn clipboard_paste(text: String) {
    let _ = tauri::async_runtime::spawn_blocking(move || {
        system::restore_focus();
        thread::sleep(Duration::from_millis(150));
        let _ = input::paste(&text);
    })
    .await;
}

#[tauri::command]
async fn quick_keys(keys: String) {
    let _ = tauri::async_runtime::spawn_blocking(move || {
        system::restore_focus();
        thread::sleep(Duration::from_millis(150));
        let _ = input::press(&keys);
    })
    .await;
}

/// Global keys (volume, Win+D, …) that don't need the user's app focused.
#[tauri::command]
fn send_keys(keys: String) {
    let _ = input::press(&keys);
}

#[tauri::command]
async fn app_list() -> Vec<String> {
    tauri::async_runtime::spawn_blocking(tools::app_list).await.unwrap_or_default()
}

#[tauri::command]
async fn launch_app(name: String) -> String {
    tauri::async_runtime::spawn_blocking(move || tools::open_app(&name)).await.unwrap_or_default()
}

fn wallpaper_folder(app: &AppHandle) -> String {
    let folder = app.state::<AppState>().cfg.lock().unwrap().wallpaper_folder.clone();
    if folder.is_empty() { system::default_wallpaper_folder() } else { folder }
}

/// `source` is "recent" or "folder". Returns the folder in use alongside the pictures.
#[tauri::command]
async fn wallpapers(app: AppHandle, source: String) -> (String, Vec<system::Wallpaper>) {
    let folder = wallpaper_folder(&app);
    let f = folder.clone();
    let walls = tauri::async_runtime::spawn_blocking(move || system::wallpapers(&source, &f)).await.unwrap_or_default();
    (folder, walls)
}

#[tauri::command]
async fn pick_wallpaper_folder(app: AppHandle) -> Option<String> {
    let start = wallpaper_folder(&app);
    let picked = tauri::async_runtime::spawn_blocking(move || system::pick_folder(&start)).await.ok().flatten()?;
    let st = app.state::<AppState>();
    let mut cfg = st.cfg.lock().unwrap();
    cfg.wallpaper_folder = picked.clone();
    config::save(&cfg);
    Some(picked)
}

#[tauri::command]
async fn app_icons(names: Vec<String>) -> std::collections::HashMap<String, String> {
    tauri::async_runtime::spawn_blocking(move || system::app_icons(&tools::app_ids(&names))).await.unwrap_or_default()
}

/// The full-screen window that plays the circular wallpaper reveal on the primary monitor,
/// placed just above the desktop so the user's windows stay on top of it.
fn wallfx_window(app: &AppHandle) -> Option<tauri::WebviewWindow> {
    let w = match app.get_webview_window("wallfx") {
        Some(w) => w,
        None => WebviewWindowBuilder::new(app, "wallfx", WebviewUrl::App("wallfx.html".into()))
            .title("Shiki wallpaper")
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .skip_taskbar(true)
            .focused(false)
            .resizable(false)
            .visible(false)
            .additional_browser_args(BROWSER_ARGS)
            .build()
            .ok()?,
    };
    if let Ok(Some(m)) = w.primary_monitor() {
        let _ = w.set_position(*m.position());
        let _ = w.set_size(*m.size());
    }
    Some(w)
}

/// Changes the wallpaper with a reveal: a circle of the new picture grows from the centre
/// of the desktop, then the real wallpaper is swapped in underneath and the effect fades away.
#[tauri::command]
async fn set_wallpaper(app: AppHandle, path: String) -> bool {
    let _ = app.asset_protocol_scope().allow_file(&path);
    let fx = wallfx_window(&app);
    if let Some(w) = &fx {
        let _ = w.set_ignore_cursor_events(true);
        let _ = w.show();
        if let Ok(h) = w.hwnd() {
            system::send_to_bottom(h.0 as isize);
        }
        // Give a freshly created window a moment to load before starting the effect.
        tokio_sleep(250).await;
        let _ = w.emit("wallfx", &path);
        tokio_sleep(1100).await;
    }
    let p = path.clone();
    let ok = tauri::async_runtime::spawn_blocking(move || system::set_wallpaper(&p)).await.unwrap_or(false);
    if let Some(w) = fx {
        tokio_sleep(450).await; // let the desktop repaint with the real wallpaper first
        let _ = w.emit("wallfx-hide", ());
        tokio_sleep(320).await;
        let _ = w.hide();
    }
    ok
}

async fn tokio_sleep(ms: u64) {
    let _ = tauri::async_runtime::spawn_blocking(move || thread::sleep(Duration::from_millis(ms))).await;
}

#[tauri::command]
async fn get_controls() -> system::Controls {
    tauri::async_runtime::spawn_blocking(system::controls).await.unwrap_or_default()
}

#[tauri::command]
async fn set_volume(percent: u32) {
    let _ = tauri::async_runtime::spawn_blocking(move || system::set_volume(percent)).await;
}

#[tauri::command]
async fn set_muted(muted: bool) {
    let _ = tauri::async_runtime::spawn_blocking(move || system::set_muted(muted)).await;
}

#[tauri::command]
async fn set_dark_mode(dark: bool) {
    let _ = tauri::async_runtime::spawn_blocking(move || system::set_dark_mode(dark)).await;
}

#[tauri::command]
async fn set_radio(which: String, on: bool) -> bool {
    tauri::async_runtime::spawn_blocking(move || system::set_radio(&which, on)).await.unwrap_or(false)
}

#[tauri::command]
fn get_autostart() -> bool {
    system::autostart()
}

#[tauri::command]
fn set_autostart(on: bool) -> bool {
    system::set_autostart(on)
}

#[tauri::command]
fn favorite_apps() -> Vec<String> {
    system::favorite_apps()
}

#[tauri::command]
fn toggle_favorite(name: String) -> Vec<String> {
    system::toggle_favorite(&name)
}

#[tauri::command]
fn app_usage() -> std::collections::HashMap<String, u32> {
    system::app_usage()
}

#[tauri::command]
fn power(action: String) {
    system::power(&action);
}

fn main() {
    let state = AppState { cfg: Mutex::new(config::load()), ..Default::default() };

    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            set_circle, get_config, save_config, get_todos, add_todo, complete_todo, get_tasks, cancel_task, open_url,
            set_hit_rect, chat_send, chat_reset, restore_focus, show_settings, media_info, media_action, media_seek, get_stats,
            clipboard_list, clipboard_clear, clipboard_paste, quick_keys, send_keys, app_list, launch_app, wallpapers, set_wallpaper, power,
            pick_wallpaper_folder, app_icons, get_controls, set_volume, set_muted, set_dark_mode, set_radio,
            favorite_apps, toggle_favorite, app_usage, preview_voice, get_autostart, set_autostart
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            // Island: hugs the top edge of the primary monitor, click-through outside its content.
            if let Some(w) = app.get_webview_window("island") {
                place_island(&w);
                w.set_ignore_cursor_events(true)?;
                w.show()?;
            }
            track_island(handle.clone());
            system::watch_clipboard();

            let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Keluar", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&settings, &quit])?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("Shiki")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, e| match e.id.as_ref() {
                    "settings" => open_settings(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, e| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                        open_settings(tray.app_handle());
                    }
                })
                .build(app)?;

            let (h1, h2) = (handle.clone(), handle.clone());
            hotkeys::spawn(
                move || {
                    let cfg = h1.state::<AppState>().cfg.lock().unwrap().clone();
                    vec![(Mode::Dictation, cfg.dictation_hotkey), (Mode::Command, cfg.command_hotkey)]
                },
                move |event| on_hold(&h2, event),
            );
            run_scheduler(handle.clone());
            watch_for_update(handle.clone());
            thread::spawn(tools::app_list); // warm the app list so the first voice command isn't slowed by it

            let has_key = !app.state::<AppState>().cfg.lock().unwrap().api_key.is_empty();
            if has_key {
                thread::spawn(move || {
                    thread::sleep(Duration::from_millis(1500));
                    island(&handle, "done", "Shiki siap. Tahan Ctrl+Win untuk dikte, Ctrl+Shift untuk perintah.");
                });
            } else {
                open_settings(&handle);
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // The island and overlay live for the whole session; settings may close for real.
            WindowEvent::CloseRequested { api, .. } if window.label() != "settings" => {
                api.prevent_close();
                let _ = window.hide();
            }
            // Display scale changed (settings, monitor plugged in/out): re-fit and re-centre the island.
            WindowEvent::ScaleFactorChanged { .. } if window.label() == "island" => {
                if let Some(w) = window.app_handle().get_webview_window("island") {
                    let w2 = w.clone();
                    let _ = w.run_on_main_thread(move || place_island(&w2));
                }
            }
            _ => {}
        })
        .build(tauri::generate_context!())
        .expect("error while building Shiki")
        .run(|_, event| {
            // Keep running in the tray when windows are hidden; only "Keluar" exits.
            if let RunEvent::ExitRequested { api, code: None, .. } = event {
                api.prevent_exit();
            }
        });
}
