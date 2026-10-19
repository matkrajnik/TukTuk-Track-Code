//! Pola Jiry na kafelku zadania: lista pól z `GET /rest/api/3/field`
//! (pamięć podręczna `jira_fields`), wartości wybranych pól zgłoszeń
//! (`jira_issue_fields`, zapisywane przy każdym pobraniu listy, tablicy
//! i obserwacji przypisań) oraz odświeżanie zgłoszeń powiązanych z otwartymi
//! zadaniami przez `POST /rest/api/3/issue/bulkfetch` niezależnie od tego, kto
//! jest przypisany. Priorytet z Jiry przenosi się na pasmo priorytetu zadania
//! przy imporcie i przy każdej zmianie w Jirze (`priority_band`).
use crate::jira::{self, Assigned, Config};
use crate::Database;
use reqwest::Client;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use tauri::{Emitter, Manager};

/// Najdłuższa zapisana wartość pola (znaki).
const VALUE_LIMIT: usize = 200;
/// Najwięcej zgłoszeń w jednym `bulkfetch`.
const BULK_LIMIT: usize = 100;
/// Pola pobierane zawsze (tytuł, status i priorytet do pasma zadania).
pub const BASE_FIELDS: [&str; 3] = ["summary", "status", "priority"];

/// Wartość pola zgłoszenia na kafelku.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldValue {
    pub id: String,
    pub name: String,
    pub value: String,
}

/// Pole z listy pól Jiry (`/rest/api/3/field`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldInfo {
    pub id: String,
    pub name: String,
    pub custom: bool,
    /// Typ ze schematu, np. `string`, `user`, `array:string`, `option`.
    pub kind: String,
}

/// Identyfikator pola Jiry: litera, potem litery, cyfry i `_`, do 60 znaków.
pub fn valid_field_id(id: &str) -> bool {
    let b = id.as_bytes();
    !b.is_empty()
        && b.len() <= 60
        && b[0].is_ascii_alphabetic()
        && b.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_')
}

/// Parametr `fields`: pola bazowe i wybrane, bez powtórek.
pub fn fields_param(base: &[&str], extra: &[String]) -> String {
    let mut out: Vec<&str> = base.to_vec();
    for id in extra {
        if valid_field_id(id) && !out.contains(&id.as_str()) {
            out.push(id);
        }
    }
    out.join(",")
}

/// Polskie nazwy pól systemowych, gdy lista pól nie była jeszcze pobrana.
fn system_name(id: &str) -> Option<&'static str> {
    Some(match id {
        "labels" => "Etykiety",
        "assignee" => "Przypisany",
        "priority" => "Priorytet",
        "reporter" => "Zgłaszający",
        "creator" => "Autor",
        "duedate" => "Termin",
        "components" => "Komponenty",
        "fixVersions" => "Wersje naprawy",
        "versions" => "Wersje",
        "issuetype" => "Typ",
        "resolution" => "Rozwiązanie",
        "created" => "Utworzono",
        "updated" => "Zaktualizowano",
        "parent" => "Nadrzędne",
        "project" => "Projekt",
        "environment" => "Środowisko",
        _ => return None,
    })
}

/// Nazwa pola: z pamięci podręcznej, inaczej polska nazwa pola systemowego,
/// inaczej identyfikator.
pub fn field_name(names: &HashMap<String, String>, id: &str) -> String {
    names
        .get(id)
        .cloned()
        .or_else(|| system_name(id).map(str::to_string))
        .unwrap_or_else(|| id.to_string())
}

fn short(text: &str, limit: usize) -> String {
    text.trim().chars().take(limit).collect()
}

/// Data `RRRR-MM-DD` → `DD.MM.RRRR`, data z czasem `RRRR-MM-DDTGG:MM…` →
/// `DD.MM.RRRR GG:MM` (czas tak, jak podała Jira).
fn format_date(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let date = b.len() >= 10
        && b[4] == b'-'
        && b[7] == b'-'
        && [0, 1, 2, 3, 5, 6, 8, 9]
            .iter()
            .all(|&i| b[i].is_ascii_digit());
    if !date {
        return None;
    }
    let day = format!("{}.{}.{}", &s[8..10], &s[5..7], &s[0..4]);
    if b.len() == 10 {
        return Some(day);
    }
    if b.len() >= 16
        && b[10] == b'T'
        && b[13] == b':'
        && [11, 12, 14, 15].iter().all(|&i| b[i].is_ascii_digit())
    {
        return Some(format!("{day} {}", &s[11..16]));
    }
    None
}

