//! Windows integrations behind the island panel: media, stats, clipboard history,
//! wallpapers, power, and focus/fullscreen tracking.
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::Serialize;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicIsize, Ordering},
        Mutex,
    },
    thread,
    time::Duration,
};
use windows::core::w;
use windows::Win32::{
    Foundation::{FILETIME, HWND, POINT, RECT},
    Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST},
    System::{
        DataExchange::{GetClipboardSequenceNumber, IsClipboardFormatAvailable, RegisterClipboardFormatW},
        Power::{GetSystemPowerStatus, SetSuspendState, SYSTEM_POWER_STATUS},
        Shutdown::LockWorkStation,
        SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX},
        Threading::GetSystemTimes,
    },
    UI::WindowsAndMessaging::{
        GetClassNameW, GetCursorPos, GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId, SetForegroundWindow,
        SystemParametersInfoW, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SPI_GETDESKWALLPAPER, SPI_SETDESKWALLPAPER,
    },
};

// ---------- focus & fullscreen ----------

/// Last foreground window that isn't ours, so we can hand focus back after the panel closes.
static LAST_FOREGROUND: AtomicIsize = AtomicIsize::new(0);

fn is_ours(hwnd: HWND) -> bool {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid == std::process::id()
}

pub fn cursor_pos() -> (i32, i32) {
    let mut p = POINT::default();
    let _ = unsafe { GetCursorPos(&mut p) };
    (p.x, p.y)
}

/// Remembers the user's current window (if it isn't ours) and reports whether it covers its whole monitor.
pub fn track_foreground() -> bool {
    let fg = unsafe { GetForegroundWindow() };
    if fg.is_invalid() || is_ours(fg) {
        return false;
    }
    LAST_FOREGROUND.store(fg.0 as isize, Ordering::Relaxed);

    let mut class = [0u16; 64];
    let n = unsafe { GetClassNameW(fg, &mut class) } as usize;
    let class = String::from_utf16_lossy(&class[..n]);
    if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd") {
        return false;
    }
    let mut r = RECT::default();
    let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    unsafe {
        if GetWindowRect(fg, &mut r).is_err() || !GetMonitorInfoW(MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST), &mut mi).as_bool() {
            return false;
        }
    }
    let m = mi.rcMonitor;
    r.left <= m.left && r.top <= m.top && r.right >= m.right && r.bottom >= m.bottom
}

unsafe extern "system" fn collect_youtube(hwnd: HWND, lparam: windows::Win32::Foundation::LPARAM) -> windows::core::BOOL {
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowTextW, IsWindowVisible};
    let out = &mut *(lparam.0 as *mut Vec<HWND>);
    if IsWindowVisible(hwnd).as_bool() && !is_ours(hwnd) {
        let mut buf = [0u16; 512];
        let n = GetWindowTextW(hwnd, &mut buf).max(0) as usize;
        // Browsers title the window after the active tab: "<video> - YouTube - Brave".
        if String::from_utf16_lossy(&buf[..n]).contains("- YouTube") {
            out.push(hwnd);
        }
    }
    windows::core::BOOL(1)
}

