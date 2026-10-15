use crate::{db, Database};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};

pub struct ReminderState {
    clock: Mutex<ReminderClock>,
    origin: Instant,
}
#[derive(Default)]
struct ReminderClock {
    since: Option<u64>,
    notified: bool,
}
impl ReminderClock {
    fn observe(&mut self, now: u64, active: bool, background: bool, delay: u64) -> bool {
        if active || !background {
            *self = Self::default();
            return false;
        }
        let since = *self.since.get_or_insert(now);
        if !self.notified && now.saturating_sub(since) >= delay {
            self.notified = true;
            return true;
        }
        false
    }
    fn snooze(&mut self, now: u64) {
        self.since = Some(now);
        self.notified = false;
    }
}
fn display(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("reminder") {
        if let Ok(Some(monitor)) = app
            .get_webview_window("main")
            .and_then(|w| w.current_monitor().ok())
            .map(Ok)
            .unwrap_or_else(|| window.primary_monitor())
        {
            let scale = monitor.scale_factor();
            let area = monitor.work_area();
            let x = area.position.x + area.size.width as i32 - (390.0 * scale) as i32;
            let y = area.position.y + area.size.height as i32 - (230.0 * scale) as i32;
            let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
        }
        let _ = app.emit_to("reminder", "reminder-shown", ());
        let _ = window.show();
    }
}
/// Rejestruje stan przypomnienia; wołać przed utworzeniem okien.
pub fn setup(app: &tauri::AppHandle) {
    app.manage(ReminderState {
        clock: Mutex::new(ReminderClock::default()),
        origin: Instant::now(),
    });
}

/// Pętla co 2 s (przypomnienie, widget w trybie „tylko w tle”). Kończy się,
/// gdy okno main zniknie, więc wołać po `create_windows`.
pub fn start(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let delay = 300;
        #[cfg(debug_assertions)]
        let delay = std::env::var("TODO_TRACK_REMINDER_SECONDS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(delay);
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let Some(main) = app.get_webview_window("main") else {
                break;
            };
            let active = match app.state::<Database>().0.lock() {
                Ok(db) => db
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM sessions WHERE ended_at IS NULL)",
                        [],
                        |r| r.get::<_, bool>(0),
                    )
                    .unwrap_or(true),
                Err(_) => true,
            };
            // Blok skupienia i przerwa po nim: przypomnienie o braku pomiaru milczy.
            let active = active || crate::focus::quiet(&app);
            let background =
                !main.is_visible().unwrap_or(true) || !main.is_focused().unwrap_or(true);
            crate::widget::apply(&app, background || main.is_minimized().unwrap_or(false));
            let state = app.state::<ReminderState>();
            let due = state
                .clock
                .lock()
                .map(|mut c| c.observe(state.origin.elapsed().as_secs(), active, background, delay))
                .unwrap_or(false);
            if active || !background {
                if let Some(w) = app.get_webview_window("reminder") {
                    let _ = w.hide();
                }
            } else if due {
                display(&app);
            }
        }
    });
}
pub fn reset(app: &tauri::AppHandle) {
    if let Some(state) = app.try_state::<ReminderState>() {
        if let Ok(mut clock) = state.clock.lock() {
            *clock = ReminderClock::default();
        }
    }
    if let Some(w) = app.get_webview_window("reminder") {
        let _ = w.hide();
    }
}
#[tauri::command]
pub fn reminder_action(app: tauri::AppHandle, action: String) -> Result<(), String> {
    if !matches!(action.as_str(), "open" | "snooze") {
        return Err("Nieznana akcja.".into());
    }
    let state = app.state::<ReminderState>();
    state
        .clock
        .lock()
        .map_err(|e| e.to_string())?
        .snooze(state.origin.elapsed().as_secs());
    if let Some(w) = app.get_webview_window("reminder") {
        let _ = w.hide();
    }
    if action == "open" {
        crate::show(&app);
        app.emit_to("main", "prompt-task", db::now())
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reminds_once_after_five_background_minutes_and_snoozes() {
        let mut c = ReminderClock::default();
        assert!(!c.observe(10, false, true, 300));
        assert!(!c.observe(309, false, true, 300));
        assert!(c.observe(310, false, true, 300));
        assert!(!c.observe(900, false, true, 300));
        c.snooze(900);
        assert!(!c.observe(1199, false, true, 300));
        assert!(c.observe(1200, false, true, 300));
    }
    #[test]
    fn foreground_and_active_timer_reset_idle_period() {
        let mut c = ReminderClock::default();
        c.observe(0, false, true, 300);
        assert!(!c.observe(299, true, true, 300));
        assert!(!c.observe(500, false, true, 300));
        assert!(!c.observe(799, false, false, 300));
        assert!(!c.observe(1000, false, true, 300));
        assert!(c.observe(1300, false, true, 300));
    }
}
