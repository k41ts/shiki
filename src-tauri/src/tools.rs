//! Functions Gemini can call to act on the PC.
use crate::{input, store};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    os::windows::process::CommandExt,
    process::Command,
    sync::Mutex,
    thread::sleep,
    time::Duration,
};

const NO_WINDOW: u32 = 0x0800_0000;

fn decl(name: &str, desc: &str, params: &[(&str, &str, &str)], required: &[&str]) -> Value {
    let props: serde_json::Map<String, Value> = params
        .iter()
        .map(|(n, t, d)| (n.to_string(), json!({ "type": t, "description": d })))
        .collect();
    let mut f = json!({ "name": name, "description": desc });
    if !params.is_empty() {
        f["parameters"] = json!({ "type": "OBJECT", "properties": props, "required": required });
    }
    f
}

/// Loadout › Abilities: each ability unlocks a group of tools.
pub const ABILITIES: &[(&str, &[&str])] = &[
    ("apps", &["open_app"]),
    ("web", &["search", "open_website", "play_youtube"]),
    ("media", &["media_seek", "media_control", "now_playing"]),
    ("keyboard", &["press_keys", "type_text"]),
    ("planner", &["add_todo", "list_todos", "complete_todo", "schedule_task", "list_scheduled", "cancel_scheduled"]),
    ("files", &["find_files", "whatsapp_prepare", "whatsapp_send", "whatsapp_cancel"]),
];

/// Tool declarations for the enabled abilities only.
pub fn declarations(abilities: &[String]) -> Value {
    let allowed: Vec<&str> = ABILITIES
        .iter()
        .filter(|(id, _)| abilities.iter().any(|a| a == id))
        .flat_map(|(_, tools)| tools.iter().copied())
        .collect();
    let all = all_declarations();
    Value::Array(
        all.as_array()
            .map(|a| a.iter().filter(|d| d["name"].as_str().is_some_and(|n| allowed.contains(&n))).cloned().collect())
            .unwrap_or_default(),
    )
}

fn all_declarations() -> Value {
    json!([
        decl("open_app", "Open an installed Windows application by name.",
            &[("name", "STRING", "App name, e.g. Spotify, Notepad, Unity Hub, Calculator")], &["name"]),
        decl("search", "Search in the default browser.",
            &[("query", "STRING", "What to search for"),
              ("site", "STRING", "google, youtube, amazon, wikipedia, maps, tokopedia, shopee, or a domain like amazon.in. Default google")],
            &["query"]),
        decl("play_youtube", "Play a video or song on YouTube right away: finds the top result and opens it playing. Use this whenever the user wants to play, watch or listen to something (including \"play it\" after a search), instead of search.",
            &[("query", "STRING", "What to play, e.g. 'paparazzi dubstep'"),
              ("start_seconds", "NUMBER", "Start playing from this many seconds in. ALWAYS use this when the user wants a new song/video to start at a certain time (\"play X from minute 2\", \"play X and skip to 1:30\") instead of calling media_seek afterwards")],
            &["query"]),
        decl("media_seek", "Jump within whatever is ALREADY playing (YouTube in the browser, Spotify, etc.). Not for something you are starting in the same turn: give play_youtube start_seconds instead.",
            &[("seconds", "NUMBER", "Seconds. Convert times like 1:30 to 90"),
              ("mode", "STRING", "'to' = jump to that time (default), 'forward' = skip ahead by it, 'back' = go back by it")],
            &["seconds"]),
        decl("media_control", "Control whatever is playing right now.",
            &[("action", "STRING", "play, pause, toggle, next or previous")], &["action"]),
        decl("now_playing", "What is playing right now, and at what time.", &[], &[]),
        decl("open_website", "Open a website in the default browser.",
            &[("url", "STRING", "e.g. youtube.com or https://github.com")], &["url"]),
        decl("press_keys", "Press a keyboard shortcut in the active window.",
            &[("keys", "STRING", "e.g. ctrl+t, win+d, alt+tab, alt+f4, volume up, volume down, volume mute, play/pause media, next track, previous track")],
            &["keys"]),
        decl("type_text", "Type text into the active window at the cursor.",
            &[("text", "STRING", "Text to type")], &["text"]),
        decl("add_todo", "Add an item to the user's to-do list.",
            &[("text", "STRING", "The to-do item"),
              ("due", "STRING", "Optional reminder time, local, YYYY-MM-DD HH:MM")],
            &["text"]),
        decl("list_todos", "List the user's to-do items, numbered.", &[], &[]),
        decl("complete_todo", "Mark a to-do item as done (removes it).",
            &[("number", "INTEGER", "1-based number from list_todos")], &["number"]),
        decl("schedule_task", "Schedule a command to run automatically later, even if nobody is at the keyboard. Use for anything in the future or recurring.",
            &[("command", "STRING", "The instruction to carry out then, e.g. 'open Unity and Spotify' or 'search amazon.in for gift wraps'"),
              ("at", "STRING", "Local time: YYYY-MM-DD HH:MM, or HH:MM"),
              ("repeat", "STRING", "once, daily or weekdays. Default once")],
            &["command", "at"]),
        decl("find_files", "Find the user's files by name in Desktop, Documents, Downloads, Pictures and D:\\Coding. Returns up to 8 matches, newest first, with full paths.",
            &[("query", "STRING", "Words from the file name, e.g. 'laporan pkl' or 'cv pdf'")], &["query"]),
        decl("whatsapp_prepare", "Open the user's WhatsApp Web chat with a contact and attach a file, WITHOUT sending. Afterwards you must ask the user to confirm (name the file and the contact) and end your turn.",
            &[("file", "STRING", "Full path from find_files"),
              ("contact", "STRING", "Contact or group name as it appears in WhatsApp"),
              ("caption", "STRING", "Optional message to send with the file")],
            &["file", "contact"]),
        decl("whatsapp_send", "Send the file prepared by whatsapp_prepare. Only call this after the user explicitly said yes in a NEW message.", &[], &[]),
        decl("whatsapp_cancel", "Cancel the file prepared by whatsapp_prepare without sending it.", &[], &[]),
        decl("list_scheduled", "List scheduled tasks and routines, numbered.", &[], &[]),
        decl("cancel_scheduled", "Cancel a scheduled task.",
            &[("number", "INTEGER", "1-based number from list_scheduled")], &["number"]),
    ])
}