/// Loads `url` in the browser tab that's showing YouTube, instead of piling up new tabs.
/// Returns false if there's no such tab (or it couldn't be focused), so the caller can open a new one.
/// Borrows the browser window whose active tab is YouTube to type into it, without the user seeing it:
/// the window is made fully transparent, focused, `act` runs, and then everything is put back —
/// minimized again if it was minimized, otherwise back behind the window the user was in.
/// Returns false if there's no YouTube tab or it couldn't be focused.
fn with_youtube_tab(act: impl FnOnce() -> bool) -> bool {
    use windows::Win32::Foundation::COLORREF;
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowLongPtrW, IsIconic, SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE,
        LWA_ALPHA, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_MINIMIZE, SW_RESTORE, WS_EX_LAYERED,
    };
    let mut wins: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(collect_youtube), windows::Win32::Foundation::LPARAM(&mut wins as *mut _ as isize));
    }
    let Some(&hwnd) = wins.first() else { return false };
    unsafe {
        let before = GetForegroundWindow();
        if before == hwnd {
            return act(); // the user is already looking at it
        }
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let was_layered = ex & WS_EX_LAYERED.0 as isize != 0;
        let was_min = IsIconic(hwnd).as_bool();

        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_LAYERED.0 as isize);
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 0, LWA_ALPHA);
        if was_min {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        // Windows only lets the app that had the last input take focus; a harmless key press gives us that.
        crate::input::suppress_start_menu();
        let _ = SetForegroundWindow(hwnd);
        thread::sleep(Duration::from_millis(150));
        let focused = GetForegroundWindow() == hwnd;
        let ok = focused && act();
        thread::sleep(Duration::from_millis(120));

        if was_min {
            let _ = ShowWindow(hwnd, SW_MINIMIZE);
        } else if !before.is_invalid() && before != hwnd {
            let _ = SetWindowPos(hwnd, Some(before), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
        if !before.is_invalid() && before != hwnd {
            crate::input::suppress_start_menu();
            let _ = SetForegroundWindow(before);
        }
        thread::sleep(Duration::from_millis(80)); // let the minimize finish while still invisible
        if was_layered {
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
        } else {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex);
        }
        ok
    }
}

/// Presses a YouTube keyboard shortcut in the YouTube tab, invisibly.
fn youtube_shortcut(keys: &str) -> bool {
    with_youtube_tab(|| crate::input::press(keys).is_ok())
}

/// Loads `url` in the YouTube tab instead of piling up new tabs, invisibly.
pub fn navigate_youtube_tab(url: &str) -> bool {
    with_youtube_tab(|| {
        let _ = crate::input::press("ctrl+l");
        thread::sleep(Duration::from_millis(100));
        if crate::input::paste(url).is_err() {
            return false;
        }
        crate::input::press("enter").is_ok()
    })
}

pub fn restore_focus() {
    let h = LAST_FOREGROUND.load(Ordering::Relaxed);
    if h != 0 {
        unsafe {
            let _ = SetForegroundWindow(HWND(h as *mut _));
        }
    }
}

// ---------- media ----------

#[derive(Serialize, Default, Clone)]
pub struct Media {
    pub has: bool,
    pub title: String,
    pub artist: String,
    pub playing: bool,
    pub thumb: Option<String>,
    /// Seconds.
    pub position: f64,
    pub duration: f64,
    pub can_seek: bool,
}

type Session = windows::Media::Control::GlobalSystemMediaTransportControlsSession;

fn is_playing(s: &Session) -> bool {
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status;
    s.GetPlaybackInfo().and_then(|i| i.PlaybackStatus()).is_ok_and(|st| st == Status::Playing)
}

/// (position, duration) in seconds. Players only report position on changes, so extrapolate while playing.
fn timeline(s: &Session) -> windows::core::Result<(f64, f64)> {
    let tl = s.GetTimelineProperties()?;
    let mut pos = tl.Position()?.Duration as f64 / 1e7;
    let end = tl.EndTime()?.Duration as f64 / 1e7;
    if is_playing(s) {
        let updated = tl.LastUpdatedTime()?.UniversalTime;
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        let now = now.as_secs() as i64 * 10_000_000 + now.subsec_nanos() as i64 / 100 + 116_444_736_000_000_000;
        if updated > 0 {
            pos += ((now - updated) as f64 / 1e7).max(0.0);
        }
    }
    Ok((if end > 0.0 { pos.min(end) } else { pos }, end))
}

fn clock(secs: f64) -> String {
    let s = secs.max(0.0).round() as u64;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) }
}

/// When something was last started, so a seek right after it can wait for the new video to load.
static NEW_MEDIA_AT: Mutex<Option<std::time::Instant>> = Mutex::new(None);

pub fn mark_new_media() {
    *NEW_MEDIA_AT.lock().unwrap() = Some(std::time::Instant::now());
}

