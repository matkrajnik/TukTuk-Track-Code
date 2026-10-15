//! Szablony zadań (PLAN_1.3.md, etap 3): `/nazwa` w szybkim dodawaniu tworzy
//! zadanie z tytułem, krokami, szacunkiem, projektem i priorytetem szablonu.
//! Tabela `task_templates(id, name UNIQUE, json)` powstaje w `db::init`, więc
//! kopia SQLite i przywracanie obejmują szablony razem z resztą danych.
use crate::planner::{CheckItem, Details};
use crate::Database;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tauri::Emitter;

/// Zawartość szablonu (kolumna `json`).
#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Body {
    /// Tytuł z polami `{klucz}` i `{tytuł}`; pusty = tytuł z linii albo z Jiry.
    pub title: String,
    pub estimate_minutes: u32,
    pub steps: Vec<String>,
    pub project: String,
    /// 1 = wysoki, 0 = normalny, -1 = niski.
    pub priority: i32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    pub id: i64,
    pub name: String,
    #[serde(flatten)]
    pub body: Body,
}

/// Nazwa jak w parserze (`src/quickadd.ts`): litery, cyfry, `-` i `_`, 1–60 znaków.
pub fn valid_name(name: &str) -> bool {
    let count = name.chars().count();
    (1..=60).contains(&count)
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

fn validate(name: &str, body: &mut Body) -> Result<(), String> {
    if !valid_name(name) {
        return Err(
            "Nazwa szablonu może mieć od 1 do 60 liter, cyfr, „-” lub „_” (bez spacji).".into(),
        );
    }
    body.title = body.title.trim().into();
    body.project = body.project.trim().into();
    body.steps = body
        .steps
        .iter()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();
    if body.title.chars().count() > 240 {
        return Err("Tytuł szablonu może mieć najwyżej 240 znaków.".into());
    }
    if body.project.chars().count() > 80 {
        return Err("Nazwa projektu może mieć najwyżej 80 znaków.".into());
    }
    if !(-1..=1).contains(&body.priority) || body.estimate_minutes > 600_000 {
        return Err("Niepoprawny priorytet lub szacunek szablonu.".into());
    }
    if body.steps.len() > 100 || body.steps.iter().any(|s| s.chars().count() > 240) {
        return Err("Szablon może mieć najwyżej 100 kroków po 240 znaków.".into());
    }
    Ok(())
}

pub fn list(db: &Connection) -> Result<Vec<Template>, String> {
    let mut rows = db
        .prepare("SELECT id,name,json FROM task_templates")
        .map_err(|e| e.to_string())?
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|(id, name, json)| Template {
            id,
            name,
            // Uszkodzony json nie blokuje listy: szablon da się poprawić w Ustawieniach.
            body: serde_json::from_str(&json).unwrap_or_default(),
        })
        .collect::<Vec<_>>();
    rows.sort_by_key(|t| t.name.to_lowercase());
    Ok(rows)
}

/// Szablon o nazwie bez względu na wielkość liter.
pub fn find(db: &Connection, name: &str) -> Result<Option<Template>, String> {
    let wanted = name.trim().to_lowercase();
    Ok(list(db)?
        .into_iter()
        .find(|t| t.name.to_lowercase() == wanted))
}

