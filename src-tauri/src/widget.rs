//! Mini-widget: małe okno zawsze na wierzchu z bieżącym pomiarem.
//!
//! O widoczności decyduje `apply`: ustawienia `widget_enabled` i
//! `widget_background_only`, schowanie przyciskiem X (do końca sesji aplikacji,
//! bez zmiany ustawienia) oraz stan głównego okna (tryb „tylko w tle”).
//! `apply` jest wołane przy starcie wątku (`start`), po `settings-changed`, po zmianie fokusu
//! głównego okna i co 2 s z pętli w `reminder.rs`.
use crate::settings::{self, AppSettings};
use serde::Serialize;
use std::sync::{mpsc, Mutex};
use tauri::{
    AppHandle, Emitter, Listener, LogicalSize, Manager, PhysicalPosition, PhysicalSize, Runtime,
};

pub const LABEL: &str = "widget";

/// Malutki widok (C2), rozmiar logiczny: sam czas z kropką stanu.
pub const COMPACT_SIZE: (f64, f64) = (96.0, 28.0);
/// Szerokość pełnego widoku (wysokość 64, a z przypiętymi 96, podaje frontend).
pub const FULL_WIDTH: f64 = 320.0;

/// Prostokąt okna albo obszaru roboczego w pikselach fizycznych.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Strona, od której okno rośnie: przy prawej krawędzi ekranu w lewo, przy
/// dolnej w górę.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Anchor {
    pub right: bool,
    pub bottom: bool,
}

/// Narożnik malutkiego okna najbliższy krawędziom obszaru roboczego: środek
/// okna w prawej (dolnej) połowie obszaru oznacza rozrost w lewo (w górę).
pub fn anchor(compact: Rect, work: Rect) -> Anchor {
    Anchor {
        right: 2 * compact.x + compact.w > 2 * work.x + work.w,
        bottom: 2 * compact.y + compact.h > 2 * work.y + work.h,
    }
}

/// Prostokąt powiększonego okna: rośnie od narożnika najbliższego krawędzi
/// ekranu (`anchor`), a gdy to nie wystarcza, jest przesuwany do obszaru
/// roboczego (bez paska zadań). `work` dotyczy monitora, na którym stoi okno,
/// `full_w` i `full_h` są już przeliczone na piksele fizyczne (skala monitora).
pub fn expanded_rect(compact: Rect, full_w: i32, full_h: i32, work: Rect) -> (Rect, Anchor) {
    let anchor = anchor(compact, work);
    // Okno większe od obszaru: przylega do początku obszaru.
    let fit = |start: i32, size: i32, lo: i32, span: i32| start.min(lo + span - size).max(lo);
    let x = if anchor.right {
        compact.x + compact.w - full_w
    } else {
        compact.x
    };
    let y = if anchor.bottom {
        compact.y + compact.h - full_h
    } else {
        compact.y
    };
    let rect = Rect {
        x: fit(x, full_w, work.x, work.w),
        y: fit(y, full_h, work.y, work.h),
        w: full_w,
        h: full_h,
    };
    (rect, anchor)
}

/// Malutkie okno wyliczone z położenia przesuniętego powiększonego okna
/// (użytkownik przeciągnął pełny widok): ten sam narożnik zostaje na miejscu.
pub fn collapsed_rect(placed: Rect, anchor: Anchor, compact_w: i32, compact_h: i32) -> Rect {
    Rect {
        x: if anchor.right {
            placed.x + placed.w - compact_w
        } else {
            placed.x
        },
        y: if anchor.bottom {
            placed.y + placed.h - compact_h
        } else {
            placed.y
        },
        w: compact_w,
        h: compact_h,
    }
}

/// Powiększony widok: gdzie stało malutkie okno (do niego wracamy po zwinięciu)
/// i gdzie umieszczono powiększone.
#[derive(Clone, Copy, Debug)]
struct Open {
    compact: Rect,
    placed: Rect,
    anchor: Anchor,
}

