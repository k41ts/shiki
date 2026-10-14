//! Push-to-talk hotkeys, polled with GetAsyncKeyState (no keyboard hook needed).
use crate::{input::vk_from_name, Mode};
use std::{
    thread,
    time::{Duration, Instant},
};
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

const MODIFIERS: [u16; 11] = [0x10, 0x11, 0x12, 0x5B, 0x5C, 0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5];
const DELAY: Duration = Duration::from_millis(250);

fn down(vk: u16) -> bool {
    unsafe { (GetAsyncKeyState(vk as i32) as u16) & 0x8000 != 0 }
}

pub fn key_down(name: &str) -> bool {
    match name.trim().to_lowercase().as_str() {
        "win" | "windows" => down(0x5B) || down(0x5C),
        other => vk_from_name(other).is_some_and(down),
    }
}

/// Any non-modifier key held? (mouse buttons excluded so circling works)
fn other_key_down() -> bool {
    (0x08u16..=0xFE).any(|vk| !MODIFIERS.contains(&vk) && down(vk))
}

pub fn wait_modifiers_released() {
    let end = Instant::now() + Duration::from_secs(2);
    while Instant::now() < end && ["ctrl", "shift", "alt", "win"].iter().any(|k| key_down(k)) {
        thread::sleep(Duration::from_millis(20));
    }
}

/// Lifecycle of a push-to-talk hold.
#[derive(Clone, Copy)]
pub enum Hold {
    /// Keys just went down: start the mic now so the first word isn't lost.
    Press,
    /// Held for DELAY with no other key: it's really push-to-talk.
    Confirm(Mode),
    /// Turned out to be a normal shortcut (Ctrl+Shift+T, …) or a quick tap: discard.
    Cancel,
    /// Released after Confirm: process the recording.
    Release(Mode),
}

/// Reports push-to-talk holds; a combo only counts once held for DELAY with no other key
/// pressed, so normal shortcuts still work.
pub fn spawn(
    combos: impl Fn() -> Vec<(Mode, Vec<String>)> + Send + 'static,
    on_event: impl Fn(Hold) + Send + 'static,
) {
    thread::spawn(move || {
        let mut state: Option<Mode> = None;
        let mut pending: Option<(Mode, Instant, bool)> = None; // (mode, since, cancelled)
        loop {
            let mut list = combos();
            list.sort_by_key(|(_, keys)| std::cmp::Reverse(keys.len()));
            let active = list
                .iter()
                .find(|(_, keys)| !keys.is_empty() && keys.iter().all(|k| key_down(k)))
                .map(|(m, _)| *m);

            match (state, active) {
                (None, None) => {
                    if let Some((_, _, false)) = pending.take() {
                        on_event(Hold::Cancel); // let go before it counted as a hold
                    }
                }
                (None, Some(a)) => match pending.as_mut() {
                    Some((m, since, cancelled)) if *m == a => {
                        if !*cancelled && other_key_down() {
                            *cancelled = true;
                            on_event(Hold::Cancel); // it's a normal shortcut like Ctrl+Shift+T
                        } else if !*cancelled && since.elapsed() >= DELAY {
                            state = Some(a);
                            on_event(Hold::Confirm(a));
                        }
                    }
                    _ => {
                        if let Some((_, _, false)) = pending.take() {
                            on_event(Hold::Cancel);
                        }
                        let cancelled = other_key_down();
                        pending = Some((a, Instant::now(), cancelled));
                        if !cancelled {
                            on_event(Hold::Press);
                        }
                    }
                },
                (Some(s), a) if a != Some(s) => {
                    on_event(Hold::Release(s));
                    state = None;
                    pending = None;
                }
                _ => {}
            }
            thread::sleep(Duration::from_millis(20));
        }
    });
}
