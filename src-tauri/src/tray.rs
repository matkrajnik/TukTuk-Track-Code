//! Ikona w zasobniku: menu, tooltip z bieżącym pomiarem i celem dnia oraz
//! ikona z kropką, gdy pomiar trwa. Odświeżanie co 5 s i po `data-changed`.
use crate::{db, reminder, settings, stats, Database};
use chrono::Datelike;
use rusqlite::{Connection, OptionalExtension};
use std::{
    sync::{mpsc, Mutex},
    time::{Duration, Instant},
};
use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    Emitter, Listener, Manager, Wry,
};

/// Windows obcina dymek ikony do 127 znaków UTF-16.
pub const TOOLTIP_LIMIT: usize = 127;
/// Długość nazwy zadania w nieklikalnej pozycji menu.
const MENU_TITLE_LIMIT: usize = 60;
const REFRESH: Duration = Duration::from_secs(5);
/// Jak długo dymek pokazuje potwierdzenie Start/Stop zamiast stanu.
const FEEDBACK_HOLD: Duration = Duration::from_secs(10);

/// Stan pokazywany w trayu.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Status {
    /// Tytuł i czas trwania aktywnego pomiaru.
    pub active: Option<(String, i64)>,
    /// Suma pracy dzisiaj (dzielona przez lokalną północ).
    pub today_ms: i64,
    /// Cel na dziś w minutach; 0 dla dnia wolnego.
    pub target_minutes: u32,
}

/// „5:10” — godziny bez zera wiodącego, minuty w dół.
fn hm(ms: i64) -> String {
    let minutes = ms.max(0) / 60_000;
    format!("{}:{:02}", minutes / 60, minutes % 60)
}

/// „01:23:45”, jak zegar w aplikacji.
pub(crate) fn hms(ms: i64) -> String {
    let seconds = ms.max(0) / 1000;
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        (seconds / 60) % 60,
        seconds % 60
    )
}

fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// Przycina tekst do `limit` jednostek UTF-16, dodając wielokropek.
fn truncate(text: &str, limit: usize) -> String {
    if utf16_len(text) <= limit {
        return text.to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        if used + c.len_utf16() + 1 > limit {
            break;
        }
        used += c.len_utf16();
        out.push(c);
    }
    out.truncate(out.trim_end().len());
    out.push('…');
    out
}

/// Jedna linia z białymi znakami zwiniętymi do spacji.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Tooltip: `Zadanie X · 01:23:45 · dziś 5:10 / 8:00`, maksymalnie 127 znaków;
/// przycinany jest tylko tytuł.
pub fn tooltip(status: &Status) -> String {
    let mut today = format!("dziś {}", hm(status.today_ms));
    if status.target_minutes > 0 {
        today.push_str(&format!(" / {}", hm(status.target_minutes as i64 * 60_000)));
    }
    match &status.active {
        None => truncate(
            &format!("Todo Track · brak pomiaru · {today}"),
            TOOLTIP_LIMIT,
        ),
        Some((title, elapsed)) => {
            let rest = format!(" · {} · {today}", hms(*elapsed));
            let room = TOOLTIP_LIMIT.saturating_sub(utf16_len(&rest));
            let title = one_line(title);
            let title = if title.is_empty() {
                "Bez nazwy".to_string()
            } else {
                title
            };
            truncate(&format!("{}{rest}", truncate(&title, room)), TOOLTIP_LIMIT)
        }
    }
}

/// Tekst nieklikalnej pozycji menu z bieżącym zadaniem. `&` w Windows oznacza
/// klawisz skrótu, więc jest podwajany.
pub fn menu_label(active: Option<&str>) -> String {
    match active {
        None => "Brak pomiaru".into(),
        Some(title) => format!(
            "Mierzone: {}",
            truncate(&one_line(title), MENU_TITLE_LIMIT).replace('&', "&&")
        ),
    }
}

/// Czyta stan traya z bazy: aktywny pomiar, suma dnia (SQL, bez snapshotu) i cel.
pub fn read_status(
    db: &Connection,
    today: chrono::NaiveDate,
    now: i64,
) -> rusqlite::Result<Status> {
    let active = db
        .query_row(
            "SELECT t.title, s.started_at FROM sessions s JOIN tasks t ON t.id=s.task_id
             WHERE s.ended_at IS NULL ORDER BY s.started_at DESC LIMIT 1",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
        )
        .optional()?
        .map(|(title, started)| (title, (now - started).max(0)));
    let s = settings::load(db);
    let weekday = today.weekday().number_from_monday() as u8;
    Ok(Status {
        active,
        today_ms: stats::day_total_ms(db, today, now)?,
        target_minutes: if s.work_days.contains(&weekday) {
            s.target_day_minutes
        } else {
            0
        },
    })
}

struct TrayState {
    current: MenuItem<Wry>,
    idle_icon: Option<Image<'static>>,
    /// Potwierdzenie Start/Stop ze skrótu i chwila jego pokazania.
    feedback: Mutex<Option<(String, Instant)>>,
}

