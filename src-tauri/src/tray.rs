//! Ikona w zasobniku: menu, tooltip z bieżącym pomiarem i celem dnia oraz
//! ikona z kropką, gdy pomiar trwa. Odświeżanie co 5 s i po `data-changed`.
use crate::{db, reminder, settings, stats, Database};
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
    /// Godzina „Zakończ dzień” (HH:MM), gdy dzisiejszy dzień jest zakończony (B13).
    pub day_ended: Option<String>,
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
        None => match &status.day_ended {
            Some(at) => truncate(
                &format!("Todo Track · {}", day_ended_label(at, status.today_ms)),
                TOOLTIP_LIMIT,
            ),
            None => truncate(
                &format!("Todo Track · brak pomiaru · {today}"),
                TOOLTIP_LIMIT,
            ),
        },
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

/// „Dzień zakończony 16:40 · 7:55” (B13): godzina zakończenia i suma dnia.
pub fn day_ended_label(at: &str, today_ms: i64) -> String {
    format!("Dzień zakończony {at} · {}", hm(today_ms))
}

/// Pozycja menu dnia: „Zakończ dzień” albo, po zakończeniu, „Wznów dzień”.
pub fn day_item_label(status: &Status) -> &'static str {
    if status.day_ended.is_some() {
        "Wznów dzień"
    } else {
        "Zakończ dzień"
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

/// Przypięte zadanie („w toku”) jako pozycja menu: przełączenie jednym kliknięciem.
#[derive(Debug, Clone, PartialEq)]
pub struct Pin {
    pub task_id: i64,
    pub title: String,
    /// Klucz zgłoszenia do wyświetlenia: z Jiry (`WMS-12`) albo z Redmine
    /// (`#1234`); zadanie ma najwyżej jedno zgłoszenie.
    pub jira_key: Option<String>,
    /// Zadanie jest teraz mierzone (pozycja nieaktywna).
    pub current: bool,
}

/// „▶ WMS-12 · Poprawka logowania” dla mierzonego, „WMS-12 · …” (albo
/// „#1234 · …” dla Redmine) dla pozostałych; `&` podwojone jak w `menu_label`.
pub fn pin_label(pin: &Pin) -> String {
    let title = one_line(&pin.title);
    let text = match &pin.jira_key {
        Some(key) => format!("{key} · {title}"),
        None => title,
    };
    let text = truncate(&text, MENU_TITLE_LIMIT).replace('&', "&&");
    if pin.current {
        format!("▶ {text}")
    } else {
        text
    }
}

/// Przypięte otwarte zadania w kolejności przypięcia (`pinnedTasks` w widgecie).
pub fn read_pins(db: &Connection) -> rusqlite::Result<Vec<Pin>> {
    db.prepare(
        "SELECT t.id, t.title, COALESCE(j.issue_key, '#' || r.issue_id),
                EXISTS(SELECT 1 FROM sessions s WHERE s.task_id=t.id AND s.ended_at IS NULL)
         FROM tasks t JOIN task_details d ON d.task_id=t.id
         LEFT JOIN jira_tasks j ON j.task_id=t.id
         LEFT JOIN redmine_tasks r ON r.task_id=t.id
         LEFT JOIN task_lifecycle l ON l.task_id=t.id
         WHERE t.done=0 AND COALESCE(l.archived,0)=0 AND l.deleted_at IS NULL
           AND COALESCE(json_extract(d.json,'$.pinnedAt'),0)>0
         ORDER BY json_extract(d.json,'$.pinnedAt'), t.id
         LIMIT ?1",
    )?
    .query_map([crate::planner::MAX_PINNED as i64], |r| {
        Ok(Pin {
            task_id: r.get(0)?,
            title: r.get(1)?,
            jira_key: r.get(2)?,
            current: r.get(3)?,
        })
    })?
    .collect()
}

/// Czyta stan traya z bazy: aktywny pomiar, suma dnia (SQL, bez snapshotu) i cel.
#[cfg(test)]
pub fn read_status(
    db: &Connection,
    today: chrono::NaiveDate,
    now: i64,
) -> rusqlite::Result<Status> {
    read_status_with(db, &settings::load(db), today, now)
}

/// `read_status` z podanymi ustawieniami (w aplikacji z cache, B6).
pub fn read_status_with(
    db: &Connection,
    s: &settings::AppSettings,
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
    let day_ended = crate::days::get(db, &today.format("%Y-%m-%d").to_string())?
        .and_then(|m| m.ended_at)
        .and_then(|at| chrono::TimeZone::timestamp_millis_opt(&chrono::Local, at).single())
        .map(|at| at.format("%H:%M").to_string());
    Ok(Status {
        active,
        day_ended,
        today_ms: stats::day_total_ms(db, today, now)?,
        // Cel dnia minus spotkania niepokryte pomiarem (`capacity::goal_ms_in`).
        target_minutes: (crate::capacity::goal_ms_in(db, &chrono::Local, s, today, now)? / 60_000)
            as u32,
    })
}

/// Pozycja pierwszej przypiętej pozycji w menu (po „Mierzone…” i separatorze).
const PINS_AT: usize = 2;

struct TrayState {
    current: MenuItem<Wry>,
    /// „Zakończ dzień” / „Wznów dzień” (B13).
    day: MenuItem<Wry>,
    menu: Menu<Wry>,
    /// Liczba pozycji wstawionych dla przypiętych (z separatorem za nimi).
    pin_items: Mutex<usize>,
    idle_icon: Option<Image<'static>>,
    /// Potwierdzenie Start/Stop ze skrótu i chwila jego pokazania.
    feedback: Mutex<Option<(String, Instant)>>,
}

/// Dymek z potwierdzeniem Start/Stop: jedna linia, w limicie Windows.
pub fn feedback_tooltip(text: &str) -> String {
    truncate(&one_line(text), TOOLTIP_LIMIT)
}

/// Pokazuje potwierdzenie Start/Stop w dymku ikony przez `FEEDBACK_HOLD`; razem ze
/// zmianą ikony (kropka przy pomiarze) to ślad akcji przy schowanym oknie, gdy widget
/// (który słucha `toggle-feedback`) jest wyłączony; obok idzie powiadomienie Windows (J).
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

/// Klik w przypięte zadanie: poprzedni pomiar staje, startuje wybrane (jak `mutate`).
/// Po zakończeniu dnia (B13) najpierw pytanie „Wznowić dzień?” w głównym oknie.
fn start_pinned(app: &tauri::AppHandle, task_id: i64) {
    if crate::days::start_blocked_now(app) {
        crate::days::ask_resume(app, crate::days::Pending::Task(task_id));
        return;
    }
    start_task(app, task_id);
}

/// Start zadania z dymkiem „Start: X” (przypięte w trayu, start po „Wznów i start”).
pub fn start_task(app: &tauri::AppHandle, task_id: i64) {
    let state = app.state::<Database>();
    let result = state
        .0
        .lock()
        .map_err(|e| e.to_string())
        .and_then(|mut db| {
            db::change(&mut db, "start", Some(task_id), None, db::now()).map(|s| {
                s.tasks
                    .into_iter()
                    .find(|t| t.id == task_id)
                    .map(|t| t.title)
                    .unwrap_or_default()
            })
        });
    match result {
        Ok(title) => {
            reminder::reset(app);
            let _ = app.emit("data-changed", ());
            let text = format!("Start: {title}");
            show_feedback(app, &text);
            let _ = app.emit("toggle-feedback", text);
        }
        Err(e) => {
            crate::show(app);
            let _ = app.emit("storage-error", e);
        }
    }
}

/// Wymienia pozycje przypiętych zadań w menu (z separatorem za nimi, gdy są).
fn set_pins(app: &tauri::AppHandle, state: &TrayState, pins: &[Pin]) -> tauri::Result<()> {
    let Ok(mut count) = state.pin_items.lock() else {
        return Ok(());
    };
    for _ in 0..*count {
        state.menu.remove_at(PINS_AT)?;
    }
    *count = 0;
    for (i, pin) in pins.iter().enumerate() {
        let item = MenuItem::with_id(
            app,
            format!("pin:{}", pin.task_id),
            pin_label(pin),
            !pin.current,
            None::<&str>,
        )?;
        state.menu.insert(&item, PINS_AT + i)?;
        *count += 1;
    }
    if !pins.is_empty() {
        state
            .menu
            .insert(&PredefinedMenuItem::separator(app)?, PINS_AT + pins.len())?;
        *count += 1;
    }
    Ok(())
}

/// Sygnały odświeżenia zebrane od `setup` do `start`.
pub struct Refresher(mpsc::Receiver<()>);

/// Buduje ikonę traya, rejestruje `TrayState` i nasłuch zdarzeń. Wątek
/// odświeżający uruchamia dopiero `start`, po utworzeniu okien.
pub fn setup(app: &tauri::App) -> tauri::Result<Refresher> {
    let current = MenuItem::with_id(app, "current", "Brak pomiaru", false, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Otwórz Todo Track", true, None::<&str>)?;
    let quick = MenuItem::with_id(app, "quick", "Szybkie przełączanie", true, None::<&str>)?;
    let stop = MenuItem::with_id(app, "stop", "Zatrzymaj pomiar", true, None::<&str>)?;
    let day = MenuItem::with_id(app, "day", "Zakończ dzień", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Zakończ aplikację", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &current,
            &PredefinedMenuItem::separator(app)?,
            &open,
            &quick,
            &stop,
            &day,
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
            // „Zakończ dzień” pyta w głównym oknie o zatrzymanie pomiaru (B13);
            // „Wznów dzień” nie zmienia pomiaru, więc działa od razu.
            "day" => {
                let ended = app
                    .state::<Database>()
                    .0
                    .lock()
                    .map(|db| crate::days::ended_now(&db))
                    .unwrap_or(false);
                if ended {
                    crate::days::resume_now(app);
                } else {
                    crate::days::request_end(app);
                }
            }
            "quit" => stop_measurement(app, true),
            id => {
                if let Some(task_id) = id.strip_prefix("pin:").and_then(|t| t.parse().ok()) {
                    start_pinned(app, task_id);
                }
            }
        });
    if let Some(icon) = idle_icon.clone() {
        builder = builder.icon(icon);
    }
    builder.build(app)?;

    let (wake, signal) = mpsc::channel();
    app.manage(TrayState {
        current,
        day,
        menu,
        pin_items: Mutex::new(0),
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
    Ok(Refresher(signal))
}

/// Wątek odświeżający tooltip, ikonę i menu co 5 s i po zdarzeniach. Kończy
/// się, gdy okno main zniknie (zamykanie aplikacji), więc wołać po
/// `create_windows`: wcześniej wątek zakończyłby się od razu.
pub fn start(app: &tauri::AppHandle, refresher: Refresher) {
    let Refresher(signal) = refresher;
    let handle = app.clone();
    std::thread::spawn(move || {
        let mut last: Option<(bool, String)> = None;
        let mut last_pins: Option<Vec<Pin>> = None;
        loop {
            if handle.get_webview_window("main").is_none() {
                break;
            }
            refresh(&handle, &mut last, &mut last_pins);
            match signal.recv_timeout(REFRESH) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });
}

fn refresh(
    app: &tauri::AppHandle,
    last: &mut Option<(bool, String)>,
    last_pins: &mut Option<Vec<Pin>>,
) {
    let (status, pins) = {
        let database = app.state::<Database>();
        let Ok(db) = database.0.lock() else {
            return;
        };
        let s = settings::cached(app, &db);
        match read_status_with(&db, &s, chrono::Local::now().date_naive(), db::now()) {
            Ok(status) => (status, read_pins(&db).ok()),
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
    let label = match (&status.active, &status.day_ended) {
        (None, Some(at)) => day_ended_label(at, status.today_ms),
        (active, _) => menu_label(active.as_ref().map(|(t, _)| t.as_str())),
    };
    let _ = state.day.set_text(day_item_label(&status));
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
    if let Some(pins) = pins.filter(|p| last_pins.as_ref() != Some(p)) {
        if set_pins(app, &state, &pins).is_ok() {
            *last_pins = Some(pins);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(title: Option<&str>, elapsed: i64, today: i64, target: u32) -> Status {
        Status {
            active: title.map(|t| (t.to_string(), elapsed)),
            today_ms: today,
            target_minutes: target,
            ..Status::default()
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

    /// B13: po „Zakończ dzień” dymek i menu mówią „Dzień zakończony 16:40 · 7:55”,
    /// a pozycja dnia zmienia się na „Wznów dzień”; trwający pomiar ma pierwszeństwo.
    #[test]
    fn ended_day_shows_end_time_and_resume_item() {
        let ended = Status {
            day_ended: Some("16:40".into()),
            ..status(None, 0, (7 * 60 + 55) * MIN, 480)
        };
        assert_eq!(
            tooltip(&ended),
            "Todo Track · Dzień zakończony 16:40 · 7:55"
        );
        assert_eq!(day_item_label(&ended), "Wznów dzień");
        assert_eq!(day_item_label(&status(None, 0, 0, 480)), "Zakończ dzień");
        let measuring = Status {
            day_ended: Some("16:40".into()),
            ..status(Some("A"), 5_000, 61 * MIN, 0)
        };
        assert_eq!(tooltip(&measuring), "A · 00:00:05 · dziś 1:01");
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
    fn pins_are_read_in_pin_order_with_jira_key_and_current_mark() {
        let mut db = Connection::open_in_memory().unwrap();
        db::init(&db).unwrap();
        for title in [
            "Przegląd kodu",
            "R&D logowania",
            "Ukończone",
            "Bez przypięcia",
        ] {
            db::change(&mut db, "add", None, Some(title.into()), 0).unwrap();
        }
        db.execute(
            "INSERT INTO jira_tasks(task_id,site,issue_key) VALUES(2,'https://x.atlassian.net','WMS-12')",
            [],
        )
        .unwrap();
        for (id, at) in [(2, 10), (1, 20), (3, 30)] {
            crate::planner::set_pinned(&mut db, id, true, at).unwrap();
        }
        db::change(&mut db, "toggle", Some(3), None, 40).unwrap();
        db::change(&mut db, "start", Some(1), None, 50).unwrap();
        let pins = read_pins(&db).unwrap();
        let labels: Vec<_> = pins.iter().map(pin_label).collect();
        assert_eq!(labels, ["WMS-12 · R&&D logowania", "▶ Przegląd kodu"]);
        assert_eq!(
            pins.iter()
                .map(|p| (p.task_id, p.current))
                .collect::<Vec<_>>(),
            [(2, false), (1, true)]
        );
        let long = pin_label(&Pin {
            task_id: 9,
            title: "x".repeat(200),
            jira_key: None,
            current: false,
        });
        assert_eq!(long.chars().count(), MENU_TITLE_LIMIT);
        assert!(long.ends_with('…'));
    }

    #[test]
    fn pins_show_redmine_number_like_jira_key() {
        let mut db = Connection::open_in_memory().unwrap();
        db::init(&db).unwrap();
        db::change(&mut db, "add", None, Some("Import CSV".into()), 0).unwrap();
        db.execute(
            "INSERT INTO redmine_tasks(task_id,site,issue_id) VALUES(1,'https://redmine.example.pl',1234)",
            [],
        )
        .unwrap();
        crate::planner::set_pinned(&mut db, 1, true, 10).unwrap();
        let pins = read_pins(&db).unwrap();
        assert_eq!(pins[0].jira_key.as_deref(), Some("#1234"));
        assert_eq!(pin_label(&pins[0]), "#1234 · Import CSV");
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
        // Zakończony dzień: godzina zakończenia w lokalnym czasie (B13).
        db::stop(&db, at(wednesday, 16)).unwrap();
        crate::days::end(
            &mut db,
            &Local,
            "2026-10-14",
            wednesday,
            at(wednesday, 16),
            None,
            false,
        )
        .unwrap();
        assert_eq!(
            read_status(&db, wednesday, at(wednesday, 17))
                .unwrap()
                .day_ended
                .as_deref(),
            Some("16:00")
        );
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
