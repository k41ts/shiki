//! Sending a file to a WhatsApp contact through WhatsApp Web in the browser.
//!
//! WhatsApp Web is driven through UI Automation (the accessibility tree screen readers use), so Shiki
//! clicks the real search box, chat and Send button and can tell when something isn't there.
//! Labels are matched in Indonesian and English.
//!
//! Two steps on purpose: `prepare` opens the chat and attaches the file (WhatsApp shows its preview),
//! then Shiki asks the user. `send` only works in a later agent turn, i.e. after the user answered,
//! so a misheard name or file can't go out without a "yes".
use crate::{files, input};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread::sleep;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationInvokePattern, TreeScope_Descendants, UIA_ButtonControlTypeId,
    UIA_CONTROLTYPE_ID, UIA_ControlTypePropertyId, UIA_DataGridControlTypeId, UIA_DataItemControlTypeId, UIA_EditControlTypeId,
    UIA_InvokePatternId, UIA_NamePropertyId, TreeScope_Children,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowTextW, IsIconic, IsWindowVisible, SetForegroundWindow, ShowWindow, SW_RESTORE,
};

/// Bumped at the start of every agent turn.
pub static TURN: AtomicU64 = AtomicU64::new(0);

struct Pending {
    file: String,
    chat: String,
    turn: u64,
    hwnd: isize,
}
static PENDING: Mutex<Option<Pending>> = Mutex::new(None);

pub fn is_pending() -> bool {
    PENDING.lock().unwrap().is_some()
}

// ---------- the browser window ----------

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> windows::core::BOOL {
    let out = &mut *(lparam.0 as *mut Vec<HWND>);
    if IsWindowVisible(hwnd).as_bool() {
        let (mut title, mut class) = ([0u16; 256], [0u16; 64]);
        let tn = GetWindowTextW(hwnd, &mut title).max(0) as usize;
        let cn = GetClassNameW(hwnd, &mut class).max(0) as usize;
        let (t, c) = (String::from_utf16_lossy(&title[..tn]), String::from_utf16_lossy(&class[..cn]));
        // Chromium browsers (Brave, Chrome, Edge) title the window after the active tab.
        if c == "Chrome_WidgetWin_1" && t.contains("WhatsApp") {
            out.push(hwnd);
        }
    }
    windows::core::BOOL(1)
}

fn whatsapp_window() -> Option<HWND> {
    let mut wins: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut wins as *mut _ as isize));
    }
    wins.first().copied()
}

fn focus(hwnd: HWND) -> bool {
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        input::suppress_start_menu(); // gives us the right to take focus
        let _ = SetForegroundWindow(hwnd);
        sleep(Duration::from_millis(250));
        GetForegroundWindow() == hwnd
    }
}

// ---------- UI Automation ----------

struct Node {
    el: IUIAutomationElement,
    kind: UIA_CONTROLTYPE_ID,
    name: String,
}

struct Page {
    uia: IUIAutomation,
    root: IUIAutomationElement,
}

impl Page {
    fn new(hwnd: HWND) -> Option<Page> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
            let root = uia.ElementFromHandle(hwnd).ok()?;
            Some(Page { uia, root })
        }
    }

    /// Every element with its type and label, fetched in one round trip.
    fn nodes(&self) -> Vec<Node> {
        self.under(&self.root, false)
    }

    /// Elements below `el` (only direct children if `children_only`).
    fn under(&self, el: &IUIAutomationElement, children_only: bool) -> Vec<Node> {
        unsafe {
            let run = || -> windows::core::Result<Vec<Node>> {
                let cache = self.uia.CreateCacheRequest()?;
                cache.AddProperty(UIA_ControlTypePropertyId)?;
                cache.AddProperty(UIA_NamePropertyId)?;
                let scope = if children_only { TreeScope_Children } else { TreeScope_Descendants };
                let all = el.FindAllBuildCache(scope, &self.uia.CreateTrueCondition()?, &cache)?;
                let mut out = Vec::new();
                for i in 0..all.Length()? {
                    let el = all.GetElement(i)?;
                    let kind = el.CachedControlType().unwrap_or_default();
                    let name = el.CachedName().map(|b| b.to_string()).unwrap_or_default();
                    out.push(Node { el, kind, name });
                }
                Ok(out)
            };
            run().unwrap_or_default()
        }
    }

    fn find(&self, kind: UIA_CONTROLTYPE_ID, matches: impl Fn(&str) -> bool) -> Option<Node> {
        self.nodes().into_iter().find(|n| n.kind == kind && matches(&n.name.to_lowercase()))
    }

    /// Waits up to `secs` for an element to appear (WhatsApp Web loads and re-renders asynchronously).
    fn wait_for(&self, kind: UIA_CONTROLTYPE_ID, secs: u64, matches: impl Fn(&str) -> bool) -> Option<Node> {
        let start = Instant::now();
        loop {
            if let Some(n) = self.find(kind, &matches) {
                return Some(n);
            }
            if start.elapsed() > Duration::from_secs(secs) {
                return None;
            }
            sleep(Duration::from_millis(400));
        }
    }
}