/// Stan widgetu widziany z frontendu (przełącznik w nagłówku głównego okna).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// Ustawienie `widget_enabled`.
    pub enabled: bool,
    /// Ustawienie `widget_background_only`.
    pub background_only: bool,
    /// Schowany przyciskiem X do końca sesji aplikacji.
    pub dismissed: bool,
}

impl Status {
    fn from_settings(s: &AppSettings, dismissed: bool) -> Self {
        Self {
            enabled: s.widget_enabled,
            background_only: s.widget_background_only,
            dismissed,
        }
    }

    /// Stan po `settings-changed`. Schowanie X trwa do końca sesji, ale ponowne
    /// włączenie widgetu w ustawieniach (false -> true) je kasuje: zaznaczony
    /// checkbox zawsze pokazuje widget.
    fn after_settings_change(self, s: &AppSettings) -> Self {
        Self::from_settings(s, self.dismissed && self.enabled)
    }
}

/// Czy okno widgetu ma być widoczne. `main_background`: główne okno jest
/// schowane, zminimalizowane albo bez fokusu.
pub fn should_show(status: Status, main_background: bool) -> bool {
    status.enabled && !status.dismissed && (!status.background_only || main_background)
}

pub struct WidgetState {
    /// Ustawienia trzymane w pamięci, żeby `apply` nie brało blokady bazy.
    status: Mutex<Status>,
    /// Powiększony widok malutkiego widgetu (`None` = malutki albo pełny na stałe).
    open: Mutex<Option<Open>>,
    wake: Mutex<mpsc::Sender<()>>,
}

fn status(app: &tauri::AppHandle) -> Status {
    app.try_state::<WidgetState>()
        .and_then(|s| s.status.lock().ok().map(|s| *s))
        .unwrap_or_default()
}

/// Czy główne okno jest w tle: schowane, zminimalizowane albo bez fokusu.
pub fn main_background(app: &tauri::AppHandle) -> bool {
    match app.get_webview_window("main") {
        Some(main) => {
            !main.is_visible().unwrap_or(true)
                || main.is_minimized().unwrap_or(false)
                || !main.is_focused().unwrap_or(true)
        }
        None => true,
    }
}

/// Pokazuje lub chowa okno widgetu. `main_background` liczy wywołujący (pętla
/// w `reminder.rs` i tak czyta stan głównego okna). Okno nie przejmuje fokusu
/// (`focusable: false`), więc `show` nie zabiera go aplikacji, w której pracujesz.
pub fn apply(app: &tauri::AppHandle, main_background: bool) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    let show = should_show(status(app), main_background);
    if window.is_visible().unwrap_or(!show) == show {
        return;
    }
    if show {
        let _ = window.show();
        let _ = app.emit_to(LABEL, "widget-shown", ());
    } else {
        let _ = window.hide();
    }
}

/// Budzi wątek widgetu, który wywoła `apply`. Dla miejsc, które nie mogą czekać
/// na odczyt stanu okien (zdarzenia z wątku głównego).
pub fn wake(app: &tauri::AppHandle) {
    if let Some(state) = app.try_state::<WidgetState>() {
        if let Ok(tx) = state.wake.lock() {
            let _ = tx.send(());
        }
    }
}

fn update(app: &tauri::AppHandle, change: impl FnOnce(&mut Status)) {
    let next = {
        let Some(state) = app.try_state::<WidgetState>() else {
            return;
        };
        let Ok(mut s) = state.status.lock() else {
            return;
        };
        change(&mut s);
        *s
    };
    let _ = app.emit("widget-state", next);
    wake(app);
}

/// Sygnały `wake` zebrane od `setup` do `start`.
pub struct Waker(mpsc::Receiver<()>);

