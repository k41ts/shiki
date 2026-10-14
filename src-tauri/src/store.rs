//! To-do list and scheduled tasks, stored as JSON in the data dir.
use crate::config::data_dir;
use chrono::{Datelike, Duration, Local, NaiveDateTime, NaiveTime, Weekday};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());
const TIME_FMT: &str = "%Y-%m-%dT%H:%M";

#[derive(Serialize, Deserialize, Clone)]
pub struct Todo {
    pub text: String,
    #[serde(default)]
    pub due: String,
    #[serde(default)]
    pub reminded: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct Task {
    pub command: String,
    pub next: String,
    pub repeat: String,
}

fn read<T: DeserializeOwned>(name: &str) -> Vec<T> {
    std::fs::read_to_string(data_dir().join(name))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write<T: Serialize>(name: &str, items: &[T]) {
    if let Ok(s) = serde_json::to_string_pretty(items) {
        let _ = std::fs::write(data_dir().join(name), s);
    }
}

// ---------- to-do ----------

pub fn todos() -> Vec<Todo> {
    read("todos.json")
}

pub fn add_todo(text: &str, due: &str) -> String {
    let _g = LOCK.lock().unwrap();
    let mut list = todos();
    list.push(Todo { text: text.into(), due: due.trim().into(), reminded: false });
    write("todos.json", &list);
    format!("Added '{text}' to the to-do list.")
}

pub fn complete_todo(number: usize) -> String {
    let _g = LOCK.lock().unwrap();
    let mut list = todos();
    if number == 0 || number > list.len() {
        return format!("There is no to-do number {number}.");
    }
    let done = list.remove(number - 1);
    write("todos.json", &list);
    format!("Done: {}.", done.text)
}

pub fn list_todos() -> String {
    let list = todos();
    if list.is_empty() {
        return "To-do list is empty.".into();
    }
    list.iter()
        .enumerate()
        .map(|(i, t)| if t.due.is_empty() { format!("{}. {}", i + 1, t.text) } else { format!("{}. {} ({})", i + 1, t.text, t.due) })
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------- scheduled tasks ----------

/// "YYYY-MM-DD HH:MM" or "HH:MM" (today). Returns (time, had_date).
pub fn parse_time(at: &str) -> Option<(NaiveDateTime, bool)> {
    let at = at.trim().replace('T', " ");
    if let Ok(dt) = NaiveDateTime::parse_from_str(&at, "%Y-%m-%d %H:%M") {
        return Some((dt, true));
    }
    let t = NaiveTime::parse_from_str(&at, "%H:%M").ok()?;
    Some((Local::now().date_naive().and_time(t), false))
}

fn next_run(mut dt: NaiveDateTime, repeat: &str, now: NaiveDateTime) -> NaiveDateTime {
    let weekend = |d: &NaiveDateTime| matches!(d.weekday(), Weekday::Sat | Weekday::Sun);
    while dt <= now || (repeat == "weekdays" && weekend(&dt)) {
        dt += Duration::days(1);
    }
    dt
}

pub fn tasks() -> Vec<Task> {
    read("tasks.json")
}

pub fn schedule(command: &str, at: &str, repeat: &str) -> String {
    if !["once", "daily", "weekdays"].contains(&repeat) {
        return "repeat must be once, daily or weekdays.".into();
    }
    let Some((dt, has_date)) = parse_time(at) else {
        return format!("Could not understand time '{at}'. Use YYYY-MM-DD HH:MM or HH:MM.");
    };
    let now = Local::now().naive_local();
    if repeat == "once" && has_date && dt <= now {
        return "That time has already passed.".into();
    }
    let dt = next_run(dt, repeat, now);
    let _g = LOCK.lock().unwrap();
    let mut list = tasks();
    list.push(Task { command: command.into(), next: dt.format(TIME_FMT).to_string(), repeat: repeat.into() });
    write("tasks.json", &list);
    format!("Scheduled '{command}' for {} ({repeat}).", dt.format("%a %d %b %H:%M"))
}

pub fn cancel_task(number: usize) -> String {
    let _g = LOCK.lock().unwrap();
    let mut list = tasks();
    if number == 0 || number > list.len() {
        return format!("There is no scheduled task number {number}.");
    }
    let t = list.remove(number - 1);
    write("tasks.json", &list);
    format!("Cancelled '{}'.", t.command)
}

pub fn list_tasks() -> String {
    let list = tasks();
    if list.is_empty() {
        return "No scheduled tasks.".into();
    }
    list.iter()
        .enumerate()
        .map(|(i, t)| format!("{}. {} — {} ({})", i + 1, t.command, t.next.replace('T', " "), t.repeat))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Pops everything due: (commands to run, to-do reminders to show).
pub fn take_due() -> (Vec<String>, Vec<String>) {
    let _g = LOCK.lock().unwrap();
    let now = Local::now().naive_local();

    let list = tasks();
    let mut keep = Vec::new();
    let mut due = Vec::new();
    for mut t in list.clone() {
        let Ok(when) = NaiveDateTime::parse_from_str(&t.next, TIME_FMT) else { continue };
        if when <= now {
            // Repeating runs missed while the PC was off are skipped; one-offs always run.
            if t.repeat == "once" || now - when < Duration::minutes(10) {
                due.push(t.command.clone());
            }
            if t.repeat == "once" {
                continue;
            }
            t.next = next_run(when, &t.repeat, now).format(TIME_FMT).to_string();
        }
        keep.push(t);
    }
    if keep != list {
        write("tasks.json", &keep);
    }

    let mut todos = todos();
    let mut reminders = Vec::new();
    for t in todos.iter_mut().filter(|t| !t.due.is_empty() && !t.reminded) {
        if parse_time(&t.due).is_some_and(|(when, _)| when <= now) {
            t.reminded = true;
            reminders.push(t.text.clone());
        }
    }
    if !reminders.is_empty() {
        write("todos.json", &todos);
    }
    (due, reminders)
}