pub fn execute(name: &str, args: &Value) -> String {
    let s = |k: &str| args[k].as_str().unwrap_or("").to_string();
    let n = |k: &str| args[k].as_f64().unwrap_or(0.0) as usize;
    match name {
        "open_app" => open_app(&s("name")),
        "search" => search(&s("query"), &s("site")),
        "play_youtube" => play_youtube(&s("query"), args["start_seconds"].as_f64().unwrap_or(0.0)),
        "media_seek" => {
            let mode = s("mode");
            crate::system::media_seek(args["seconds"].as_f64().unwrap_or(0.0), if mode.is_empty() { "to" } else { &mode })
        }
        "media_control" => {
            crate::system::media_action(&s("action"));
            sleep(Duration::from_millis(400));
            crate::system::now_playing()
        }
        "now_playing" => crate::system::now_playing(),
        "open_website" => {
            let mut url = s("url");
            if !url.contains("://") {
                url = format!("https://{url}");
            }
            open(&url)
        }
        "press_keys" => {
            sleep(Duration::from_millis(200));
            match input::press(&s("keys")) {
                Ok(()) => format!("Pressed {}.", s("keys")),
                Err(e) => e,
            }
        }
        "type_text" => {
            sleep(Duration::from_millis(200));
            match input::paste(&s("text")) {
                Ok(()) => "Typed it.".into(),
                Err(e) => e,
            }
        }
        "add_todo" => store::add_todo(&s("text"), &s("due")),
        "list_todos" => store::list_todos(),
        "complete_todo" => store::complete_todo(n("number")),
        "schedule_task" => {
            let repeat = s("repeat");
            store::schedule(&s("command"), &s("at"), if repeat.is_empty() { "once" } else { &repeat })
        }
        "find_files" => {
            let found = crate::files::find(&s("query"));
            if found.is_empty() {
                format!("No files matching '{}' in Desktop, Documents, Downloads, Pictures or D:\\Coding.", s("query"))
            } else {
                found.iter().enumerate().map(|(i, f)| format!("{}. {}", i + 1, crate::files::describe(f))).collect::<Vec<_>>().join("\n")
            }
        }
        "whatsapp_prepare" => crate::whatsapp::prepare(&s("file"), &s("contact"), &s("caption")),
        "whatsapp_send" => crate::whatsapp::send(),
        "whatsapp_cancel" => crate::whatsapp::cancel(),
        "list_scheduled" => store::list_tasks(),
        "cancel_scheduled" => store::cancel_task(n("number")),
        other => format!("Unknown tool {other}."),
    }
}

fn open(url: &str) -> String {
    match open::that_detached(url) {
        Ok(()) => format!("Opened {url}."),
        Err(e) => format!("Could not open {url}: {e}"),
    }
}

