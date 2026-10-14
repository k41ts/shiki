//! Synthetic keyboard input and clipboard paste.
use std::{thread::sleep, time::Duration};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VIRTUAL_KEY,
};

fn key(vk: u16, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// Press all keys in order, then release them in reverse.
pub fn chord(vks: &[u16]) {
    let mut inputs: Vec<INPUT> = vks.iter().map(|&vk| key(vk, false)).collect();
    inputs.extend(vks.iter().rev().map(|&vk| key(vk, true)));
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

/// Tap an unassigned key so Windows doesn't treat the coming Win release as "open Start".
pub fn suppress_start_menu() {
    chord(&[0xE8]);
}

pub fn vk_from_name(name: &str) -> Option<u16> {
    let n = name.trim().to_lowercase();
    let vk = match n.as_str() {
        "ctrl" | "control" => 0x11,
        "shift" => 0x10,
        "alt" => 0x12,
        "win" | "windows" => 0x5B,
        "enter" | "return" => 0x0D,
        "esc" | "escape" => 0x1B,
        "tab" => 0x09,
        "space" => 0x20,
        "backspace" => 0x08,
        "delete" | "del" => 0x2E,
        "up" => 0x26,
        "down" => 0x28,
        "left" => 0x25,
        "right" => 0x27,
        "home" => 0x24,
        "end" => 0x23,
        "page up" | "pageup" => 0x21,
        "page down" | "pagedown" => 0x22,
        "print screen" | "printscreen" => 0x2C,
        "volume up" => 0xAF,
        "volume down" => 0xAE,
        "volume mute" | "mute" => 0xAD,
        "play/pause media" | "play/pause" | "play pause" => 0xB3,
        "next track" => 0xB0,
        "previous track" | "prev track" => 0xB1,
        "stop media" => 0xB2,
        s if s.len() == 1 => {
            let c = s.chars().next()?.to_ascii_uppercase();
            if c.is_ascii_alphanumeric() { c as u16 } else { return None }
        }
        s if s.starts_with('f') => match s[1..].parse::<u16>() {
            Ok(k @ 1..=24) => 0x70 + k - 1,
            _ => return None,
        },
        _ => return None,
    };
    Some(vk)
}

/// Press a shortcut like "ctrl+t" or "volume up".
pub fn press(keys: &str) -> Result<(), String> {
    let vks: Option<Vec<u16>> = keys.split('+').map(vk_from_name).collect();
    let vks = vks.ok_or_else(|| format!("Unknown key in '{keys}'"))?;
    chord(&vks);
    Ok(())
}

/// Paste text into the focused window, restoring the user's clipboard afterwards.
pub fn paste(text: &str) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    let old = cb.get_text().ok();
    cb.set_text(text.to_string()).map_err(|e| e.to_string())?;
    sleep(Duration::from_millis(80));
    // Ctrl+V with small gaps; some apps drop keys that arrive in the same instant.
    for (vk, up) in [(0x11, false), (0x56, false), (0x56, true), (0x11, true)] {
        unsafe {
            SendInput(&[key(vk, up)], std::mem::size_of::<INPUT>() as i32);
        }
        sleep(Duration::from_millis(15));
    }
    // Give slow apps time to read the clipboard before restoring the user's old content.
    sleep(Duration::from_millis(1000));
    if let Some(old) = old {
        if cb.get_text().ok().as_deref() == Some(text) {
            let _ = cb.set_text(old);
        }
    }
    Ok(())
}
