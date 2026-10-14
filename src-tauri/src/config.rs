use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Config {
    pub api_key: String,
    pub model: String,
    pub dictation_hotkey: Vec<String>,
    pub command_hotkey: Vec<String>,
    /// Folder for the Wallpaper tab's "My folder"; empty means Pictures.
    pub wallpaper_folder: String,

    // ---- Loadout: the four slots ----
    /// Soul: which mask (hannya, oni, kitsune).
    pub mask: String,
    /// Personality: how Shiki talks (see gemini::persona).
    pub personality: String,
    /// Voice: a Gemini TTS voice name; empty means Shiki doesn't talk back.
    pub voice: String,
    /// Also speak replies to typed chat (voice commands are always spoken when a voice is set).
    pub speak_chat: bool,
    /// Abilities the agent may use (see tools::ABILITIES).
    pub abilities: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "gemini-3.5-flash-lite".into(),
            dictation_hotkey: vec!["ctrl".into(), "win".into()],
            command_hotkey: vec!["ctrl".into(), "shift".into()],
            wallpaper_folder: String::new(),
            mask: "hannya".into(),
            personality: "tenang".into(),
            voice: String::new(),
            speak_chat: false,
            abilities: crate::tools::ABILITIES.iter().map(|(id, _)| id.to_string()).collect(),
        }
    }
}

pub fn data_dir() -> PathBuf {
    let appdata = PathBuf::from(std::env::var("APPDATA").unwrap_or_else(|_| ".".into()));
    let dir = appdata.join("Shiki");
    // The app used to be called Tama; carry its settings, to-dos and schedules over once.
    let old = appdata.join("Tama");
    if !dir.exists() && old.exists() {
        let _ = std::fs::rename(&old, &dir);
    }
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn file() -> PathBuf {
    data_dir().join("config.json")
}

pub fn load() -> Config {
    let mut cfg: Config = std::fs::read_to_string(file())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    if cfg.api_key.is_empty() {
        cfg.api_key = std::env::var("GEMINI_API_KEY").unwrap_or_default();
    }
    cfg
}

/// Appends a line to shiki.log in the data dir.
pub fn log(msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(data_dir().join("shiki.log")) {
        let _ = writeln!(f, "{} {msg}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
    }
}

pub fn save(cfg: &Config) {
    if let Ok(s) = serde_json::to_string_pretty(cfg) {
        let _ = std::fs::write(file(), s);
    }
}