/// Stan początkowy z ustawień i nasłuch `settings-changed`. Wątek
/// odświeżający uruchamia dopiero `start`, po utworzeniu okien.
pub fn setup(app: &tauri::App) -> Waker {
    let (wake, signal) = mpsc::channel::<()>();
    app.manage(WidgetState {
        status: Mutex::new(Status::from_settings(
            &settings::current(app.handle()),
            false,
        )),
        open: Mutex::new(None),
        wake: Mutex::new(wake),
    });
    let handle = app.handle().clone();
    app.listen_any("settings-changed", move |event| {
        if let Ok(s) = serde_json::from_str::<AppSettings>(event.payload()) {
            update(&handle, |status| {
                *status = status.after_settings_change(&s);
            });
        }
    });
    Waker(signal)
}

/// Wątek odświeżający: pierwsze `apply` od razu, potem po każdym `wake`.
/// Kończy się, gdy okno main zniknie, więc wołać po `create_windows`;
/// sygnały wysłane wcześniej czekają w kanale.
pub fn start(app: &tauri::AppHandle, waker: Waker) {
    let Waker(signal) = waker;
    let handle = app.clone();
    std::thread::spawn(move || {
        apply(&handle, main_background(&handle));
        while signal.recv().is_ok() {
            // Kilka sygnałów pod rząd: wystarczy jedno sprawdzenie.
            while signal.try_recv().is_ok() {}
            if handle.get_webview_window("main").is_none() {
                break;
            }
            apply(&handle, main_background(&handle));
        }
    });
}

/// Stan dla przełącznika w nagłówku głównego okna.
#[tauri::command]
pub fn widget_status(app: tauri::AppHandle) -> Status {
    status(&app)
}

/// Chowa widget do końca sesji aplikacji (`true`, przycisk X) albo go przywraca
/// (`false`, przełącznik w nagłówku). Ustawienie `widget_enabled` się nie zmienia.
#[tauri::command]
pub fn widget_dismiss(app: tauri::AppHandle, dismissed: bool) -> Status {
    if dismissed {
        crate::window_state::save(&app);
    }
    update(&app, |s| s.dismissed = dismissed);
    status(&app)
}

fn rect_of<R: Runtime>(window: &tauri::WebviewWindow<R>) -> Option<Rect> {
    let pos = window.outer_position().ok()?;
    let size = window.outer_size().ok()?;
    Some(Rect {
        x: pos.x,
        y: pos.y,
        w: size.width as i32,
        h: size.height as i32,
    })
}

fn place<R: Runtime>(window: &tauri::WebviewWindow<R>, rect: Rect) {
    let _ = window.set_position(PhysicalPosition::new(rect.x, rect.y));
    let _ = window.set_size(PhysicalSize::new(
        rect.w.max(1) as u32,
        rect.h.max(1) as u32,
    ));
}

/// Obszar roboczy monitora, na którym stoi okno (bez paska zadań).
fn work_area<R: Runtime>(window: &tauri::WebviewWindow<R>) -> Rect {
    match window.current_monitor() {
        Ok(Some(m)) => {
            let w = m.work_area();
            Rect {
                x: w.position.x,
                y: w.position.y,
                w: w.size.width as i32,
                h: w.size.height as i32,
            }
        }
        // Bez informacji o monitorze nie ograniczamy okna.
        _ => Rect {
            x: -100_000,
            y: -100_000,
            w: 200_000,
            h: 200_000,
        },
    }
}

/// Zwija powiększone okno do malutkiego: na zapamiętane miejsce, a po
/// przeciągnięciu pełnego widoku od tego samego narożnika.
fn collapse_open<R: Runtime>(
    window: &tauri::WebviewWindow<R>,
    open: Open,
    compact_w: i32,
    compact_h: i32,
) {
    let target = match rect_of(window) {
        Some(now) if (now.x, now.y) != (open.placed.x, open.placed.y) => {
            collapsed_rect(now, open.anchor, compact_w, compact_h)
        }
        _ => Rect {
            w: compact_w,
            h: compact_h,
            ..open.compact
        },
    };
    place(window, target);
}