/// If something was just started, wait (up to 10 s) until it is actually playing with a known length.
fn wait_for_new_media() {
    let started = NEW_MEDIA_AT.lock().unwrap().take();
    let Some(started) = started.filter(|t| t.elapsed() < Duration::from_secs(20)) else { return };
    while started.elapsed() < Duration::from_secs(10) {
        if let Ok(s) = session() {
            if is_playing(&s) && timeline(&s).is_ok_and(|(_, end)| end > 0.0) {
                thread::sleep(Duration::from_millis(700)); // let the player settle before seeking
                return;
            }
        }
        thread::sleep(Duration::from_millis(300));
    }
}

/// Jump to `seconds` ("to"), or move by it ("forward" / "back").
pub fn media_seek(seconds: f64, mode: &str) -> String {
    wait_for_new_media();
    let run = || -> windows::core::Result<String> {
        let s = session()?;
        let title = s.TryGetMediaPropertiesAsync()?.join()?.Title()?.to_string();
        let (pos, end) = timeline(&s)?;
        let mut target = match mode {
            "forward" => pos + seconds,
            "back" => pos - seconds,
            _ => seconds,
        };
        target = target.max(0.0);
        if end > 0.0 {
            target = target.min(end - 1.0);
        }
        // Players can accept the request and still ignore it, so check where we landed and retry once.
        for _ in 0..2 {
            if !s.TryChangePlaybackPositionAsync((target * 1e7) as i64)?.join()? {
                return Ok(format!("'{title}' doesn't allow jumping to a position."));
            }
            thread::sleep(Duration::from_millis(600));
            if timeline(&s).is_ok_and(|(now, _)| (now - target).abs() < 5.0) {
                return Ok(format!("Jumped to {} of '{title}' (length {}).", clock(target), clock(end)));
            }
        }
        Ok(format!("Tried to jump to {} in '{title}', but the player didn't move. It may still be loading; ask again in a moment.", clock(target)))
    };
    run().unwrap_or_else(|_| "Nothing is playing right now.".into())
}

pub fn now_playing() -> String {
    let run = || -> windows::core::Result<String> {
        let s = session()?;
        let p = s.TryGetMediaPropertiesAsync()?.join()?;
        let (pos, end) = timeline(&s)?;
        let state = if is_playing(&s) { "playing" } else { "paused" };
        Ok(format!("'{}' by '{}' is {state} at {} of {}.", p.Title()?, p.Artist()?, clock(pos), clock(end)))
    };
    run().unwrap_or_else(|_| "Nothing is playing right now.".into())
}

static THUMB_CACHE: Mutex<Option<(String, Option<String>)>> = Mutex::new(None);

fn read_thumb(r: &windows::Storage::Streams::IRandomAccessStreamReference) -> windows::core::Result<String> {
    use windows::Storage::Streams::DataReader;
    let s = r.OpenReadAsync()?.join()?;
    let size = s.Size()? as u32;
    let reader = DataReader::CreateDataReader(&s.GetInputStreamAt(0)?)?;
    reader.LoadAsync(size)?.join()?;
    let mut buf = vec![0u8; size as usize];
    reader.ReadBytes(&mut buf)?;
    let mime = s.ContentType().map(|m| m.to_string()).unwrap_or_else(|_| "image/png".into());
    Ok(format!("data:{mime};base64,{}", B64.encode(buf)))
}

fn manager() -> windows::core::Result<windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager> {
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager as Manager;
    Manager::RequestAsync()?.join()
}

/// The session that's actually playing (there can be several: browser tabs, Spotify, …),
/// falling back to the one Windows considers current.
fn session() -> windows::core::Result<Session> {
    let m = manager()?;
    if let Some(s) = m.GetSessions()?.into_iter().find(is_playing) {
        return Ok(s);
    }
    m.GetCurrentSession()
}

/// Pauses everything that's playing, so starting something new doesn't play on top of it.
pub fn pause_all() {
    let run = || -> windows::core::Result<()> {
        for s in manager()?.GetSessions()? {
            if is_playing(&s) {
                let _ = s.TryPauseAsync()?.join();
            }
        }
        Ok(())
    };
    let _ = run();
}

