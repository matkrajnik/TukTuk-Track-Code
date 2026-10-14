use crate::{db, Database};
use serde::Serialize;
use std::sync::Mutex;
use tauri::{Emitter, Manager};
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub session_id: i64,
    pub task_id: i64,
    pub idle_at: i64,
    pub returned_at: i64,
}
#[derive(Default)]
pub struct State(Mutex<Clock>);
#[derive(Default)]
struct Clock {
    candidate: Option<(i64, i64, i64)>,
    pending: Option<Notice>,
}
impl Clock {
    fn observe(&mut self, session: Option<(i64, i64, i64)>, now: i64, idle_ms: u32) -> bool {
        let Some((id, task, start)) = session else {
            self.candidate = None;
            return false;
        };
        if idle_ms >= 600_000 {
            if self.candidate.as_ref().map(|v| v.0) != Some(id) {
                self.candidate = Some((id, task, (now - i64::from(idle_ms)).max(start)));
            }
        } else if let Some((old_id, task, at)) = self.candidate.take() {
            if old_id == id && self.pending.is_none() && now - at >= 600_000 {
                self.pending = Some(Notice {
                    session_id: id,
                    task_id: task,
                    idle_at: at,
                    returned_at: now,
                });
                return true;
            }
        }
        false
    }
}
#[cfg(windows)]
fn idle_ms() -> Option<u32> {
    #[repr(C)]
    struct LastInput {
        size: u32,
        time: u32,
    }
    #[link(name = "user32")]
    extern "system" {
        fn GetLastInputInfo(info: *mut LastInput) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetTickCount() -> u32;
    }
    let mut input = LastInput {
        size: std::mem::size_of::<LastInput>() as u32,
        time: 0,
    };
    unsafe {
        if GetLastInputInfo(&mut input) == 0 {
            None
        } else {
            Some(GetTickCount().wrapping_sub(input.time))
        }
    }
}
#[cfg(not(windows))]
fn idle_ms() -> Option<u32> {
    None
}

/// Rejestruje stan bezczynności; wołać przed utworzeniem okien.
pub fn setup(app: &tauri::AppHandle) {
    app.manage(State::default());
}

/// Pętla co 2 s sprawdzająca bezczynność. Kończy się, gdy okno main zniknie,
/// więc wołać po `create_windows`.
pub fn start(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(2));
        let Some(_) = app.get_webview_window("main") else {
            break;
        };
        let enabled = app
            .state::<Database>()
            .0
            .lock()
            .ok()
            .map(|db| crate::settings::load(&db).idle_enabled)
            .unwrap_or(false);
        if !enabled {
            if let Ok(mut state) = app.state::<State>().0.lock() {
                *state = Clock::default();
            }
            continue;
        }
        let Some(idle) = idle_ms() else {
            continue;
        };
        let session = app.state::<Database>().0.lock().ok().and_then(|db| {
            db.query_row(
                "SELECT id,task_id,started_at FROM sessions WHERE ended_at IS NULL",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .ok()
        });
        let due = app
            .state::<State>()
            .0
            .lock()
            .map(|mut c| c.observe(session, db::now(), idle))
            .unwrap_or(false);
        if due {
            let _ = app.emit("idle-notice", ());
            crate::show(&app);
        }
    });
}
#[tauri::command]
pub fn idle_notice(app: tauri::AppHandle) -> Option<Notice> {
    app.state::<State>()
        .0
        .lock()
        .ok()
        .and_then(|c| c.pending.clone())
}
pub fn clear(app: &tauri::AppHandle) {
    if let Ok(mut state) = app.state::<State>().0.lock() {
        *state = Clock::default();
    }
    let _ = app.emit("idle-notice", ());
}
#[tauri::command]
pub fn idle_resolve(app: tauri::AppHandle, trim: bool) -> Result<db::Snapshot, String> {
    let state = app.state::<State>();
    let mut clock = state.0.lock().map_err(|e| e.to_string())?;
    let notice = clock
        .pending
        .clone()
        .ok_or("Brak zgłoszonej bezczynności.")?;
    let database = app.state::<Database>();
    let conn = database.0.lock().map_err(|e| e.to_string())?;
    if trim {
        conn.execute("UPDATE sessions SET ended_at=MAX(started_at,?1) WHERE id=?2 AND (ended_at IS NULL OR ended_at>?1)",rusqlite::params![notice.idle_at,notice.session_id]).map_err(|e|e.to_string())?;
    }
    clock.pending = None;
    let result = db::snapshot(&conn).map_err(|e| e.to_string())?;
    let _ = app.emit("data-changed", ());
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_prompts_after_return_without_mutating_time() {
        let mut c = Clock::default();
        assert!(!c.observe(Some((1, 2, 100)), 700_000, 650_000));
        assert!(c.observe(Some((1, 2, 100)), 800_000, 0));
        assert_eq!(c.pending.as_ref().unwrap().idle_at, 50_000);
        assert!(!c.observe(Some((1, 2, 100)), 800_010, 10));
    }
    #[test]
    fn short_break_and_switching_sessions_do_not_prompt() {
        let mut c = Clock::default();
        assert!(!c.observe(Some((1, 2, 0)), 500_000, 499_999));
        assert!(!c.observe(Some((1, 2, 0)), 500_001, 0));
        assert!(c.pending.is_none());
        c.observe(Some((1, 2, 0)), 700_000, 650_000);
        assert!(!c.observe(Some((2, 3, 700_000)), 800_000, 0));
        assert!(c.pending.is_none());
        c.observe(Some((2, 3, 700_000)), 1_400_000, 650_000);
        c.observe(None, 1_500_000, 0);
        assert!(!c.observe(Some((3, 4, 1_500_000)), 1_500_100, 0));
    }
}