/// Dymek z potwierdzeniem Start/Stop: jedna linia, w limicie Windows.
pub fn feedback_tooltip(text: &str) -> String {
    truncate(&one_line(text), TOOLTIP_LIMIT)
}

/// Pokazuje potwierdzenie Start/Stop w dymku ikony przez `FEEDBACK_HOLD`; razem ze
/// zmianą ikony (kropka przy pomiarze) to jedyny ślad akcji przy schowanym oknie,
/// dopóki widget (B) i powiadomienia (J) nie słuchają `toggle-feedback`.
pub fn show_feedback(app: &tauri::AppHandle, text: &str) {
    let text = feedback_tooltip(text);
    if let Some(state) = app.try_state::<TrayState>() {
        if let Ok(mut f) = state.feedback.lock() {
            *f = Some((text.clone(), Instant::now()));
        }
    }
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(text));
    }
}

fn active_icon() -> Image<'static> {
    tauri::include_image!("./icons/icon-active.ico")
}

fn stop_measurement(app: &tauri::AppHandle, quit: bool) {
    let state = app.state::<Database>();
    let result = state
        .0
        .lock()
        .map_err(|e| e.to_string())
        .and_then(|db| db::stop(&db, db::now()).map_err(|e| e.to_string()));
    match result {
        Ok(()) => {
            reminder::reset(app);
            let _ = app.emit("data-changed", ());
            if quit {
                crate::window_state::save(app);
                app.exit(0);
            }
        }
        Err(e) => {
            crate::show(app);
            let _ = app.emit("storage-error", e);
        }
    }
}

/// Buduje ikonę traya i uruchamia wątek odświeżający.
pub fn setup(app: &tauri::App) -> tauri::Result<()> {
    let current = MenuItem::with_id(app, "current", "Brak pomiaru", false, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Otwórz Todo Track", true, None::<&str>)?;
    let quick = MenuItem::with_id(app, "quick", "Szybkie przełączanie", true, None::<&str>)?;
    let stop = MenuItem::with_id(app, "stop", "Zatrzymaj pomiar", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Zakończ aplikację", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &current,
            &PredefinedMenuItem::separator(app)?,
            &open,
            &quick,
            &stop,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;
    let idle_icon = app.default_window_icon().map(|i| i.clone().to_owned());
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("Todo Track")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                crate::show(tray.app_handle());
            }
        })
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => crate::show(app),
            "quick" => {
                if let Err(e) = crate::quick_open(app.clone()) {
                    crate::show(app);
                    let _ = app.emit("storage-error", e);
                }
            }
            "stop" => stop_measurement(app, false),
            "quit" => stop_measurement(app, true),
            _ => {}
        });
    if let Some(icon) = idle_icon.clone() {
        builder = builder.icon(icon);
    }
    builder.build(app)?;

    let (wake, signal) = mpsc::channel();
    app.manage(TrayState {
        current,
        idle_icon,
        feedback: Mutex::new(None),
    });
    for event in ["data-changed", "settings-changed"] {
        let wake = wake.clone();
        // Zdarzenie bywa emitowane pod blokadą bazy, więc tylko budzimy wątek.
        app.listen_any(event, move |_| {
            let _ = wake.send(());
        });
    }
    let handle = app.handle().clone();
    std::thread::spawn(move || {
        let mut last: Option<(bool, String)> = None;
        loop {
            if handle.get_webview_window("main").is_none() {
                break;
            }
            refresh(&handle, &mut last);
            match signal.recv_timeout(REFRESH) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });
    Ok(())
}