pub fn media() -> Media {
    let get = || -> windows::core::Result<Media> {
        let s = session()?;
        let p = s.TryGetMediaPropertiesAsync()?.join()?;
        let title = p.Title()?.to_string();
        let artist = p.Artist()?.to_string();
        let playing = is_playing(&s);
        let (position, duration) = timeline(&s).unwrap_or_default();
        let can_seek = s.GetPlaybackInfo()?.Controls()?.IsPlaybackPositionEnabled()?;
        let key = format!("{title}\u{1}{artist}");
        let mut cache = THUMB_CACHE.lock().unwrap();
        let thumb = match cache.as_ref() {
            Some((k, t)) if *k == key => t.clone(),
            _ => {
                let t = p.Thumbnail().ok().and_then(|r| read_thumb(&r).ok());
                *cache = Some((key, t.clone()));
                t
            }
        };
        Ok(Media { has: !title.is_empty(), title, artist, playing, thumb, position, duration, can_seek })
    };
    get().unwrap_or_default()
}

pub fn media_action(action: &str) {
    let run = || -> windows::core::Result<()> {
        let s = session()?;
        let controls = s.GetPlaybackInfo()?.Controls()?;
        // A single YouTube video (not a playlist) tells Windows it has no next/previous. The media keys
        // (what a touchpad gesture sends) still reach the browser without bringing it forward, so try
        // those first; if the track didn't change, use YouTube's "next video" shortcut / restart the track.
        let title = || s.TryGetMediaPropertiesAsync().and_then(|op| op.join()).and_then(|p| p.Title()).map(|t| t.to_string()).unwrap_or_default();
        let changed_after_key = |key: &str| {
            let before = title();
            let _ = crate::input::press(key);
            (0..10).any(|_| {
                thread::sleep(Duration::from_millis(200));
                session().ok().and_then(|n| n.TryGetMediaPropertiesAsync().ok()?.join().ok()?.Title().ok()).is_some_and(|t| t.to_string() != before)
            })
        };
        let op = match action {
            "next" if !controls.IsNextEnabled()? => {
                if !changed_after_key("next track") {
                    youtube_shortcut("shift+n");
                }
                return Ok(());
            }
            "prev" | "previous" if !controls.IsPreviousEnabled()? => {
                if changed_after_key("previous track") {
                    return Ok(());
                }
                s.TryChangePlaybackPositionAsync(0)?
            }
            "next" => s.TrySkipNextAsync()?,
            "prev" | "previous" => s.TrySkipPreviousAsync()?,
            "play" => s.TryPlayAsync()?,
            "pause" => s.TryPauseAsync()?,
            _ => s.TryTogglePlayPauseAsync()?,
        };
        op.join()?;
        Ok(())
    };
    let _ = run();
}

// ---------- stats ----------

#[derive(Serialize)]
pub struct Stats {
    pub cpu: f32,
    pub ram_used_gb: f32,
    pub ram_total_gb: f32,
    pub battery: Option<u8>,
    pub charging: bool,
}

static PREV_TIMES: Mutex<Option<(u64, u64)>> = Mutex::new(None);

pub fn stats() -> Stats {
    let ft = |t: FILETIME| ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64;
    let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
    let _ = unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) };
    let (idle, total) = (ft(idle), ft(kernel) + ft(user)); // kernel time includes idle
    let mut prev = PREV_TIMES.lock().unwrap();
    let cpu = match *prev {
        Some((pi, pt)) if total > pt => 1.0 - (idle - pi) as f32 / (total - pt) as f32,
        _ => 0.0,
    };
    *prev = Some((idle, total));

    let mut mem = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    let _ = unsafe { GlobalMemoryStatusEx(&mut mem) };
    let gb = |b: u64| b as f32 / 1024f32.powi(3);

    let mut p = SYSTEM_POWER_STATUS::default();
    let _ = unsafe { GetSystemPowerStatus(&mut p) };
    Stats {
        cpu: (cpu * 100.0).clamp(0.0, 100.0),
        ram_used_gb: gb(mem.ullTotalPhys - mem.ullAvailPhys),
        ram_total_gb: gb(mem.ullTotalPhys),
        battery: (p.BatteryLifePercent != 255).then_some(p.BatteryLifePercent),
        charging: p.ACLineStatus == 1,
    }
}