/// Tworzy (`id` = `None`) albo zmienia szablon, także jego nazwę.
pub fn save(
    db: &mut Connection,
    id: Option<i64>,
    name: &str,
    mut body: Body,
) -> Result<Vec<Template>, String> {
    let name = name.trim();
    validate(name, &mut body)?;
    let tx = db.transaction().map_err(|e| e.to_string())?;
    if let Some(other) = find(&tx, name)? {
        if Some(other.id) != id {
            return Err(format!("Szablon „{}” już istnieje.", other.name));
        }
    }
    let json = serde_json::to_string(&body).map_err(|e| e.to_string())?;
    match id {
        Some(id) => {
            let changed = tx
                .execute(
                    "UPDATE task_templates SET name=?1, json=?2 WHERE id=?3",
                    params![name, json, id],
                )
                .map_err(|e| e.to_string())?;
            if changed == 0 {
                return Err("Nie znaleziono szablonu.".into());
            }
        }
        None => {
            tx.execute(
                "INSERT INTO task_templates(name,json) VALUES(?1,?2)",
                params![name, json],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    list(db)
}

pub fn delete(db: &Connection, id: i64) -> Result<Vec<Template>, String> {
    let removed = db
        .execute("DELETE FROM task_templates WHERE id=?1", [id])
        .map_err(|e| e.to_string())?;
    if removed == 0 {
        return Err("Nie znaleziono szablonu.".into());
    }
    list(db)
}

/// Tytuł zadania z szablonu.
/// - `{klucz}` → klucz Jiry (pusty, gdy go nie podano);
/// - `{tytuł}` (także `{tytul}`) → tytuł z linii, a bez niego tytuł zgłoszenia z Jiry;
/// - tytuł z linii bez pola `{tytuł}` w szablonie jest dopisywany na końcu;
/// - pusty tytuł szablonu zostawia tytuł z linii (dalej jak bez szablonu).
///
/// Odstępy i znaki `:`/`-` po pustych polach są sprzątane, np. „Code review :” → „Code review”.
pub fn render_title(
    template: &str,
    key: Option<&str>,
    line: &str,
    summary: impl FnOnce(&str) -> Option<String>,
) -> Result<String, String> {
    let line = line.trim();
    let template = template.trim();
    if template.is_empty() {
        return Ok(line.to_owned());
    }
    let wants_title = template.contains("{tytuł}") || template.contains("{tytul}");
    let value = if !line.is_empty() || !wants_title {
        line.to_owned()
    } else if let Some(key) = key {
        summary(key)
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .ok_or(format!(
                "Nie znam tytułu {key}. Wpisz tytuł albo odśwież listę zgłoszeń w widoku Jira."
            ))?
    } else {
        String::new()
    };
    let mut title = template
        .replace("{klucz}", key.unwrap_or(""))
        .replace("{tytuł}", &value)
        .replace("{tytul}", &value);
    if !wants_title && !line.is_empty() {
        title = format!("{title} {line}");
    }
    Ok(tidy(&title))
}

/// Sprząta tytuł po pustych polach: pojedyncze odstępy, bez odstępu przed `:`/`,`,
/// bez separatorów na początku i końcu.
fn tidy(title: &str) -> String {
    let words = title.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = String::with_capacity(words.len());
    for c in words.chars() {
        if matches!(c, ':' | ',' | '.') && out.ends_with(' ') {
            out.pop();
        }
        out.push(c);
    }
    let separators: &[char] = &[':', ',', '-', '–', '—', ' '];
    out.trim_matches(separators).to_owned()
}

/// Pola szablonu pod pola z linii: jawne pola z linii (niezerowe) wygrywają.
/// `None`, gdy ani linia, ani szablon nie mają pól planu.
pub fn merge_details(line: Option<Details>, body: &Body) -> Option<Details> {
    let template_fields = body.estimate_minutes > 0
        || !body.project.is_empty()
        || body.priority != 0
        || !body.steps.is_empty();
    if line.is_none() && !template_fields {
        return None;
    }
    let mut details = line.unwrap_or_default();
    if details.estimate_minutes == 0 {
        details.estimate_minutes = body.estimate_minutes;
    }
    if details.project.trim().is_empty() {
        details.project = body.project.clone();
    }
    if details.priority == 0 {
        details.priority = body.priority;
    }
    if details.checklist.is_empty() {
        details.checklist = body
            .steps
            .iter()
            .map(|text| CheckItem {
                text: text.clone(),
                done: false,
            })
            .collect();
    }
    Some(details)
}

fn changed(app: &tauri::AppHandle) {
    let _ = app.emit("templates-changed", ());
}

#[tauri::command]
pub fn templates_list(state: tauri::State<Database>) -> Result<Vec<Template>, String> {
    let db = state.0.lock().map_err(|e| e.to_string())?;
    list(&db)
}

#[tauri::command]
pub fn template_save(
    state: tauri::State<Database>,
    app: tauri::AppHandle,
    id: Option<i64>,
    name: String,
    template: Body,
) -> Result<Vec<Template>, String> {
    let mut db = state.0.lock().map_err(|e| e.to_string())?;
    let result = save(&mut db, id, &name, template)?;
    changed(&app);
    Ok(result)
}

#[tauri::command]
pub fn template_delete(
    state: tauri::State<Database>,
    app: tauri::AppHandle,
    id: i64,
) -> Result<Vec<Template>, String> {
    let db = state.0.lock().map_err(|e| e.to_string())?;
    let result = delete(&db, id)?;
    changed(&app);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn open() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db::init(&db).unwrap();
        db
    }

    fn review() -> Body {
        Body {
            title: "Code review {klucz}: {tytuł}".into(),
            estimate_minutes: 45,
            steps: vec![
                "Przeczytaj opis".into(),
                "  ".into(),
                " Sprawdź testy ".into(),
            ],
            project: " WMS ".into(),
            priority: 1,
        }
    }

    #[test]
    fn saves_and_reads_templates() {
        let mut db = open();
        let list = save(&mut db, None, " review ", review()).unwrap();
        assert_eq!(list.len(), 1);
        let t = &list[0];
        assert_eq!(t.name, "review");
        assert_eq!(t.body.title, "Code review {klucz}: {tytuł}");
        assert_eq!(t.body.steps, vec!["Przeczytaj opis", "Sprawdź testy"]);
        assert_eq!(t.body.project, "WMS");
        assert_eq!((t.body.estimate_minutes, t.body.priority), (45, 1));
        // Odczyt bez względu na wielkość liter, zmiana nazwy i treści.
        let found = find(&db, "REVIEW").unwrap().unwrap();
        assert_eq!(found.id, t.id);
        let renamed = save(
            &mut db,
            Some(t.id),
            "przegląd",
            Body {
                title: "Przegląd {tytuł}".into(),
                ..Body::default()
            },
        )
        .unwrap();
        assert_eq!(renamed.len(), 1);
        assert_eq!(renamed[0].name, "przegląd");
        assert_eq!(renamed[0].body.title, "Przegląd {tytuł}");
        assert!(find(&db, "review").unwrap().is_none());
        // Usunięcie; drugi raz czytelny błąd.
        assert!(delete(&db, t.id).unwrap().is_empty());
        assert!(delete(&db, t.id).is_err());
        assert!(save(&mut db, Some(999), "x", Body::default()).is_err());
    }

    #[test]
    fn template_name_is_unique_and_validated() {
        let mut db = open();
        save(&mut db, None, "review", review()).unwrap();
        let err = save(&mut db, None, "Review", Body::default())
            .err()
            .unwrap();
        assert!(err.contains("już istnieje"), "{err}");
        // Unikalność także dla polskich liter, których SQLite NOCASE nie składa.
        save(&mut db, None, "źródło", Body::default()).unwrap();
        assert!(save(&mut db, None, "ŹRÓDŁO", Body::default()).is_err());
        // Zmiana nazwy na zajętą też jest odrzucana; zapis pod tą samą nazwą działa.
        let id = find(&db, "źródło").unwrap().unwrap().id;
        assert!(save(&mut db, Some(id), "REVIEW", Body::default()).is_err());
        save(&mut db, Some(id), "Źródło", Body::default()).unwrap();
        for bad in ["", "dwa słowa", "a/b", &"x".repeat(61)] {
            assert!(save(&mut db, None, bad, Body::default()).is_err(), "{bad}");
        }
        assert!(save(
            &mut db,
            None,
            "dlugi",
            Body {
                steps: vec!["x".repeat(241)],
                ..Body::default()
            }
        )
        .is_err());
        assert!(save(
            &mut db,
            None,
            "prio",
            Body {
                priority: 2,
                ..Body::default()
            }
        )
        .is_err());
        assert_eq!(list(&db).unwrap().len(), 2);
    }

    #[test]
    fn renders_title_fields() {
        let jira = |key: &str| (key == "WMS-123").then(|| "Eksport etykiet".to_owned());
        let t = "Code review {klucz}: {tytuł}";
        assert_eq!(
            render_title(t, Some("WMS-123"), "", jira).unwrap(),
            "Code review WMS-123: Eksport etykiet"
        );
        // Tytuł z linii wygrywa z tytułem z Jiry.
        assert_eq!(
            render_title(t, Some("WMS-123"), " Poprawki ", jira).unwrap(),
            "Code review WMS-123: Poprawki"
        );
        // Bez klucza i bez tytułu: puste pola są sprzątane.
        assert_eq!(render_title(t, None, "", jira).unwrap(), "Code review");
        assert_eq!(
            render_title(t, None, "Raport", jira).unwrap(),
            "Code review: Raport"
        );
        // Nieznane zgłoszenie: czytelny błąd zamiast tytułu z dziurą.
        assert!(render_title(t, Some("WMS-9"), "", jira)
            .err()
            .unwrap()
            .contains("WMS-9"));
        // Bez pola {tytuł} tytuł z linii jest dopisywany; pusty szablon go zostawia.
        assert_eq!(
            render_title("Daily", None, "z zespołem", jira).unwrap(),
            "Daily z zespołem"
        );
        assert_eq!(render_title("", None, "Raport", jira).unwrap(), "Raport");
        assert_eq!(
            render_title("{klucz} – {tytul}", Some("WMS-123"), "", jira).unwrap(),
            "WMS-123 – Eksport etykiet"
        );
    }

    #[test]
    fn explicit_fields_win_over_template() {
        let body = review();
        let mut body = body;
        validate("review", &mut body).unwrap();
        let merged = merge_details(None, &body).unwrap();
        assert_eq!(merged.estimate_minutes, 45);
        assert_eq!(merged.project, "WMS");
        assert_eq!(merged.priority, 1);
        assert_eq!(
            merged
                .checklist
                .iter()
                .map(|c| c.text.as_str())
                .collect::<Vec<_>>(),
            ["Przeczytaj opis", "Sprawdź testy"]
        );
        let merged = merge_details(
            Some(Details {
                estimate_minutes: 90,
                priority: -1,
                project: "Foundation".into(),
                planned_day: "2026-10-16".into(),
                ..Details::default()
            }),
            &body,
        )
        .unwrap();
        assert_eq!(merged.estimate_minutes, 90);
        assert_eq!(merged.priority, -1);
        assert_eq!(merged.project, "Foundation");
        assert_eq!(merged.planned_day, "2026-10-16");
        assert_eq!(merged.checklist.len(), 2);
        assert!(merge_details(None, &Body::default()).is_none());
    }

    #[test]
    fn existing_database_gains_table_without_losing_data() {
        // Baza sprzed szablonów: init dodaje tabelę i nie rusza zadań.
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE tasks (id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, done INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL);
             INSERT INTO tasks(title,created_at) VALUES('Stare',1);",
        )
        .unwrap();
        db::init(&db).unwrap();
        db::init(&db).unwrap();
        save(&mut db, None, "review", review()).unwrap();
        assert_eq!(db::snapshot(&db).unwrap().tasks[0].title, "Stare");
        assert_eq!(list(&db).unwrap().len(), 1);
    }
}