fn click(n: &Node) -> bool {
    unsafe {
        if let Ok(p) = n.el.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId) {
            if p.Invoke().is_ok() {
                return true;
            }
        }
        match n.el.CurrentBoundingRectangle() {
            Ok(r) if r.right > r.left && r.bottom > r.top => {
                input::click((r.left + r.right) / 2, (r.top + r.bottom) / 2);
                true
            }
            _ => false,
        }
    }
}

fn is_search_box(name: &str) -> bool {
    (name.starts_with("cari") || name.starts_with("search")) && (name.contains("obrolan") || name.contains("chat"))
}
/// The search box's label comes from its placeholder, so it's empty once something is typed in it.
fn is_search_or_blank(name: &str) -> bool {
    is_search_box(name) || name.is_empty()
}
fn is_results(name: &str) -> bool {
    name.starts_with("hasil pencarian") || name.starts_with("search results")
}

const DAYS: &[&str] = &[
    "senin", "selasa", "rabu", "kamis", "jumat", "sabtu", "minggu", "kemarin", "monday", "tuesday", "wednesday", "thursday", "friday",
    "saturday", "sunday", "yesterday",
];

/// A result row reads "Alex Luqman 12.41 PM Telepon suara …": the chat's title is what comes before the time/date.
fn title_of(row: &str) -> String {
    let stamp = |t: &str| {
        let lower = t.to_lowercase();
        DAYS.contains(&lower.as_str()) || (t.starts_with(|c: char| c.is_ascii_digit()) && t.contains(['.', ':', '/']))
    };
    row.split_whitespace().take_while(|t| !stamp(t)).collect::<Vec<_>>().join(" ")
}

/// Lowercase letters and digits only; "q" and "k" count as the same sound (Luqman / Lukman).
fn norm(word: &str) -> String {
    word.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect::<String>().replace('q', "k")
}
fn words(s: &str) -> Vec<String> {
    s.split_whitespace().map(norm).filter(|w| !w.is_empty()).collect()
}
fn similar(a: &str, b: &str) -> bool {
    a == b || (a.len() >= 3 && b.starts_with(a)) || strsim::normalized_levenshtein(a, b) >= 0.74
}
/// How well a chat title matches what the user said; None unless every spoken word matches a title word, in order.
fn score(spoken: &[String], title: &str) -> Option<i32> {
    let title = words(title);
    let mut at = 0;
    let mut first = None;
    for w in spoken {
        let i = (at..title.len()).find(|&i| similar(w, &title[i]))?;
        first.get_or_insert(i);
        at = i + 1;
    }
    let extra = title.len() as i32 - spoken.len() as i32;
    Some(100 - extra * 3 - first.unwrap_or(0) as i32 * 10)
}
fn is_composer(name: &str) -> bool {
    name.starts_with("ketik pesan") || name.starts_with("type a message")
}
/// The attachment preview's send button: "Kirim 1 yang dipilih" / "Send 1 selected" (or plain "Kirim"/"Send"),
/// but not other "Kirim …" buttons such as "Kirim dokumen".
fn is_send(name: &str) -> bool {
    name == "kirim" || name == "send" || (name.starts_with("kirim ") && name.contains("dipilih")) || (name.starts_with("send ") && name.contains("selected"))
}
/// "Ketik pesan ke Andi" → "Andi"
fn chat_name(composer: &str) -> String {
    for prefix in ["Ketik pesan ke ", "Type a message to "] {
        if let Some(rest) = composer.strip_prefix(prefix) {
            return rest.to_string();
        }
    }
    composer.to_string()
}