// ---------- clipboard history (in memory only) ----------

static CLIPS: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub fn watch_clipboard() {
    thread::spawn(|| {
        let mut last = 0u32;
        // Password managers set this format so clipboard monitors skip their content.
        let exclude = unsafe { RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing")) };
        loop {
            let seq = unsafe { GetClipboardSequenceNumber() };
            if seq != last {
                last = seq;
                let excluded = unsafe { IsClipboardFormatAvailable(exclude) }.is_ok();
                // Only open the clipboard for text. Leaving images alone avoids getting in the way of
                // apps like Snipping Tool while they are still putting a screenshot there.
                let has_text = unsafe { IsClipboardFormatAvailable(13 /* CF_UNICODETEXT */) }.is_ok();
                if !excluded && has_text {
                    if let Ok(text) = arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
                        let text = text.trim().to_string();
                        if !text.is_empty() && text.len() < 20_000 {
                            let mut clips = CLIPS.lock().unwrap();
                            clips.retain(|c| *c != text);
                            clips.insert(0, text);
                            clips.truncate(30);
                        }
                    }
                }
            }
            thread::sleep(Duration::from_millis(600));
        }
    });
}

pub fn clips() -> Vec<String> {
    CLIPS.lock().unwrap().clone()
}

pub fn clear_clips() {
    CLIPS.lock().unwrap().clear();
}

// ---------- wallpapers ----------

#[derive(Serialize)]
pub struct Wallpaper {
    pub path: String,
    pub name: String,
    pub thumb: String,
    pub current: bool,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn current_wallpaper() -> String {
    let mut buf = [0u16; 260];
    let _ = unsafe {
        SystemParametersInfoW(SPI_GETDESKWALLPAPER, buf.len() as u32, Some(buf.as_mut_ptr() as *mut _), Default::default())
    };
    let n = buf.iter().position(|&c| c == 0).unwrap_or(0);
    String::from_utf16_lossy(&buf[..n])
}

fn history_file() -> PathBuf {
    crate::config::data_dir().join("wallpapers.json")
}

/// Sets the wallpaper on every monitor. IDesktopWallpaper is what Windows' own slideshow uses,
/// so the change fades in; the older SystemParametersInfo call (instant) is the fallback.
pub fn set_wallpaper(path: &str) -> bool {
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{DesktopWallpaper, IDesktopWallpaper};
    let faded = unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        CoCreateInstance::<_, IDesktopWallpaper>(&DesktopWallpaper, None, CLSCTX_ALL)
            .and_then(|dw| dw.SetWallpaper(windows::core::PCWSTR::null(), &windows::core::HSTRING::from(path)))
            .is_ok()
    };
    let ok = faded || {
        let mut p = wide(path);
        unsafe { SystemParametersInfoW(SPI_SETDESKWALLPAPER, 0, Some(p.as_mut_ptr() as *mut _), SPIF_UPDATEINIFILE | SPIF_SENDCHANGE).is_ok() }
    };
    if ok {
        let mut recent: Vec<String> = std::fs::read_to_string(history_file()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        recent.retain(|p| !p.eq_ignore_ascii_case(path));
        recent.insert(0, path.to_string());
        recent.truncate(20);
        if let Ok(s) = serde_json::to_string(&recent) {
            let _ = std::fs::write(history_file(), s);
        }
    }
    ok
}

fn is_image(p: &Path) -> bool {
    p.extension().and_then(|x| x.to_str()).is_some_and(|x| matches!(x.to_lowercase().as_str(), "jpg" | "jpeg" | "png"))
}

fn image_files(dir: &Path, depth: u8, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            if depth > 0 {
                image_files(&p, depth - 1, out);
            }
        } else if is_image(&p) {
            out.push(p);
        }
    }
}

pub fn default_wallpaper_folder() -> String {
    Path::new(&std::env::var("USERPROFILE").unwrap_or_default()).join("Pictures").display().to_string()
}