fn compact_px<R: Runtime>(window: &tauri::WebviewWindow<R>) -> (i32, i32) {
    let scale = window.scale_factor().unwrap_or(1.0);
    (
        (COMPACT_SIZE.0 * scale).round() as i32,
        (COMPACT_SIZE.1 * scale).round() as i32,
    )
}

/// Przed zapisem położenia okien: zwinięty widok, żeby zapamiętać miejsce
/// malutkiego widgetu, a nie powiększonego.
pub fn collapse_for_save<R: Runtime>(app: &AppHandle<R>) {
    let Some(state) = app.try_state::<WidgetState>() else {
        return;
    };
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    let Ok(mut open) = state.open.lock() else {
        return;
    };
    if let Some(o) = open.take() {
        let (w, h) = compact_px(&window);
        collapse_open(&window, o, w, h);
    }
}

/// Układ okna widgetu zgodny z widokiem we frontendzie: `compact` to ustawienie
/// `widget_compact`, `expanded` to powiększenie po najechaniu, `full_height`
/// to wysokość pełnego widoku (64, z przypiętymi 96, w pikselach logicznych).
/// Zwraca `true`, gdy okno zostaje powiększone mimo prośby o zwinięcie: otwarte
/// jest okno szybkiego przełączania (⇄). Okno nie przejmuje fokusu.
#[tauri::command]
pub fn widget_set_layout(
    app: tauri::AppHandle,
    compact: bool,
    expanded: bool,
    full_height: u32,
) -> Result<bool, String> {
    if !(40..=400).contains(&full_height) {
        return Err("Niepoprawna wysokość widgetu.".into());
    }
    let window = app
        .get_webview_window(LABEL)
        .ok_or("Brak okna mini-widgetu.")?;
    let state = app.state::<WidgetState>();
    let mut open = state.open.lock().map_err(|e| e.to_string())?;
    let scale = window.scale_factor().map_err(|e| e.to_string())?;
    let (cw, ch) = compact_px(&window);
    if !compact {
        // Pełny widok na stałe: bez powrotu na miejsce malutkiego.
        *open = None;
        window
            .set_size(LogicalSize::new(FULL_WIDTH, f64::from(full_height)))
            .map_err(|e| e.to_string())?;
        return Ok(false);
    }
    if expanded {
        let compact_rect = match *open {
            Some(o) => o.compact,
            None => {
                let now = rect_of(&window).ok_or("Brak położenia okna.")?;
                Rect {
                    w: cw,
                    h: ch,
                    ..now
                }
            }
        };
        let full_w = (FULL_WIDTH * scale).round() as i32;
        let full_h = (f64::from(full_height) * scale).round() as i32;
        let (placed, anchor) = expanded_rect(compact_rect, full_w, full_h, work_area(&window));
        place(&window, placed);
        *open = Some(Open {
            compact: compact_rect,
            placed,
            anchor,
        });
        return Ok(true);
    }
    if let Some(o) = *open {
        let quick_open = app
            .get_webview_window("quick")
            .is_some_and(|w| w.is_visible().unwrap_or(false));
        if quick_open {
            return Ok(true);
        }
        collapse_open(&window, o, cw, ch);
        *open = None;
    } else {
        window
            .set_size(PhysicalSize::new(cw as u32, ch as u32))
            .map_err(|e| e.to_string())?;
    }
    Ok(false)
}