// ---------- the flow ----------

/// Opens the chat with `contact` in WhatsApp Web and attaches `file` (with an optional caption). Does not send.
pub fn prepare(file: &str, contact: &str, caption: &str) -> String {
    let path = Path::new(file);
    if !path.is_file() {
        return format!("File not found: {file}. Use find_files to get the exact path.");
    }
    let hwnd = match whatsapp_window() {
        Some(h) => h,
        None => {
            let _ = open::that_detached("https://web.whatsapp.com");
            let start = Instant::now();
            let mut found = None;
            while found.is_none() && start.elapsed() < Duration::from_secs(20) {
                sleep(Duration::from_millis(500));
                found = whatsapp_window();
            }
            match found {
                Some(h) => h,
                None => return "WhatsApp Web didn't open. Ask the user to open web.whatsapp.com and log in, then try again.".into(),
            }
        }
    };
    if !focus(hwnd) {
        return "Couldn't bring WhatsApp Web to the front.".into();
    }
    let Some(page) = Page::new(hwnd) else { return "Couldn't read the WhatsApp Web page.".into() };
    // An earlier attachment preview would hide the search box: close it first.
    if PENDING.lock().unwrap().take().is_some() || page.find(UIA_ButtonControlTypeId, is_send).is_some() {
        let _ = input::press("esc");
        sleep(Duration::from_millis(500));
    }

    // 1. Search box (also tells us WhatsApp Web has finished loading and is logged in).
    let Some(search) = page.wait_for(UIA_EditControlTypeId, 25, is_search_or_blank) else {
        return "WhatsApp Web isn't ready (still loading, or not logged in). Ask the user to check the WhatsApp tab and try again.".into();
    };
    let saved = arboard::Clipboard::new().and_then(|mut c| c.get_text()).ok();
    let restore = || {
        if let Some(text) = &saved {
            let _ = arboard::Clipboard::new().and_then(|mut c| c.set_text(text.clone()));
        }
    };

    // 2. WhatsApp's own search wants exact spelling, so search one word at a time and match the
    //    full name ourselves (tolerating small spelling differences). Only chat titles count, so a
    //    group that merely lists the person as a member isn't picked.
    let spoken = words(contact);
    let mut best: Vec<(i32, String, IUIAutomationElement)> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for query in contact.split_whitespace().filter(|q| q.chars().count() >= 2) {
        click(&search);
        sleep(Duration::from_millis(250));
        let _ = input::press("ctrl+a");
        let _ = input::paste(query);
        let Some(grid) = page.wait_for(UIA_DataGridControlTypeId, 5, is_results) else { continue };
        sleep(Duration::from_millis(500)); // let the list settle
        for row in page.under(&grid.el, true).into_iter().filter(|n| n.kind == UIA_DataItemControlTypeId) {
            let title = title_of(&row.name);
            if title.is_empty() || seen.contains(&title) {
                continue;
            }
            seen.push(title.clone());
            if let Some(sc) = score(&spoken, &title) {
                best.push((sc, title, row.el));
            }
        }
        if best.iter().any(|(sc, ..)| *sc >= 100) {
            break; // an exact-length match; no need to try the other words
        }
    }
    best.sort_by(|a, b| b.0.cmp(&a.0));
    let tied: Vec<&String> = best.iter().filter(|(sc, ..)| Some(*sc) == best.first().map(|b| b.0)).map(|(_, t, _)| t).collect();
    if best.is_empty() {
        let _ = input::press("esc");
        restore();
        let close: Vec<&String> = seen.iter().take(5).collect();
        return if close.is_empty() {
            format!("No WhatsApp chat matches '{contact}'. Ask the user for the name exactly as it appears in WhatsApp.")
        } else {
            format!("No WhatsApp chat matches '{contact}'. Chats found while searching: {}. Ask the user which one they mean.", close.iter().map(|t| format!("'{t}'")).collect::<Vec<_>>().join(", "))
        };
    }
    if tied.len() > 1 {
        let _ = input::press("esc");
        restore();
        return format!("Several WhatsApp chats match '{contact}': {}. Ask the user which one.", tied.iter().map(|t| format!("'{t}'")).collect::<Vec<_>>().join(", "));
    }
    let (_, _, row) = best.swap_remove(0);
    click(&Node { el: row, kind: UIA_DataItemControlTypeId, name: String::new() });

    // 3. The message box names the open chat: that's how we know which chat we're in.
    sleep(Duration::from_millis(800));
    let Some(composer) = page.wait_for(UIA_EditControlTypeId, 6, is_composer) else {
        restore();
        return format!("Couldn't open the chat for '{contact}'.");
    };
    let chat = chat_name(&composer.name);

    // 4. Paste the file like Explorer's Copy → WhatsApp opens its attachment preview.
    click(&composer);
    sleep(Duration::from_millis(300));
    if let Err(e) = files::copy_to_clipboard(&[path]) {
        restore();
        return format!("Couldn't attach the file: {e}");
    }
    let _ = input::press("ctrl+v");
    let preview = page.wait_for(UIA_ButtonControlTypeId, 8, is_send);
    restore();
    if preview.is_none() {
        return format!("Opened the chat '{chat}', but WhatsApp didn't show the attachment preview, so nothing is attached. Ask the user to try again.");
    }
    if !caption.trim().is_empty() {
        if let Some(cap) = page.find(UIA_EditControlTypeId, |n| n.contains("keterangan") || n.contains("caption")) {
            click(&cap);
            sleep(Duration::from_millis(200));
        }
        let _ = input::paste(caption.trim());
    }

    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    *PENDING.lock().unwrap() = Some(Pending { file: name.clone(), chat: chat.clone(), turn: TURN.load(Ordering::SeqCst), hwnd: hwnd.0 as isize });
    format!(
        "Prepared, NOT sent: '{name}' is attached in the WhatsApp chat '{chat}', preview open. \
         Ask the user to confirm sending '{name}' to '{chat}' (use that exact chat name)."
    )
}