/// "recent": wallpapers set through Shiki, the current one and Windows' built-ins.
/// "folder": pictures in `folder` (and its direct subfolders).
pub fn wallpapers(source: &str, folder: &str) -> Vec<Wallpaper> {
    let current = current_wallpaper();
    let mut files: Vec<PathBuf> = Vec::new();
    if source == "folder" {
        image_files(Path::new(folder), 1, &mut files);
    } else {
        let recent: Vec<String> = std::fs::read_to_string(history_file()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        files.extend(recent.iter().map(PathBuf::from));
        if !current.is_empty() {
            files.push(PathBuf::from(&current));
        }
        image_files(Path::new(r"C:\Windows\Web\Wallpaper"), 1, &mut files);
    }
    let mut seen = std::collections::HashSet::new();
    files.retain(|p| p.exists() && seen.insert(p.display().to_string().to_lowercase()));
    files.truncate(40);

    let cache = crate::config::data_dir().join("thumbs");
    let _ = std::fs::create_dir_all(&cache);
    let current = current.to_lowercase();
    files
        .iter()
        .filter_map(|p| {
            let meta = std::fs::metadata(p).ok()?;
            let stamp = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
            let thumb_path = cache.join(format!("{:x}-w.jpg", fxhash(&format!("{}{stamp}", p.display()))));
            if !thumb_path.exists() {
                let img = image::open(p).ok()?.thumbnail(560, 560).into_rgb8();
                img.save_with_format(&thumb_path, image::ImageFormat::Jpeg).ok()?;
            }
            let bytes = std::fs::read(&thumb_path).ok()?;
            let path = p.display().to_string();
            Some(Wallpaper {
                current: path.to_lowercase() == current,
                name: p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
                thumb: format!("data:image/jpeg;base64,{}", B64.encode(bytes)),
                path,
            })
        })
        .collect()
}

fn fxhash(s: &str) -> u64 {
    s.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

/// Windows' folder picker. Blocks until the user chooses (or cancels).
pub fn pick_folder(start: &str) -> Option<String> {
    use windows::core::HSTRING;
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{
        FileOpenDialog, IFileOpenDialog, IShellItem, SHCreateItemFromParsingName, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
    };
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL).ok()?;
        let opts = dialog.GetOptions().ok()?;
        dialog.SetOptions(opts | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM).ok()?;
        let _ = dialog.SetTitle(&HSTRING::from("Pilih folder wallpaper"));
        if let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(&HSTRING::from(start), None) {
            let _ = dialog.SetFolder(&item);
        }
        dialog.Show(None).ok()?;
        let name = dialog.GetResult().ok()?.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = name.to_string().ok();
        CoTaskMemFree(Some(name.0 as *const _));
        path
    }
}

// ---------- app icons ----------

/// The app's icon as PNG, straight from the Windows shell (same icon the Start menu shows).
fn shell_icon_png(app_id: &str, size: i32) -> Option<Vec<u8>> {
    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY};
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let path = windows::core::HSTRING::from(format!("shell:AppsFolder\\{app_id}"));
        let factory: IShellItemImageFactory = SHCreateItemFromParsingName(&path, None).ok()?;
        let hbmp = factory.GetImage(SIZE { cx: size, cy: size }, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK).ok()?;
        let mut bmp = BITMAP::default();
        GetObjectW(hbmp.into(), std::mem::size_of::<BITMAP>() as i32, Some(&mut bmp as *mut _ as *mut _));
        let (w, h) = (bmp.bmWidth, bmp.bmHeight);
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // top-down rows
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; (w * h * 4) as usize];
        let dc = GetDC(None);
        let rows = GetDIBits(dc, hbmp, 0, h as u32, Some(buf.as_mut_ptr() as *mut _), &mut info, DIB_RGB_COLORS);
        ReleaseDC(None, dc);
        let _ = DeleteObject(hbmp.into());
        if rows == 0 || w <= 0 || h <= 0 {
            return None;
        }
        // BGRA, premultiplied → straight RGBA.
        for px in buf.chunks_exact_mut(4) {
            px.swap(0, 2);
            let a = px[3] as u32;
            if a > 0 && a < 255 {
                for c in &mut px[..3] {
                    *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
                }
            }
        }
        let img = image::RgbaImage::from_raw(w as u32, h as u32, buf)?;
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).ok()?;
        Some(out.into_inner())
    }
}