/// Klik w nazwę zadania na widgecie: pokazuje główne okno.
#[tauri::command]
pub fn widget_open_main(app: tauri::AppHandle) {
    crate::show(&app);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(enabled: bool, background_only: bool, dismissed: bool) -> Status {
        Status {
            enabled,
            background_only,
            dismissed,
        }
    }

    #[test]
    fn hidden_when_disabled_or_dismissed() {
        for background in [false, true] {
            assert!(!should_show(status(false, false, false), background));
            assert!(!should_show(status(false, true, false), background));
            assert!(!should_show(status(true, false, true), background));
            assert!(!should_show(status(true, true, true), background));
        }
    }

    #[test]
    fn background_only_follows_main_window() {
        assert!(should_show(status(true, false, false), false));
        assert!(should_show(status(true, false, false), true));
        assert!(!should_show(status(true, true, false), false));
        assert!(should_show(status(true, true, false), true));
    }

    #[test]
    fn status_reads_widget_settings_and_keeps_dismissal() {
        let s = AppSettings {
            widget_enabled: true,
            widget_background_only: true,
            ..AppSettings::default()
        };
        assert_eq!(Status::from_settings(&s, true), status(true, true, true));
        assert_eq!(
            Status::from_settings(&AppSettings::default(), false),
            status(false, false, false)
        );
    }

    #[test]
    fn reenabling_in_settings_clears_dismissal() {
        let on = AppSettings {
            widget_enabled: true,
            ..AppSettings::default()
        };
        let off = AppSettings::default();
        // X, potem odznaczenie i ponowne zaznaczenie „Pokazuj mini-widget”.
        let dismissed = status(true, false, true);
        let disabled = dismissed.after_settings_change(&off);
        assert!(!should_show(disabled, true));
        let reenabled = disabled.after_settings_change(&on);
        assert_eq!(reenabled, status(true, false, false));
        assert!(should_show(reenabled, false));
        // Inna zmiana ustawień przy włączonym widgecie zachowuje schowanie.
        let background = AppSettings {
            widget_enabled: true,
            widget_background_only: true,
            ..AppSettings::default()
        };
        assert_eq!(
            dismissed.after_settings_change(&background),
            status(true, true, true)
        );
    }

    const WORK: Rect = Rect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1040,
    };

    fn compact_at(x: i32, y: i32, scale: f64) -> Rect {
        Rect {
            x,
            y,
            w: (COMPACT_SIZE.0 * scale).round() as i32,
            h: (COMPACT_SIZE.1 * scale).round() as i32,
        }
    }

    fn grow(compact: Rect, work: Rect, scale: f64) -> (Rect, Anchor) {
        expanded_rect(
            compact,
            (FULL_WIDTH * scale).round() as i32,
            (64.0 * scale).round() as i32,
            work,
        )
    }

    #[test]
    fn expands_to_the_right_and_down_from_top_left() {
        let compact = compact_at(100, 80, 1.0);
        let (rect, anchor) = grow(compact, WORK, 1.0);
        assert!(!anchor.right && !anchor.bottom);
        assert_eq!(
            rect,
            Rect {
                x: 100,
                y: 80,
                w: 320,
                h: 64
            }
        );
    }

    #[test]
    fn expands_left_near_right_edge_and_up_near_bottom_edge() {
        // Prawy dolny róg obszaru roboczego (nad paskiem zadań).
        let compact = compact_at(1920 - 96 - 10, 1040 - 28 - 10, 1.0);
        let (rect, anchor) = grow(compact, WORK, 1.0);
        assert!(anchor.right && anchor.bottom);
        // Prawa i dolna krawędź zostają tam, gdzie były.
        assert_eq!(rect.x + rect.w, compact.x + compact.w);
        assert_eq!(rect.y + rect.h, compact.y + compact.h);
        assert_eq!((rect.w, rect.h), (320, 64));
        // Zwinięcie wraca dokładnie na malutkie miejsce.
        assert_eq!(collapsed_rect(rect, anchor, 96, 28), compact);
    }

    #[test]
    fn mixed_corners_and_clamping_to_the_work_area() {
        // Lewy dolny róg: w prawo i w górę.
        let compact = compact_at(5, 1040 - 28 - 4, 1.0);
        let (rect, anchor) = grow(compact, WORK, 1.0);
        assert!(!anchor.right && anchor.bottom);
        assert_eq!((rect.x, rect.y + rect.h), (5, 1040 - 4));
        // Okno wystające poza obszar (np. po zmianie rozdzielczości) wraca do obszaru.
        let outside = compact_at(1920 - 20, 3, 1.0);
        let (clamped, _) = grow(outside, WORK, 1.0);
        assert!(clamped.x >= 0 && clamped.x + clamped.w <= 1920);
        assert!(clamped.y >= 0 && clamped.y + clamped.h <= 1040);
    }

    #[test]
    fn scale_150_percent_uses_physical_sizes() {
        let work = Rect {
            x: 0,
            y: 0,
            w: 2560,
            h: 1400,
        };
        let compact = compact_at(2560 - 144 - 15, 20, 1.5);
        assert_eq!((compact.w, compact.h), (144, 42));
        let (rect, anchor) = grow(compact, work, 1.5);
        assert!(anchor.right && !anchor.bottom);
        assert_eq!((rect.w, rect.h), (480, 96));
        assert_eq!(rect.x + rect.w, compact.x + compact.w);
        assert_eq!(rect.y, 20);
        assert_eq!(collapsed_rect(rect, anchor, 144, 42), compact);
    }

    #[test]
    fn two_monitors_use_the_work_area_of_the_window_monitor() {
        // Lewy monitor (100%) z ujemnymi współrzędnymi i prawy (150%).
        let left = Rect {
            x: -1920,
            y: 0,
            w: 1920,
            h: 1040,
        };
        let right = Rect {
            x: 0,
            y: 0,
            w: 2560,
            h: 1400,
        };
        // Widget przy prawej krawędzi lewego monitora rośnie w lewo, mimo że
        // na prawo od niego jest drugi monitor.
        let on_left = compact_at(-96 - 8, 100, 1.0);
        let (rect, anchor) = grow(on_left, left, 1.0);
        assert!(anchor.right);
        assert_eq!(rect.x + rect.w, on_left.x + on_left.w);
        assert!(rect.x >= left.x);
        // Przy lewej krawędzi prawego monitora rośnie w prawo.
        let on_right = compact_at(8, 100, 1.5);
        let (rect, anchor) = grow(on_right, right, 1.5);
        assert!(!anchor.right);
        assert_eq!(rect.x, 8);
        // Przy lewej krawędzi lewego monitora: w prawo, bez wyjścia poza -1920.
        let far_left = compact_at(-1920, 50, 1.0);
        let (rect, _) = grow(far_left, left, 1.0);
        assert_eq!(rect.x, -1920);
        // Obszar roboczy z paskiem zadań u góry (y = 48): okno nie wchodzi pod pasek.
        let top_bar = Rect {
            x: 0,
            y: 48,
            w: 1920,
            h: 992,
        };
        let (rect, _) = grow(compact_at(300, 10, 1.0), top_bar, 1.0);
        assert_eq!(rect.y, 48);
    }

    #[test]
    fn moved_expanded_window_collapses_from_the_same_corner() {
        let compact = compact_at(1800, 900, 1.0);
        let (rect, anchor) = grow(compact, WORK, 1.0);
        // Użytkownik przeciągnął pełny widok o (-300, -200).
        let moved = Rect {
            x: rect.x - 300,
            y: rect.y - 200,
            ..rect
        };
        let back = collapsed_rect(moved, anchor, 96, 28);
        assert_eq!(
            (back.x, back.y),
            (compact.x - 300, compact.y - 200),
            "malutki widget zostaje pod tym samym narożnikiem"
        );
    }

    #[test]
    fn status_serializes_in_camel_case() {
        let json = serde_json::to_value(status(true, false, true)).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"enabled": true, "backgroundOnly": false, "dismissed": true})
        );
    }
}
