//! Finding the user's files by name, and putting files on the clipboard (so they can be pasted into apps).
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Folders Shiki searches: the usual profile folders (also their OneDrive copies) and D:\Coding.
pub fn roots() -> Vec<PathBuf> {
    let home = PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default());
    let mut roots = Vec::new();
    for name in ["Desktop", "Documents", "Downloads", "Pictures"] {
        for dir in [home.join(name), home.join("OneDrive").join(name)] {
            if dir.is_dir() {
                roots.push(dir);
            }
        }
    }
    roots.push(PathBuf::from(r"D:\Coding"));
    roots.retain(|r| r.is_dir());
    roots
}

const SKIP: &[&str] = &["node_modules", "target", "__pycache__", "venv", "AppData", "$RECYCLE.BIN", "System Volume Information"];

pub struct Found {
    pub path: PathBuf,
    pub size: u64,
    pub modified: SystemTime,
}

/// Files whose name contains every word of `query`, newest first.
pub fn find(query: &str) -> Vec<Found> {
    let words: Vec<String> = query.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(String::from).collect();
    if words.is_empty() {
        return Vec::new();
    }
    let mut found = Vec::new();
    let mut stack: Vec<(PathBuf, u8)> = roots().into_iter().map(|r| (r, 0)).collect();
    let mut visited = 0usize;
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            visited += 1;
            if visited > 200_000 {
                break;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            let Ok(meta) = e.metadata() else { continue };
            if meta.is_dir() {
                if depth < 6 && !name.starts_with('.') && !SKIP.iter().any(|s| s.eq_ignore_ascii_case(&name)) {
                    stack.push((e.path(), depth + 1));
                }
            } else {
                let lower = name.to_lowercase();
                if words.iter().all(|w| lower.contains(w.as_str())) {
                    found.push(Found { path: e.path(), size: meta.len(), modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH) });
                }
            }
        }
    }
    found.sort_by(|a, b| b.modified.cmp(&a.modified));
    found.truncate(8);
    found
}

pub fn describe(f: &Found) -> String {
    let size = if f.size >= 1 << 20 { format!("{:.1} MB", f.size as f64 / 1048576.0) } else { format!("{} KB", (f.size / 1024).max(1)) };
    let when = chrono::DateTime::<chrono::Local>::from(f.modified).format("%Y-%m-%d %H:%M");
    format!("{} ({size}, modified {when})", f.path.display())
}

/// Puts files on the clipboard the way Explorer's Copy does (CF_HDROP), so Ctrl+V pastes them as attachments.
pub fn copy_to_clipboard(paths: &[&Path]) -> Result<(), String> {
    use windows::Win32::Foundation::{HANDLE, POINT};
    use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::UI::Shell::DROPFILES;
    const CF_HDROP: u32 = 15;

    let mut list: Vec<u16> = paths.iter().flat_map(|p| p.as_os_str().to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect::<Vec<_>>()).collect();
    list.push(0); // the list ends with an extra NUL
    let header = std::mem::size_of::<DROPFILES>();
    unsafe {
        let mem = GlobalAlloc(GMEM_MOVEABLE, header + list.len() * 2).map_err(|e| e.to_string())?;
        let ptr = GlobalLock(mem) as *mut u8;
        if ptr.is_null() {
            return Err("Clipboard memory could not be locked.".into());
        }
        std::ptr::write(ptr as *mut DROPFILES, DROPFILES { pFiles: header as u32, pt: POINT::default(), fNC: false.into(), fWide: true.into() });
        std::ptr::copy_nonoverlapping(list.as_ptr() as *const u8, ptr.add(header), list.len() * 2);
        let _ = GlobalUnlock(mem);
        OpenClipboard(None).map_err(|e| e.to_string())?;
        let result = EmptyClipboard().and_then(|_| SetClipboardData(CF_HDROP, Some(HANDLE(mem.0))));
        let _ = CloseClipboard();
        result.map(|_| ()).map_err(|e| e.to_string())
    }
}