/// Icons (as data URLs) for Start-menu apps, cached on disk.
pub fn app_icons(apps: &[(String, String)]) -> HashMap<String, String> {
    let cache = crate::config::data_dir().join("appicons");
    let _ = std::fs::create_dir_all(&cache);
    apps.iter()
        .filter_map(|(name, id)| {
            let file = cache.join(format!("{:x}.png", fxhash(id)));
            let bytes = match std::fs::read(&file) {
                Ok(b) => b,
                Err(_) => {
                    let b = shell_icon_png(id, 64)?;
                    let _ = std::fs::write(&file, &b);
                    b
                }
            };
            Some((name.clone(), format!("data:image/png;base64,{}", B64.encode(bytes))))
        })
        .collect()
}

// ---------- apps (usage counts) ----------

fn usage_file() -> PathBuf {
    crate::config::data_dir().join("apps.json")
}

pub fn app_usage() -> HashMap<String, u32> {
    std::fs::read_to_string(usage_file()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn bump_app(name: &str) {
    let mut u = app_usage();
    *u.entry(name.to_string()).or_default() += 1;
    if let Ok(s) = serde_json::to_string(&u) {
        let _ = std::fs::write(usage_file(), s);
    }
}

// ---------- power ----------

pub fn power(action: &str) {
    let shutdown = |args: &[&str]| {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("shutdown").args(args).creation_flags(0x0800_0000).spawn();
    };
    match action {
        "lock" => unsafe {
            let _ = LockWorkStation();
        },
        "sleep" => unsafe {
            SetSuspendState(false, false, false);
        },
        "restart" => shutdown(&["/r", "/t", "0"]),
        "shutdown" => shutdown(&["/s", "/t", "0"]),
        "signout" => shutdown(&["/l"]),
        _ => {}
    }
}


// ---------- quick controls: volume, dark mode, radios ----------

#[derive(Serialize, Default)]
pub struct Controls {
    /// 0–100, None if there's no output device.
    pub volume: Option<u32>,
    pub muted: bool,
    pub dark: bool,
    pub wifi: Option<bool>,
    pub bluetooth: Option<bool>,
}

fn speaker() -> windows::core::Result<windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume> {
    use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        devices.GetDefaultAudioEndpoint(eRender, eConsole)?.Activate(CLSCTX_ALL, None)
    }
}

pub fn set_volume(percent: u32) {
    if let Ok(v) = speaker() {
        unsafe {
            let _ = v.SetMute(false, std::ptr::null());
            let _ = v.SetMasterVolumeLevelScalar(percent.min(100) as f32 / 100.0, std::ptr::null());
        }
    }
}

pub fn set_muted(muted: bool) {
    if let Ok(v) = speaker() {
        unsafe {
            let _ = v.SetMute(muted, std::ptr::null());
        }
    }
}

const PERSONALIZE: windows::core::PCWSTR = windows::core::w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");

fn read_dword(name: windows::core::PCWSTR) -> Option<u32> {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let mut value = 0u32;
    let mut size = 4u32;
    let err = unsafe { RegGetValueW(HKEY_CURRENT_USER, PERSONALIZE, name, RRF_RT_REG_DWORD, None, Some(&mut value as *mut _ as *mut _), Some(&mut size)) };
    err.is_ok().then_some(value)
}

/// Switches Windows and apps between dark and light, like Settings › Personalisation › Colours.
pub fn set_dark_mode(dark: bool) {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::System::Registry::{RegSetKeyValueW, HKEY_CURRENT_USER, REG_DWORD};
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE};
    let light: u32 = if dark { 0 } else { 1 };
    unsafe {
        for name in [windows::core::w!("AppsUseLightTheme"), windows::core::w!("SystemUsesLightTheme")] {
            let _ = RegSetKeyValueW(HKEY_CURRENT_USER, PERSONALIZE, name, REG_DWORD.0, Some(&light as *const _ as *const _), 4);
        }
        // Tell running apps (and the taskbar) to repaint with the new theme.
        let _ = SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(windows::core::w!("ImmersiveColorSet").as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            200,
            None,
        );
    }
}