/// Sends what `prepare` attached. Refuses within the same turn, so the user must have answered first.
pub fn send() -> String {
    let mut pending = PENDING.lock().unwrap();
    let Some(p) = pending.take() else { return "Nothing is waiting to be sent.".into() };
    if p.turn == TURN.load(Ordering::SeqCst) {
        let msg = format!("Not sent yet: ask the user to confirm sending '{}' to '{}' and wait for their reply.", p.file, p.chat);
        *pending = Some(p);
        return msg;
    }
    let hwnd = HWND(p.hwnd as *mut _);
    let page = if focus(hwnd) { Page::new(hwnd) } else { None };
    let Some(page) = page else {
        *pending = Some(p);
        return "Couldn't bring WhatsApp Web to the front; nothing was sent.".into();
    };
    let Some(button) = page.find(UIA_ButtonControlTypeId, is_send) else {
        return format!("The preview for '{}' is gone (it may have been closed), so nothing was sent.", p.file);
    };
    click(&button);
    // Once sent, the preview (and its Send button) disappears.
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        sleep(Duration::from_millis(400));
        if page.find(UIA_ButtonControlTypeId, is_send).is_none() {
            return format!("Sent '{}' to '{}' on WhatsApp.", p.file, p.chat);
        }
    }
    format!("Clicked Send for '{}', but the preview is still open; ask the user to check WhatsApp.", p.file)
}

/// Closes the attachment preview without sending.
pub fn cancel() -> String {
    let Some(p) = PENDING.lock().unwrap().take() else { return "Nothing was waiting to be sent.".into() };
    if focus(HWND(p.hwnd as *mut _)) {
        let _ = input::press("esc");
    }
    format!("Cancelled: '{}' was not sent to '{}'.", p.file, p.chat)
}