fn refresh(app: &tauri::AppHandle, last: &mut Option<(bool, String)>) {
    let status = {
        let database = app.state::<Database>();
        let Ok(db) = database.0.lock() else {
            return;
        };
        match read_status(&db, chrono::Local::now().date_naive(), db::now()) {
            Ok(status) => status,
            Err(_) => return,
        }
    };
    let Some(tray): Option<TrayIcon> = app.tray_by_id("main") else {
        return;
    };
    let state = app.state::<TrayState>();
    let held = state.feedback.lock().ok().and_then(|f| {
        f.as_ref()
            .filter(|(_, at)| at.elapsed() < FEEDBACK_HOLD)
            .cloned()
    });
    let text = held.map_or_else(|| tooltip(&status), |(text, _)| text);
    let _ = tray.set_tooltip(Some(text));
    let active = status.active.is_some();
    let label = menu_label(status.active.as_ref().map(|(t, _)| t.as_str()));
    if last.as_ref().map(|(a, _)| *a) != Some(active) {
        let icon = if active {
            Some(active_icon())
        } else {
            state.idle_icon.clone()
        };
        let _ = tray.set_icon(icon);
    }
    if last.as_ref().map(|(_, l)| l) != Some(&label) {
        let _ = state.current.set_text(&label);
    }
    *last = Some((active, label));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(title: Option<&str>, elapsed: i64, today: i64, target: u32) -> Status {
        Status {
            active: title.map(|t| (t.to_string(), elapsed)),
            today_ms: today,
            target_minutes: target,
        }
    }
    const MIN: i64 = 60_000;

    #[test]
    fn tooltip_shows_task_clock_and_target() {
        let s = status(
            Some("Zadanie X"),
            (3600 + 23 * 60 + 45) * 1000,
            (5 * 60 + 10) * MIN + 59_000,
            480,
        );
        assert_eq!(tooltip(&s), "Zadanie X · 01:23:45 · dziś 5:10 / 8:00");
        assert_eq!(
            tooltip(&status(None, 0, 0, 450)),
            "Todo Track · brak pomiaru · dziś 0:00 / 7:30"
        );
        // Dzień wolny: bez celu.
        assert_eq!(
            tooltip(&status(Some("A"), 5_000, 61 * MIN, 0)),
            "A · 00:00:05 · dziś 1:01"
        );
        // Ponad 24 h pomiaru i nowe linie w tytule.
        assert_eq!(
            tooltip(&status(Some(" Raport\n  TS "), 26 * 3600 * 1000, 0, 480)),
            "Raport TS · 26:00:00 · dziś 0:00 / 8:00"
        );
        assert_eq!(
            tooltip(&status(Some("   "), 0, 0, 0)),
            "Bez nazwy · 00:00:00 · dziś 0:00"
        );
    }

    #[test]
    fn long_titles_are_cut_to_the_windows_limit() {
        let title = "Ą".repeat(300);
        let text = tooltip(&status(Some(&title), 1000, 8 * 60 * MIN, 480));
        assert_eq!(text.encode_utf16().count(), TOOLTIP_LIMIT);
        assert!(text.ends_with("… · 00:00:01 · dziś 8:00 / 8:00"));
        assert!(text.starts_with("ĄĄĄ"));
        // Znaki spoza BMP liczą się podwójnie i nie są rozcinane.
        let emoji = "😀".repeat(100);
        let text = tooltip(&status(Some(&emoji), 0, 0, 480));
        assert!(text.encode_utf16().count() <= TOOLTIP_LIMIT);
        assert!(text.contains("😀…"));
        // Tytuł mieszczący się w limicie nie jest przycinany.
        let fits = "a".repeat(TOOLTIP_LIMIT - utf16_len(" · 00:00:00 · dziś 0:00"));
        let text = tooltip(&status(Some(&fits), 0, 0, 0));
        assert_eq!(text.encode_utf16().count(), TOOLTIP_LIMIT);
        assert!(!text.contains('…'));
    }

    #[test]
    fn feedback_tooltip_is_one_line_within_limit() {
        assert_eq!(feedback_tooltip("Start: Raport\n TS"), "Start: Raport TS");
        let long = feedback_tooltip(&format!("Stop: {} · 01:23:45", "x".repeat(300)));
        assert_eq!(long.encode_utf16().count(), TOOLTIP_LIMIT);
        assert!(long.ends_with('…'));
    }

    #[test]
    fn menu_label_escapes_ampersand_and_truncates() {
        assert_eq!(menu_label(None), "Brak pomiaru");
        assert_eq!(menu_label(Some("R&D")), "Mierzone: R&&D");
        let label = menu_label(Some(&"x".repeat(200)));
        assert!(label.ends_with('…'));
        assert_eq!(label.chars().count(), "Mierzone: ".len() + MENU_TITLE_LIMIT);
    }

    #[test]
    fn status_reads_active_session_day_total_and_work_day_target() {
        use chrono::{Local, NaiveDate, TimeZone};
        let mut db = Connection::open_in_memory().unwrap();
        db::init(&db).unwrap();
        db::change(&mut db, "add", None, Some("Import".into()), 0).unwrap();
        // Środa 14 października 2026 i sobota 17 października 2026.
        let wednesday = NaiveDate::from_ymd_opt(2026, 10, 14).unwrap();
        let saturday = NaiveDate::from_ymd_opt(2026, 10, 17).unwrap();
        let at = |d: NaiveDate, h: u32| {
            Local
                .from_local_datetime(&d.and_hms_opt(h, 0, 0).unwrap())
                .earliest()
                .unwrap()
                .timestamp_millis()
        };
        let none = read_status(&db, wednesday, at(wednesday, 12)).unwrap();
        assert_eq!(none, status(None, 0, 0, 480));
        db.execute(
            "INSERT INTO sessions(task_id,started_at,ended_at) VALUES(1,?1,?2),(1,?3,NULL)",
            rusqlite::params![at(wednesday, 8), at(wednesday, 9), at(wednesday, 10)],
        )
        .unwrap();
        let s = read_status(&db, wednesday, at(wednesday, 12)).unwrap();
        assert_eq!(s, status(Some("Import"), 2 * 60 * MIN, 3 * 60 * MIN, 480));
        assert_eq!(tooltip(&s), "Import · 02:00:00 · dziś 3:00 / 8:00");
        settings::save(&mut db, &serde_json::json!({"targetDayMinutes": 360})).unwrap();
        assert_eq!(
            read_status(&db, saturday, at(saturday, 12))
                .unwrap()
                .target_minutes,
            0
        );
        assert_eq!(
            read_status(&db, wednesday, at(wednesday, 12))
                .unwrap()
                .target_minutes,
            360
        );
    }
}
