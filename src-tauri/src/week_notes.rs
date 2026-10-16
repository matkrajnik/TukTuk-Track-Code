//! „Wnioski” z przeglądu tygodnia (PLAN_1.7.md, W2).
//!
//! Tabela `week_notes(week, text)` powstaje przez `CREATE TABLE IF NOT EXISTS`
//! w `db::init`. Tydzień to klucz poniedziałku `RRRR-MM-DD` (jak w interfejsie,
//! który liczy tydzień Pon–Nd). Pusty tekst usuwa wiersz. Kopia SQLite obejmuje
//! tabelę, bo kopiuje całą bazę.
use crate::Database;
use chrono::{Datelike, NaiveDate, Weekday};
use rusqlite::{params, Connection, OptionalExtension};

/// Najdłuższe wnioski (znaki).
pub const MAX_CHARS: usize = 10_000;

/// Klucz tygodnia: istniejący poniedziałek `RRRR-MM-DD`.
pub fn valid_week(week: &str) -> bool {
    crate::settings::valid_day(week)
        && NaiveDate::parse_from_str(week, "%Y-%m-%d").is_ok_and(|d| d.weekday() == Weekday::Mon)
}

fn check_week(week: &str) -> Result<(), String> {
    if valid_week(week) {
        Ok(())
    } else {
        Err("Tydzień musi być poniedziałkiem w formacie RRRR-MM-DD.".into())
    }
}

/// Wnioski tygodnia (pusty tekst, gdy ich nie ma).
pub fn load(db: &Connection, week: &str) -> Result<String, String> {
    check_week(week)?;
    db.query_row("SELECT text FROM week_notes WHERE week=?1", [week], |r| {
        r.get::<_, String>(0)
    })
    .optional()
    .map(|text| text.unwrap_or_default())
    .map_err(|e| e.to_string())
}

/// Zapisuje wnioski (białe znaki na końcach są obcinane); pusty tekst usuwa wpis.
/// Zwraca zapisany tekst.
pub fn save(db: &Connection, week: &str, text: &str) -> Result<String, String> {
    check_week(week)?;
    let text = text.trim();
    if text.chars().count() > MAX_CHARS {
        return Err(format!("Wnioski mogą mieć najwyżej {MAX_CHARS} znaków."));
    }
    if text.is_empty() {
        db.execute("DELETE FROM week_notes WHERE week=?1", [week])
    } else {
        db.execute(
            "INSERT INTO week_notes(week, text) VALUES(?1, ?2)
             ON CONFLICT(week) DO UPDATE SET text=excluded.text",
            params![week, text],
        )
    }
    .map_err(|e| e.to_string())?;
    Ok(text.to_owned())
}

#[tauri::command]
pub fn week_note(state: tauri::State<Database>, week: String) -> Result<String, String> {
    let db = state.0.lock().map_err(|e| e.to_string())?;
    load(&db, &week)
}

#[tauri::command]
pub fn week_note_save(
    state: tauri::State<Database>,
    week: String,
    text: String,
) -> Result<String, String> {
    let db = state.0.lock().map_err(|e| e.to_string())?;
    save(&db, &week, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        crate::db::init(&db).unwrap();
        db
    }

    #[test]
    fn week_key_must_be_an_existing_monday() {
        assert!(valid_week("2026-10-12"));
        assert!(valid_week("2026-12-28"));
        assert!(!valid_week("2026-10-15"));
        assert!(!valid_week("2026-02-30"));
        assert!(!valid_week("2026-W42"));
        assert!(!valid_week(""));
        let db = db();
        assert!(load(&db, "2026-10-13").is_err());
        assert!(save(&db, "12.10.2026", "x").is_err());
    }

    #[test]
    fn notes_are_saved_replaced_and_removed_when_empty() {
        let db = db();
        assert_eq!(load(&db, "2026-10-12").unwrap(), "");
        assert_eq!(
            save(&db, "2026-10-12", "  Za dużo przełączeń po południu.\n").unwrap(),
            "Za dużo przełączeń po południu."
        );
        assert_eq!(
            load(&db, "2026-10-12").unwrap(),
            "Za dużo przełączeń po południu."
        );
        // Inny tydzień osobno.
        save(&db, "2026-10-19", "Następny").unwrap();
        save(&db, "2026-10-12", "Poprawione").unwrap();
        assert_eq!(load(&db, "2026-10-12").unwrap(), "Poprawione");
        assert_eq!(load(&db, "2026-10-19").unwrap(), "Następny");
        // Pusty tekst usuwa wiersz.
        save(&db, "2026-10-12", "   ").unwrap();
        let rows: i64 = db
            .query_row("SELECT COUNT(*) FROM week_notes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);
        assert_eq!(load(&db, "2026-10-12").unwrap(), "");
        // Limit długości (znaki, nie bajty).
        let long = "ż".repeat(MAX_CHARS);
        assert!(save(&db, "2026-10-12", &long).is_ok());
        assert!(save(&db, "2026-10-12", &format!("{long}x")).is_err());
        assert_eq!(load(&db, "2026-10-12").unwrap(), long);
        // Ponowne `init` nie rusza zapisanych wniosków.
        crate::db::init(&db).unwrap();
        assert_eq!(load(&db, "2026-10-19").unwrap(), "Następny");
    }
}