fn encode(query: &str) -> String {
    query
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            b' ' => "+".into(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Opens the top YouTube result for `query` (the watch page autoplays).
fn play_youtube(query: &str, start_seconds: f64) -> String {
    let search_url = format!("https://www.youtube.com/results?search_query={}", encode(query));
    let html = reqwest::blocking::Client::new()
        .get(&search_url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36")
        .header("Accept-Language", "en-US,en;q=0.9")
        .timeout(Duration::from_secs(10))
        .send()
        .and_then(|r| r.text())
        .unwrap_or_default();
    // The results page embeds its data as JSON; the first "videoId" is the top result.
    let Some(start) = html.find("\"videoId\":\"").map(|i| i + 11) else {
        open(&search_url);
        return format!("Couldn't pick a video automatically, so I opened YouTube results for '{query}'.");
    };
    let id = &html[start..(start + 11).min(html.len())];
    let title = html[start..]
        .find("\"title\":{\"runs\":[{\"text\":\"")
        .map(|i| start + i + 26)
        .and_then(|t| html[t..].find('"').map(|e| html[t..t + e].replace("\\u0026", "&")))
        .unwrap_or_else(|| query.to_string());
    crate::system::pause_all(); // e.g. Spotify, or another tab
    crate::system::mark_new_media();
    let start = start_seconds.max(0.0) as u64;
    let url = if start > 0 {
        format!("https://www.youtube.com/watch?v={id}&t={start}s")
    } else {
        format!("https://www.youtube.com/watch?v={id}")
    };
    if !crate::system::navigate_youtube_tab(&url) {
        open(&url);
    }
    if start > 0 {
        format!("Playing '{title}' on YouTube from {}:{:02}.", start / 60, start % 60)
    } else {
        format!("Playing '{title}' on YouTube.")
    }
}

fn search(query: &str, site: &str) -> String {
    let q = encode(query);
    let site = site.trim().to_lowercase();
    let url = match site.as_str() {
        "" | "google" => format!("https://www.google.com/search?q={q}"),
        "youtube" => format!("https://www.youtube.com/results?search_query={q}"),
        "amazon" => format!("https://www.amazon.com/s?k={q}"),
        "wikipedia" => format!("https://en.wikipedia.org/w/index.php?search={q}"),
        "maps" => format!("https://www.google.com/maps/search/{q}"),
        "tokopedia" => format!("https://www.tokopedia.com/search?q={q}"),
        "shopee" => format!("https://shopee.co.id/search?keyword={q}"),
        domain => format!("https://www.google.com/search?q=site%3A{domain}+{q}"),
    };
    open(&url);
    format!("Searching {} for '{query}'.", if site.is_empty() { "google" } else { &site })
}

// ---------- apps ----------

static APPS: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

/// Name -> AppID for everything in the Start menu (desktop and Store apps).
fn load_apps() -> HashMap<String, String> {
    let ps = "[Console]::OutputEncoding=[Text.Encoding]::UTF8; Get-StartApps | ConvertTo-Json -Compress";
    let out = Command::new("powershell")
        .args(["-NoProfile", "-Command", ps])
        .creation_flags(NO_WINDOW)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let v: Value = serde_json::from_str(&out).unwrap_or(Value::Null);
    let items = match v {
        Value::Array(a) => a,
        Value::Object(_) => vec![v],
        _ => vec![],
    };
    items
        .iter()
        .filter_map(|i| Some((i["Name"].as_str()?.to_string(), i["AppID"].as_str()?.to_string())))
        .collect()
}

fn find_app(apps: &HashMap<String, String>, name: &str) -> Option<(String, String)> {
    let n = name.trim().to_lowercase();
    let pick = |k: &String| Some((k.clone(), apps[k].clone()));
    if let Some(k) = apps.keys().find(|k| k.to_lowercase() == n) {
        return pick(k);
    }
    if let Some(k) = apps.keys().filter(|k| k.to_lowercase().contains(&n)).min_by_key(|k| k.len()) {
        return pick(k);
    }
    apps.keys()
        .map(|k| (strsim::normalized_levenshtein(&k.to_lowercase(), &n), k))
        .filter(|(score, _)| *score >= 0.6)
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .and_then(|(_, k)| pick(k))
}

/// (name, AppID) pairs for the given Start-menu app names.
pub fn app_ids(names: &[String]) -> Vec<(String, String)> {
    let mut cache = APPS.lock().unwrap();
    let apps = cache.get_or_insert_with(load_apps);
    names.iter().filter_map(|n| Some((n.clone(), apps.get(n)?.clone()))).collect()
}

/// Start-menu app names, most used first.
pub fn app_list() -> Vec<String> {
    let mut cache = APPS.lock().unwrap();
    let apps = cache.get_or_insert_with(load_apps);
    let usage = crate::system::app_usage();
    let mut names: Vec<String> = apps.keys().cloned().collect();
    names.sort_by(|a, b| {
        let (ua, ub) = (usage.get(a).copied().unwrap_or(0), usage.get(b).copied().unwrap_or(0));
        ub.cmp(&ua).then_with(|| a.to_lowercase().cmp(&b.to_lowercase()))
    });
    names
}

pub fn open_app(name: &str) -> String {
    let mut cache = APPS.lock().unwrap();
    let mut found = cache.as_ref().and_then(|apps| find_app(apps, name));
    if found.is_none() {
        let apps = load_apps();
        found = find_app(&apps, name);
        *cache = Some(apps);
    }
    match found {
        Some((title, id)) => {
            let _ = Command::new("explorer.exe").arg(format!("shell:AppsFolder\\{id}")).spawn();
            crate::system::bump_app(&title);
            format!("Opened {title}.")
        }
        None => format!("No installed app matches '{name}'."),
    }
}