fn format_text(s: &str) -> String {
    let s = s.trim();
    if let Some(date) = format_date(s) {
        return date;
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Wartość pola jako tekst, po kształcie JSON (bez schematu pola): tekst
/// i liczba wprost, data po polsku, `tak`/`nie`, osoba po nazwie wyświetlanej,
/// opcja, priorytet, wersja, komponent i sprint po nazwie, zgłoszenie
/// nadrzędne po kluczu, listy po przecinku. Treść sformatowana (ADF) i inne
/// nieznane obiekty dają pusty tekst.
pub fn format_field(value: &Value) -> String {
    let text = match value {
        Value::Null => String::new(),
        Value::Bool(b) => if *b { "tak" } else { "nie" }.to_string(),
        Value::Number(n) => match (n.as_i64(), n.as_u64(), n.as_f64()) {
            (Some(i), _, _) => i.to_string(),
            (_, Some(u), _) => u.to_string(),
            (_, _, Some(f)) if f.fract() == 0.0 && f.abs() < 1e15 => format!("{}", f as i64),
            (_, _, Some(f)) => format!("{f}"),
            _ => String::new(),
        },
        Value::String(s) => format_text(s),
        Value::Array(items) => items
            .iter()
            .map(format_field)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", "),
        Value::Object(o) => {
            if o.get("type").and_then(Value::as_str) == Some("doc") {
                String::new()
            } else {
                ["displayName", "name", "value", "key"]
                    .iter()
                    .find_map(|k| o.get(*k).and_then(Value::as_str))
                    .map(format_text)
                    .unwrap_or_default()
            }
        }
    };
    short(&text, VALUE_LIMIT)
}

/// Wartości wybranych pól z obiektu `fields` zgłoszenia; tylko niepuste.
pub fn field_values(
    fields: &Value,
    ids: &[String],
    names: &HashMap<String, String>,
) -> Vec<FieldValue> {
    ids.iter()
        .filter(|id| valid_field_id(id))
        .filter_map(|id| {
            let value = format_field(&fields[id.as_str()]);
            (!value.is_empty()).then(|| FieldValue {
                id: id.clone(),
                name: field_name(names, id),
                value,
            })
        })
        .collect()
}

/// Pasmo priorytetu zadania (-1, 0, 1) dla nazwy priorytetu z Jiry;
/// nieznana nazwa nie zmienia zadania.
pub fn priority_band(name: &str) -> Option<i32> {
    let n = name.trim().to_lowercase();
    if n.is_empty() {
        return None;
    }
    if [
        "highest",
        "high",
        "critical",
        "blocker",
        "major",
        "urgent",
        "najwyższy",
        "wysoki",
        "krytyczny",
        "pilny",
    ]
    .contains(&n.as_str())
    {
        return Some(1);
    }
    if [
        "lowest",
        "low",
        "minor",
        "trivial",
        "najniższy",
        "niski",
        "drobny",
    ]
    .contains(&n.as_str())
    {
        return Some(-1);
    }
    if ["medium", "normal", "średni", "normalny", "zwykły"].contains(&n.as_str()) {
        return Some(0);
    }
    None
}

/// Lista pól z odpowiedzi `/rest/api/3/field`: systemowe przed niestandardowymi,
/// w kolejności nazw.
pub fn parse_fields(value: &Value) -> Result<Vec<FieldInfo>, String> {
    let list = value.as_array().ok_or("Jira nie zwróciła listy pól.")?;
    let mut out: Vec<FieldInfo> = Vec::new();
    for item in list {
        let Some(id) = item["id"].as_str().filter(|id| valid_field_id(id)) else {
            continue;
        };
        if out.iter().any(|f| f.id == id) {
            continue;
        }
        let schema = &item["schema"];
        let mut kind = schema["type"].as_str().unwrap_or("").to_string();
        if let Some(items) = schema["items"].as_str() {
            kind = format!("{kind}:{items}");
        }
        out.push(FieldInfo {
            id: id.to_string(),
            name: short(item["name"].as_str().unwrap_or(id), 120),
            custom: item["custom"]
                .as_bool()
                .unwrap_or(id.starts_with("customfield_")),
            kind: short(&kind, 60),
        });
    }
    out.sort_by(|a, b| {
        a.custom
            .cmp(&b.custom)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(out)
}

/// Zastępuje listę pól strony w pamięci podręcznej.
pub fn store_fields(db: &mut Connection, site: &str, fields: &[FieldInfo]) -> rusqlite::Result<()> {
    let tx = db.transaction()?;
    tx.execute("DELETE FROM jira_fields WHERE site=?1", [site])?;
    for f in fields {
        tx.execute(
            "INSERT INTO jira_fields(site,id,name,custom,kind) VALUES(?1,?2,?3,?4,?5)",
            params![site, f.id, f.name, f.custom, f.kind],
        )?;
    }
    tx.commit()
}

/// Lista pól strony z pamięci podręcznej (pusta, gdy nigdy nie pobrana).
pub fn cached_fields(db: &Connection, site: &str) -> rusqlite::Result<Vec<FieldInfo>> {
    db.prepare("SELECT id,name,custom,kind FROM jira_fields WHERE site=?1 ORDER BY custom, name COLLATE NOCASE")?
        .query_map([site], |r| {
            Ok(FieldInfo {
                id: r.get(0)?,
                name: r.get(1)?,
                custom: r.get(2)?,
                kind: r.get(3)?,
            })
        })?
        .collect()
}

/// Nazwy pól strony: identyfikator → nazwa.
pub fn field_names(db: &Connection, site: &str) -> HashMap<String, String> {
    db.prepare("SELECT id,name FROM jira_fields WHERE site=?1")
        .and_then(|mut s| {
            s.query_map([site], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<HashMap<String, String>>>()
        })
        .unwrap_or_default()
}

/// Wybrane pola na kafelku z ustawień.
pub fn card_fields(db: &Connection) -> Vec<String> {
    crate::settings::load(db).jira_card_fields
}

/// Pola wybrane i nazwy pól strony (jedno zapytanie na pobranie).
pub fn card_context(app: &tauri::AppHandle, site: &str) -> (Vec<String>, HashMap<String, String>) {
    let state = app.state::<Database>();
    let out = match state.0.lock() {
        Ok(db) => (card_fields(&db), field_names(&db, site)),
        Err(_) => (Vec::new(), HashMap::new()),
    };
    out
}

/// Zapisuje wartości pól zgłoszenia (zastępując poprzednie) i priorytet.
/// Zmiana priorytetu w Jirze (także pierwszy zapis) ustawia pasmo priorytetu
/// powiązanego zadania; dopóki Jira go nie zmieni, ręczne pasmo zostaje.
pub fn save_fields(
    tx: &rusqlite::Transaction,
    site: &str,
    key: &str,
    priority: &str,
    fields: &[FieldValue],
) -> rusqlite::Result<()> {
    let previous: Option<String> = tx
        .query_row(
            "SELECT value FROM jira_issue_fields WHERE site=?1 AND issue_key=?2 AND field_id='priority'",
            params![site, key],
            |r| r.get(0),
        )
        .optional()?;
    tx.execute(
        "DELETE FROM jira_issue_fields WHERE site=?1 AND issue_key=?2",
        params![site, key],
    )?;
    tx.execute(
        "INSERT INTO jira_issue_fields(site,issue_key,field_id,value) VALUES(?1,?2,'priority',?3)",
        params![site, key, short(priority, VALUE_LIMIT)],
    )?;
    for f in fields
        .iter()
        .filter(|f| f.id != "priority" && !f.value.is_empty() && valid_field_id(&f.id))
    {
        tx.execute(
            "INSERT OR REPLACE INTO jira_issue_fields(site,issue_key,field_id,value) VALUES(?1,?2,?3,?4)",
            params![site, key, f.id, short(&f.value, VALUE_LIMIT)],
        )?;
    }
    if previous.as_deref() != Some(priority) {
        if let Some(band) = priority_band(priority) {
            tx.execute(
                "INSERT INTO task_details(task_id,json) SELECT task_id, json_object('priority',?3) FROM jira_tasks WHERE site=?1 AND issue_key=?2
                 ON CONFLICT(task_id) DO UPDATE SET json=json_set(json,'$.priority',?3)",
                params![site, key, band],
            )?;
        }
    }
    Ok(())
}

/// Status i pola pobranych zgłoszeń (bez importu zadań).
pub fn store_rows(db: &mut Connection, site: &str, rows: &[Assigned]) -> rusqlite::Result<()> {
    let tx = db.transaction()?;
    jira::save_statuses(
        &tx,
        site,
        rows.iter().map(|r| (r.key.as_str(), r.status.as_str())),
    )?;
    for row in rows {
        save_fields(&tx, site, &row.key, &row.priority, &row.fields)?;
    }
    tx.commit()
}

/// Jedna wartość pola (np. przypisany po „Przypisz do mnie”).
pub fn store_value(app: &tauri::AppHandle, site: &str, key: &str, id: &str, value: &str) {
    if !valid_field_id(id) || value.trim().is_empty() {
        return;
    }
    let state = app.state::<Database>();
    let Ok(db) = state.0.lock() else {
        return;
    };
    let _ = db.execute(
        "INSERT OR REPLACE INTO jira_issue_fields(site,issue_key,field_id,value) VALUES(?1,?2,?3,?4)",
        params![site, key, id, short(value, VALUE_LIMIT)],
    );
}

/// Wartości pól z ustawień dla każdego zadania z kluczem (do `db::snapshot`).
pub fn card_fields_by_task(db: &Connection) -> rusqlite::Result<HashMap<i64, Vec<FieldValue>>> {
    let ids = card_fields(db);
    let mut out: HashMap<i64, Vec<FieldValue>> = HashMap::new();
    if ids.is_empty() {
        return Ok(out);
    }
    let names: HashMap<String, String> = db
        .prepare("SELECT id,name FROM jira_fields")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut by_task: HashMap<i64, HashMap<String, String>> = HashMap::new();
    let mut stmt = db.prepare(
        "SELECT j.task_id, f.field_id, f.value FROM jira_issue_fields f
         JOIN jira_tasks j ON j.site=f.site AND j.issue_key=f.issue_key WHERE f.value<>''",
    )?;
    for row in stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (task, id, value) = row?;
        by_task.entry(task).or_default().insert(id, value);
    }
    for (task, values) in by_task {
        let list: Vec<FieldValue> = ids
            .iter()
            .filter_map(|id| {
                values.get(id).map(|value| FieldValue {
                    id: id.clone(),
                    name: field_name(&names, id),
                    value: value.clone(),
                })
            })
            .collect();
        if !list.is_empty() {
            out.insert(task, list);
        }
    }
    Ok(out)
}

/// Klucze zgłoszeń powiązanych z otwartymi zadaniami (bez ukończonych i kosza).
fn linked_open_keys(db: &Connection, site: &str) -> rusqlite::Result<Vec<String>> {
    db.prepare(
        "SELECT j.issue_key FROM jira_tasks j JOIN tasks t ON t.id=j.task_id
         LEFT JOIN task_lifecycle l ON l.task_id=t.id
         WHERE j.site=?1 AND t.done=0 AND l.deleted_at IS NULL ORDER BY j.issue_key",
    )?
    .query_map([site], |r| r.get(0))?
    .collect()
}

/// Zgłoszenia powiązane z otwartymi zadaniami poza `skip` (np. już pobrane
/// jako przypisane do mnie), wprost po kluczach (`bulkfetch`), więc także te
/// przekazane komuś innemu. Nieistniejące klucze Jira pomija (`issueErrors`).
pub async fn fetch_linked(
    app: &tauri::AppHandle,
    client: &Client,
    c: &Config,
    secret: &str,
    skip: &HashSet<String>,
) -> Result<Vec<Assigned>, String> {
    let (keys, extra, names) = {
        let state = app.state::<Database>();
        let db = state.0.lock().map_err(|e| e.to_string())?;
        let keys: Vec<String> = linked_open_keys(&db, &c.site)
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|k| jira::valid_key(k) && !skip.contains(k))
            .collect();
        (keys, card_fields(&db), field_names(&db, &c.site))
    };
    let fields: Vec<String> = fields_param(&BASE_FIELDS, &extra)
        .split(',')
        .map(str::to_string)
        .collect();
    let mut rows = Vec::new();
    for chunk in keys.chunks(BULK_LIMIT) {
        let body = json!({"issueIdsOrKeys": chunk, "fields": fields});
        let value = jira::post(client, c, secret, "/rest/api/3/issue/bulkfetch", &body).await?;
        jira::assigned_rows(&value, &extra, &names, &mut rows)?;
    }
    Ok(rows)
}

/// Lista pól Jiry do wyboru w Ustawieniach: z pamięci podręcznej, a przy
/// `refresh` (i gdy pamięć jest pusta) z Jiry.
#[tauri::command]
pub async fn jira_fields(app: tauri::AppHandle, refresh: bool) -> Result<Vec<FieldInfo>, String> {
    let c = jira::config(&app)?;
    if c.email.is_empty() {
        return Err("Połącz konto Jiry w widoku Jira.".into());
    }
    if !refresh {
        let state = app.state::<Database>();
        let db = state.0.lock().map_err(|e| e.to_string())?;
        let cached = cached_fields(&db, &c.site).map_err(|e| e.to_string())?;
        if !cached.is_empty() {
            return Ok(cached);
        }
    }
    let secret = jira::token(&app, &c)?;
    let value = jira::get(&jira::client()?, &c, &secret, "/rest/api/3/field", &[]).await?;
    let list = parse_fields(&value)?;
    let state = app.state::<Database>();
    let mut db = state.0.lock().map_err(|e| e.to_string())?;
    store_fields(&mut db, &c.site, &list).map_err(|e| e.to_string())?;
    Ok(list)
}

/// Odświeża status i pola zgłoszeń powiązanych z otwartymi zadaniami (po zmianie
/// listy pól w Ustawieniach, bez czekania na obserwację co 5 min).
#[tauri::command]
pub async fn jira_refresh_fields(app: tauri::AppHandle) -> Result<usize, String> {
    let c = jira::config(&app)?;
    if c.email.is_empty() {
        return Err("Połącz konto Jiry w widoku Jira.".into());
    }
    let secret = jira::token(&app, &c)?;
    let rows = fetch_linked(&app, &jira::client()?, &c, &secret, &HashSet::new()).await?;
    {
        let state = app.state::<Database>();
        let mut db = state.0.lock().map_err(|e| e.to_string())?;
        store_rows(&mut db, &c.site, &rows).map_err(|e| e.to_string())?;
    }
    let _ = app.emit("data-changed", ());
    Ok(rows.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn validates_field_ids_and_builds_fields_param() {
        for ok in ["labels", "assignee", "customfield_10020", "a"] {
            assert!(valid_field_id(ok), "{ok}");
        }
        for bad in ["", "1abc", "custom field", "a-b", "é", &"x".repeat(61)] {
            assert!(!valid_field_id(bad), "{bad}");
        }
        assert_eq!(
            fields_param(
                &BASE_FIELDS,
                &[
                    "labels".into(),
                    "priority".into(),
                    "bad id".into(),
                    "labels".into()
                ]
            ),
            "summary,status,priority,labels"
        );
    }

    #[test]
    fn formats_values_by_json_shape() {
        assert_eq!(format_field(&json!(null)), "");
        assert_eq!(format_field(&json!("  Dwie\n linie ")), "Dwie linie");
        assert_eq!(format_field(&json!("2026-10-19")), "19.10.2026");
        assert_eq!(
            format_field(&json!("2026-10-19T14:05:00.000+0200")),
            "19.10.2026 14:05"
        );
        assert_eq!(format_field(&json!(5)), "5");
        assert_eq!(format_field(&json!(3.0)), "3");
        assert_eq!(format_field(&json!(2.5)), "2.5");
        assert_eq!(format_field(&json!(true)), "tak");
        assert_eq!(format_field(&json!(["backend", "pilne"])), "backend, pilne");
        assert_eq!(
            format_field(&json!({"accountId":"1","displayName":"Anna Nowak"})),
            "Anna Nowak"
        );
        assert_eq!(format_field(&json!({"name":"High","id":"2"})), "High");
        assert_eq!(
            format_field(&json!({"value":"Produkcja","id":"7"})),
            "Produkcja"
        );
        assert_eq!(
            format_field(&json!([{"id":1,"name":"Sprint 42","state":"active"}])),
            "Sprint 42"
        );
        assert_eq!(
            format_field(&json!({"key":"WMS-1","fields":{"summary":"Epik"}})),
            "WMS-1"
        );
        assert_eq!(
            format_field(&json!({"type":"doc","version":1,"content":[]})),
            ""
        );
        assert_eq!(format_field(&json!({"foo":"bar"})), "");
        let long = "x".repeat(300);
        assert_eq!(format_field(&json!(long)).chars().count(), VALUE_LIMIT);
        let names = HashMap::from([("customfield_1".to_string(), "Środowisko".to_string())]);
        let fields = json!({"labels":["a","b"],"assignee":null,"customfield_1":{"value":"Test"}});
        assert_eq!(
            field_values(
                &fields,
                &[
                    "labels".into(),
                    "assignee".into(),
                    "customfield_1".into(),
                    "x y".into()
                ],
                &names
            ),
            vec![
                FieldValue {
                    id: "labels".into(),
                    name: "Etykiety".into(),
                    value: "a, b".into()
                },
                FieldValue {
                    id: "customfield_1".into(),
                    name: "Środowisko".into(),
                    value: "Test".into()
                },
            ]
        );
        assert_eq!(field_name(&names, "customfield_9"), "customfield_9");
    }

    #[test]
    fn maps_priority_names_to_bands() {
        assert_eq!(priority_band("Highest"), Some(1));
        assert_eq!(priority_band("high"), Some(1));
        assert_eq!(priority_band("Medium"), Some(0));
        assert_eq!(priority_band("Low"), Some(-1));
        assert_eq!(priority_band("Najniższy"), Some(-1));
        assert_eq!(priority_band(""), None);
        assert_eq!(priority_band("P3"), None);
    }

    #[test]
    fn parses_and_caches_field_list() {
        let value = json!([
            {"id":"customfield_10020","name":"Sprint","custom":true,"schema":{"type":"array","items":"json"}},
            {"id":"labels","name":"Labels","custom":false,"schema":{"type":"array","items":"string"}},
            {"id":"assignee","name":"Assignee","custom":false,"schema":{"type":"user"}},
            {"id":"bad id","name":"Zle"},
            {"id":"labels","name":"Duplikat"}
        ]);
        let list = parse_fields(&value).unwrap();
        assert_eq!(
            list.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(),
            vec!["assignee", "labels", "customfield_10020"]
        );
        assert_eq!(list[1].kind, "array:string");
        assert!(list[2].custom);
        let mut conn = Connection::open_in_memory().unwrap();
        db::init(&conn).unwrap();
        assert!(cached_fields(&conn, "s").unwrap().is_empty());
        store_fields(&mut conn, "s", &list).unwrap();
        assert_eq!(cached_fields(&conn, "s").unwrap(), list);
        assert_eq!(field_names(&conn, "s")["customfield_10020"], "Sprint");
        assert!(field_names(&conn, "inna").is_empty());
        assert!(parse_fields(&json!({})).is_err());
    }

    fn value(id: &str, v: &str) -> FieldValue {
        FieldValue {
            id: id.into(),
            name: field_name(&HashMap::new(), id),
            value: v.into(),
        }
    }

    #[test]
    fn saves_fields_and_moves_priority_only_when_jira_changes_it() {
        let site = "https://a.atlassian.net";
        let mut conn = Connection::open_in_memory().unwrap();
        db::init(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO tasks(id,title,created_at) VALUES(1,'A',1),(2,'B',1);
             INSERT INTO jira_tasks(task_id,site,issue_key) VALUES(1,'https://a.atlassian.net','WMS-1'),(2,'https://a.atlassian.net','WMS-2');",
        )
        .unwrap();
        // Pierwszy zapis: priorytet High → pasmo 1, etykiety zapisane.
        let tx = conn.transaction().unwrap();
        save_fields(
            &tx,
            site,
            "WMS-1",
            "High",
            &[
                value("labels", "backend, pilne"),
                value("assignee", "Anna Nowak"),
            ],
        )
        .unwrap();
        tx.commit().unwrap();
        let snap = db::snapshot(&conn).unwrap();
        let one = snap.tasks.iter().find(|t| t.id == 1).unwrap();
        assert_eq!(one.details.priority, 1);
        assert_eq!(
            one.jira_fields,
            vec![
                value("labels", "backend, pilne"),
                value("assignee", "Anna Nowak")
            ]
        );
        // Ręczna zmiana pasma zostaje, gdy priorytet w Jirze się nie zmienił.
        conn.execute(
            "UPDATE task_details SET json=json_set(json,'$.priority',-1) WHERE task_id=1",
            [],
        )
        .unwrap();
        let tx = conn.transaction().unwrap();
        save_fields(&tx, site, "WMS-1", "High", &[value("labels", "backend")]).unwrap();
        tx.commit().unwrap();
        let snap = db::snapshot(&conn).unwrap();
        let one = snap.tasks.iter().find(|t| t.id == 1).unwrap();
        assert_eq!(one.details.priority, -1);
        assert_eq!(one.jira_fields, vec![value("labels", "backend")]);
        // Zmiana w Jirze na Low → pasmo -1 nadpisane (tu: Medium → 0).
        let tx = conn.transaction().unwrap();
        save_fields(&tx, site, "WMS-1", "Medium", &[]).unwrap();
        tx.commit().unwrap();
        let snap = db::snapshot(&conn).unwrap();
        let one = snap.tasks.iter().find(|t| t.id == 1).unwrap();
        assert_eq!(one.details.priority, 0);
        assert!(one.jira_fields.is_empty());
        // Nieznana nazwa priorytetu nie zmienia zadania; pusty priorytet też nie.
        let tx = conn.transaction().unwrap();
        save_fields(&tx, site, "WMS-2", "P3", &[value("priority", "P3")]).unwrap();
        save_fields(&tx, site, "WMS-2", "", &[]).unwrap();
        tx.commit().unwrap();
        let snap = db::snapshot(&conn).unwrap();
        let two = snap.tasks.iter().find(|t| t.id == 2).unwrap();
        assert_eq!(two.details.priority, 0);
        // Kolejność i wybór pól z ustawień; pole spoza ustawień nie jest pokazywane.
        let tx = conn.transaction().unwrap();
        save_fields(
            &tx,
            site,
            "WMS-2",
            "Low",
            &[
                value("labels", "x"),
                value("customfield_1", "Sprint 1"),
                value("duedate", "20.10.2026"),
            ],
        )
        .unwrap();
        tx.commit().unwrap();
        crate::settings::save(
            &mut conn,
            &json!({"jiraCardFields": ["priority", "customfield_1", "labels"]}),
        )
        .unwrap();
        let snap = db::snapshot(&conn).unwrap();
        let two = snap.tasks.iter().find(|t| t.id == 2).unwrap();
        assert_eq!(two.details.priority, -1);
        assert_eq!(
            two.jira_fields,
            vec![
                value("priority", "Low"),
                value("customfield_1", "Sprint 1"),
                value("labels", "x")
            ]
        );
        crate::settings::save(&mut conn, &json!({"jiraCardFields": []})).unwrap();
        assert!(db::snapshot(&conn)
            .unwrap()
            .tasks
            .iter()
            .all(|t| t.jira_fields.is_empty()));
    }

    #[test]
    fn linked_open_keys_skip_done_and_trashed_tasks() {
        let mut conn = Connection::open_in_memory().unwrap();
        db::init(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO tasks(id,title,done,created_at) VALUES(1,'A',0,1),(2,'B',1,1),(3,'C',0,1),(4,'D',0,1);
             INSERT INTO jira_tasks(task_id,site,issue_key) VALUES(1,'s','WMS-1'),(2,'s','WMS-2'),(3,'s','WMS-3'),(4,'inna','WMS-4');",
        )
        .unwrap();
        db::change(&mut conn, "delete", Some(3), None, 5).unwrap();
        assert_eq!(linked_open_keys(&conn, "s").unwrap(), vec!["WMS-1"]);
    }
}