fn radio(kind: windows::Devices::Radios::RadioKind) -> Option<windows::Devices::Radios::Radio> {
    use windows::Devices::Radios::Radio;
    let radios = Radio::GetRadiosAsync().ok()?.join().ok()?;
    radios.into_iter().find(|r| r.Kind().is_ok_and(|k| k == kind))
}

fn radio_on(kind: windows::Devices::Radios::RadioKind) -> Option<bool> {
    use windows::Devices::Radios::RadioState;
    radio(kind).and_then(|r| r.State().ok()).map(|s| s == RadioState::On)
}

/// Turns Wi-Fi ("wifi") or Bluetooth ("bluetooth") on or off. Returns false if Windows refused.
pub fn set_radio(which: &str, on: bool) -> bool {
    use windows::Devices::Radios::{Radio, RadioAccessStatus, RadioKind, RadioState};
    let kind = if which == "wifi" { RadioKind::WiFi } else { RadioKind::Bluetooth };
    let Some(r) = radio(kind) else { return false };
    if Radio::RequestAccessAsync().and_then(|op| op.join()).ok() != Some(RadioAccessStatus::Allowed) {
        return false;
    }
    let state = if on { RadioState::On } else { RadioState::Off };
    r.SetStateAsync(state).and_then(|op| op.join()).ok() == Some(RadioAccessStatus::Allowed)
}

pub fn controls() -> Controls {
    use windows::Devices::Radios::RadioKind;
    let (volume, muted) = match speaker() {
        Ok(v) => unsafe {
            (
                v.GetMasterVolumeLevelScalar().ok().map(|x| (x * 100.0).round() as u32),
                v.GetMute().map(|b| b.as_bool()).unwrap_or(false),
            )
        },
        Err(_) => (None, false),
    };
    Controls {
        volume,
        muted,
        dark: read_dword(windows::core::w!("AppsUseLightTheme")) == Some(0),
        wifi: radio_on(RadioKind::WiFi),
        bluetooth: radio_on(RadioKind::Bluetooth),
    }
}

// ---------- favourite apps ----------

fn favorites_file() -> PathBuf {
    crate::config::data_dir().join("favorites.json")
}

pub fn favorite_apps() -> Vec<String> {
    std::fs::read_to_string(favorites_file()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn toggle_favorite(name: &str) -> Vec<String> {
    let mut favs = favorite_apps();
    if let Some(i) = favs.iter().position(|f| f == name) {
        favs.remove(i);
    } else {
        favs.push(name.to_string());
    }
    if let Ok(s) = serde_json::to_string(&favs) {
        let _ = std::fs::write(favorites_file(), s);
    }
    favs
}

/// Puts a window just above the desktop and below every other window, without activating it.
pub fn send_to_bottom(hwnd: isize) {
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, HWND_BOTTOM, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE};
    unsafe {
        let _ = SetWindowPos(HWND(hwnd as *mut _), Some(HWND_BOTTOM), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

// ---------- start with Windows ----------

const RUN_KEY: windows::core::PCWSTR = windows::core::w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const RUN_NAME: windows::core::PCWSTR = windows::core::w!("Shiki");

/// Whether Windows starts Shiki at sign-in (the per-user Run key, same list as Task Manager › Startup apps).
pub fn autostart() -> bool {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
    let mut size = 0u32;
    unsafe { RegGetValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_NAME, RRF_RT_REG_SZ, None, None, Some(&mut size)) }.is_ok()
}

/// Turns starting at sign-in on (pointing at this exe) or off.
pub fn set_autostart(on: bool) -> bool {
    use windows::Win32::System::Registry::{RegDeleteKeyValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};
    unsafe {
        if !on {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_NAME);
            return true;
        }
        let Ok(exe) = std::env::current_exe() else { return false };
        let value = wide(&format!("\"{}\"", exe.display()));
        RegSetKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_NAME, REG_SZ.0, Some(value.as_ptr() as *const _), (value.len() * 2) as u32).is_ok()
    }
}