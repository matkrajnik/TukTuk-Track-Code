//! Jira z poziomu Todo Track (PLAN_1.7.md, J2): tablica według kolumn, przejścia
//! przez przeciągnięcie, przypisanie, komentarze, nazwa gałęzi i nowe zgłoszenia.
//!
//! Kolumny pochodzą z `GET /rest/agile/1.0/board/{id}/configuration`, a nazwy
//! i kategorie statusów z `GET /rest/api/3/project/{klucz}/statuses`. Oba wyniki
//! są w pamięci podręcznej (`jira_board_columns`, `jira_statuses`,
//! `jira_board_meta`), odświeżanej przy „Odśwież” i raz na dobę. Status spoza
//! kolumn tablicy trafia do kolumny według kategorii (`column_for`).
//!
//! Każdy zapis (przejście, przypisanie, komentarz, tworzenie) idzie przez
//! `jira::write` z głównym przełącznikiem `jira_write`, walidacją i polskimi
//! komunikatami błędów; komendy dodatkowo sprawdzają przełącznik przed odczytem
//! z sieci, więc przy wyłączonych zapisach backend odrzuca je od razu.
use crate::jira::{self, Config};
use crate::{db, Database};
use reqwest::{Client, Method};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use tauri::{Emitter, Manager};

/// Najwięcej zgłoszeń na tablicy („Cała tablica” i „Moje”).
pub const ISSUE_LIMIT: usize = 300;
/// Co ile odświeżać kolumny i statusy bez „Odśwież” (raz na dobę).
const CACHE_TTL: i64 = 24 * 3_600_000;
/// Pola zgłoszeń na kartach tablicy.
const BOARD_FIELDS: &str = "summary,status,issuetype,priority,assignee";
/// Najdłuższy komentarz (znaki).
pub const COMMENT_LIMIT: usize = 5000;
/// Najdłuższa nazwa gałęzi bez prefiksu (klucz i tytuł).
pub const BRANCH_LIMIT: usize = 50;
/// Pola formularza „Nowe zgłoszenie”; inne wymagane pola odsyłają do Jiry.
const CREATE_FIELDS: [&str; 6] = [
    "summary",
    "issuetype",
    "project",
    "description",
    "assignee",
    "reporter",
];

/// Kolumna tablicy: nazwa i identyfikatory statusów.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Column {
    pub name: String,
    pub status_ids: Vec<String>,
}

/// Status projektu: nazwa i kategoria (`new`, `indeterminate`, `done`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusInfo {
    pub id: String,
    pub name: String,
    pub category: String,
}

/// Osoba w Jirze (przypisanie, wyszukiwarka).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Person {
    pub account_id: String,
    pub display_name: String,
}

/// Karta tablicy.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardIssue {
    pub key: String,
    pub summary: String,
    pub status: String,
    pub status_id: String,
    pub category: String,
    pub issue_type: String,
    pub priority: String,
    pub assignee: Option<Person>,
    pub url: String,
    /// Indeks kolumny (`column_for`).
    pub column: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardView {
    board_id: u64,
    project: String,
    columns: Vec<Column>,
    issues: Vec<BoardIssue>,
    /// Więcej zgłoszeń niż `limit`: pokazane jest pierwsze `limit`.
    truncated: bool,
    limit: usize,
    /// Chwila pobrania kolumn i statusów (Unix ms).
    layout_at: i64,
}

/// Przejście z polami ekranu (`expand=transitions.fields`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveOption {
    pub id: String,
    pub name: String,
    pub to_status: String,
    pub to_status_id: String,
    pub to_category: String,
    /// Wymagane pola ekranu bez wartości domyślnej (np. Resolution).
    pub required: Vec<String>,
}

/// Wynik przeciągnięcia karty.
#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "result", rename_all = "camelCase")]
pub enum MoveOutcome {
    #[serde(rename_all = "camelCase")]
    Moved {
        status: String,
        status_id: String,
        category: String,
        column: usize,
    },
    /// Kilka przejść do kolumny: interfejs pyta, które wykonać.
    #[serde(rename_all = "camelCase")]
    Choose { options: Vec<MoveOption> },
    /// Przejście wymaga ekranu z polami: nic się nie zmienia, zgłoszenie otwiera
    /// się w przeglądarce.
    #[serde(rename_all = "camelCase")]
    Screen {
        url: String,
        fields: Vec<String>,
        message: String,
    },
}

/// Decyzja dla przeciągnięcia (czysta funkcja `decide`).
#[derive(Debug, PartialEq)]
pub enum Decision {
    Perform(MoveOption),
    Choose(Vec<MoveOption>),
    Screen(MoveOption),
    NoTransition,
}

/// Identyfikator liczbowy z JSON (tekst albo liczba), najwyżej 20 cyfr.
fn numeric_id(v: &Value) -> Option<String> {
    let id = match v {
        Value::String(s) => s.trim().to_string(),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    jira::valid_transition_id(&id).then_some(id)
}

/// Klucz projektu Jiry: wielka litera, potem wielkie litery, cyfry i `_`.
pub fn valid_project(key: &str) -> bool {
    (1..=20).contains(&key.len())
        && key.as_bytes()[0].is_ascii_uppercase()
        && key
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}

/// Identyfikator konta Atlassian (`557058:…`, `5b10…`).
pub fn valid_account_id(id: &str) -> bool {
    (1..=128).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'-' | b'_'))
}

fn short(text: &str, limit: usize) -> String {
    text.trim().chars().take(limit).collect()
}

/// Odczyt `columnConfig.columns[].statuses[].id` i klucza projektu tablicy.
pub fn parse_board_config(value: &Value) -> Result<(Vec<Column>, String), String> {
    let list = value["columnConfig"]["columns"]
        .as_array()
        .ok_or("Jira nie zwróciła kolumn tablicy.")?;
    let columns: Vec<Column> = list
        .iter()
        .take(50)
        .enumerate()
        .map(|(i, c)| {
            let name = short(c["name"].as_str().unwrap_or(""), 100);
            Column {
                name: if name.is_empty() {
                    format!("Kolumna {}", i + 1)
                } else {
                    name
                },
                status_ids: c["statuses"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|s| numeric_id(&s["id"]))
                    .collect(),
            }
        })
        .collect();
    if columns.is_empty() {
        return Err("Tablica w Jirze nie ma kolumn.".into());
    }
    let location = &value["location"];
    let project = ["key", "projectKey", "projectKeyOrId"]
        .iter()
        .find_map(|k| location[k].as_str().filter(|p| valid_project(p)))
        .unwrap_or("")
        .to_string();
    Ok((columns, project))
}

/// Statusy z `project/{klucz}/statuses` (lista typów zgłoszeń, każdy ze statusami)
/// albo z `/rest/api/3/status` (płaska lista). Bez powtórek identyfikatorów.
pub fn parse_statuses(value: &Value) -> Result<Vec<StatusInfo>, String> {
    let list = value
        .as_array()
        .ok_or("Jira nie zwróciła listy statusów.")?;
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let mut push = |s: &Value| {
        let Some(id) = numeric_id(&s["id"]) else {
            return;
        };
        if seen.insert(id.clone()) {
            out.push(StatusInfo {
                id,
                name: short(s["name"].as_str().unwrap_or(""), 100),
                category: s["statusCategory"]["key"]
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
            });
        }
    };
    for item in list {
        match item["statuses"].as_array() {
            Some(statuses) => statuses.iter().for_each(&mut push),
            None => push(item),
        }
    }
    Ok(out)
}

/// Kolumna dla statusu: kolumna, która go zawiera; inaczej pierwsza kolumna
/// ze statusem tej samej kategorii; inaczej „Do zrobienia” → pierwsza, „Gotowe”
/// → ostatnia, „W toku” → druga (przy co najmniej trzech kolumnach).
pub fn column_for(
    status_id: &str,
    category: &str,
    columns: &[Column],
    categories: &HashMap<String, String>,
) -> usize {
    if columns.is_empty() {
        return 0;
    }
    if let Some(i) = columns
        .iter()
        .position(|c| c.status_ids.iter().any(|s| s == status_id))
    {
        return i;
    }
    if !category.is_empty() {
        if let Some(i) = columns.iter().position(|c| {
            c.status_ids
                .iter()
                .any(|s| categories.get(s).map(String::as_str) == Some(category))
        }) {
            return i;
        }
    }
    let last = columns.len() - 1;
    match category {
        "new" => 0,
        "done" => last,
        _ if columns.len() >= 3 => 1,
        _ => 0,
    }
}

fn person(v: &Value) -> Option<Person> {
    let id = v["accountId"].as_str().filter(|id| valid_account_id(id))?;
    Some(Person {
        account_id: id.to_string(),
        display_name: short(v["displayName"].as_str().unwrap_or(id), 120),
    })
}

/// Dopisuje karty z jednej strony wyniku. `Ok(false)`, gdy zebrano więcej niż
/// `limit` (dalszych stron nie trzeba pobierać).
pub fn collect_issues(
    value: &Value,
    site: &str,
    columns: &[Column],
    categories: &HashMap<String, String>,
    out: &mut Vec<BoardIssue>,
    limit: usize,
) -> Result<bool, String> {
    let issues = value["issues"]
        .as_array()
        .ok_or("Jira nie zwróciła listy zgłoszeń.")?;
    for issue in issues {
        let Some(key) = issue["key"].as_str().filter(|k| jira::valid_key(k)) else {
            continue;
        };
        if out.iter().any(|i| i.key == key) {
            continue;
        }
        let f = &issue["fields"];
        let status_id = numeric_id(&f["status"]["id"]).unwrap_or_default();
        let category = f["status"]["statusCategory"]["key"]
            .as_str()
            .map(str::to_string)
            .or_else(|| categories.get(&status_id).cloned())
            .unwrap_or_default();
        out.push(BoardIssue {
            key: key.to_string(),
            summary: short(f["summary"].as_str().unwrap_or(key), 255),
            status: short(f["status"]["name"].as_str().unwrap_or(""), 100),
            column: column_for(&status_id, &category, columns, categories),
            status_id,
            category,
            issue_type: short(f["issuetype"]["name"].as_str().unwrap_or(""), 60),
            priority: short(f["priority"]["name"].as_str().unwrap_or(""), 60),
            assignee: person(&f["assignee"]),
            url: format!("{site}/browse/{key}"),
        });
    }
    Ok(out.len() <= limit)
}

/// JQL zakresu tablicy: „Moje” jak lista (z „Tylko otwarte”), „Cała tablica” otwarte.
pub fn board_jql(scope: &str, open_only: bool) -> Result<&'static str, String> {
    match (scope, open_only) {
        ("mine", true) => {
            Ok("assignee = currentUser() AND statusCategory != Done ORDER BY Rank ASC")
        }
        ("mine", false) => Ok("assignee = currentUser() ORDER BY Rank ASC"),
        ("all", _) => Ok("statusCategory != Done ORDER BY Rank ASC"),
        _ => Err("Nieznany zakres tablicy.".into()),
    }
}

/// Przejścia z polami ekranu. Pole blokuje przejście, gdy jest wymagane i nie ma
/// wartości domyślnej.
pub fn parse_move_options(value: &Value) -> Result<Vec<MoveOption>, String> {
    let list = value["transitions"]
        .as_array()
        .ok_or("Jira nie zwróciła listy przejść.")?;
    Ok(list
        .iter()
        .filter_map(|t| {
            let id = numeric_id(&t["id"])?;
            let to = &t["to"];
            let to_status = short(to["name"].as_str().unwrap_or(""), 100);
            let name = short(t["name"].as_str().unwrap_or(""), 100);
            let mut required: Vec<String> = t["fields"]
                .as_object()
                .into_iter()
                .flatten()
                .filter(|(_, f)| {
                    f["required"].as_bool() == Some(true)
                        && f["hasDefaultValue"].as_bool() != Some(true)
                })
                .map(|(key, f)| short(f["name"].as_str().unwrap_or(key), 80))
                .collect();
            required.sort();
            Some(MoveOption {
                id,
                name: if name.is_empty() {
                    to_status.clone()
                } else {
                    name
                },
                to_status_id: numeric_id(&to["id"]).unwrap_or_default(),
                to_category: to["statusCategory"]["key"]
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
                to_status,
                required,
            })
        })
        .collect())
}

/// Wybór przejścia do kolumny `target`: przejścia, których status docelowy
/// należy do kolumny (także według kategorii, jak karty). Jedno → wykonaj albo,
/// gdy wymaga pól, ekran w Jirze; kilka bez wyboru → lista; brak → nie ma przejścia.
pub fn decide(
    options: &[MoveOption],
    target: usize,
    columns: &[Column],
    categories: &HashMap<String, String>,
    chosen: Option<&str>,
) -> Result<Decision, String> {
    let candidates: Vec<MoveOption> = options
        .iter()
        .filter(|o| column_for(&o.to_status_id, &o.to_category, columns, categories) == target)
        .cloned()
        .collect();
    let pick = match (chosen, candidates.as_slice()) {
        (Some(id), _) => candidates
            .iter()
            .find(|o| o.id == id)
            .cloned()
            .ok_or("To przejście nie prowadzi do wybranej kolumny.")?,
        (None, []) => return Ok(Decision::NoTransition),
        (None, [one]) => one.clone(),
        (None, _) => return Ok(Decision::Choose(candidates)),
    };
    Ok(if pick.required.is_empty() {
        Decision::Perform(pick)
    } else {
        Decision::Screen(pick)
    })
}

/// Komentarz zwykłym tekstem jako dokument ADF: akapity rozdzielone pustą linią,
/// pojedyncze końce linii jako `hardBreak`.
pub fn adf(text: &str) -> Result<Value, String> {
    let text: String = text
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .chars()
        .filter(|c| *c == '\n' || *c == '\t' || !c.is_control())
        .collect();
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("Wpisz treść komentarza.".into());
    }
    if trimmed.chars().count() > COMMENT_LIMIT {
        return Err(format!(
            "Komentarz może mieć najwyżej {COMMENT_LIMIT} znaków."
        ));
    }
    let mut paragraphs: Vec<Vec<&str>> = vec![Vec::new()];
    for line in trimmed.split('\n') {
        let line = line.trim_end();
        if line.trim().is_empty() {
            if !paragraphs.last().is_some_and(Vec::is_empty) {
                paragraphs.push(Vec::new());
            }
        } else {
            paragraphs.last_mut().unwrap().push(line);
        }
    }
    let content: Vec<Value> = paragraphs
        .into_iter()
        .filter(|p| !p.is_empty())
        .map(|lines| {
            let mut parts = Vec::new();
            for (i, line) in lines.into_iter().enumerate() {
                if i > 0 {
                    parts.push(json!({"type": "hardBreak"}));
                }
                parts.push(json!({"type": "text", "text": line}));
            }
            json!({"type": "paragraph", "content": parts})
        })
        .collect();
    Ok(json!({"type": "doc", "version": 1, "content": content}))
}

/// Litera bez ogonków (polskie i kilka częstych), małymi literami.
fn plain(c: char) -> Option<&'static str> {
    Some(match c {
        'ą' | 'á' | 'ä' | 'à' | 'â' => "a",
        'ć' | 'č' | 'ç' => "c",
        'ę' | 'é' | 'ě' | 'è' | 'ë' | 'ê' => "e",
        'ł' => "l",
        'ń' | 'ň' => "n",
        'ó' | 'ö' | 'ô' | 'ò' => "o",
        'ś' | 'š' => "s",
        'ź' | 'ż' | 'ž' => "z",
        'í' | 'ì' | 'î' | 'ï' => "i",
        'ú' | 'ü' | 'ů' | 'ù' | 'û' => "u",
        'ř' => "r",
        'ý' => "y",
        'ť' => "t",
        'ď' => "d",
        'ß' => "ss",
        _ => return None,
    })
}

/// Nazwa gałęzi: `prefiks` + `KLUCZ-krotki-tytul` (tytuł małymi literami, bez
/// ogonków, inne znaki jako `-`), część bez prefiksu najwyżej 50 znaków.
pub fn branch_name(prefix: &str, key: &str, title: &str) -> String {
    let mut slug = String::new();
    for c in title.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if let Some(p) = plain(c) {
            slug.push_str(p);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    let full = if slug.is_empty() {
        key.to_string()
    } else {
        format!("{key}-{slug}")
    };
    let cut: String = full.chars().take(BRANCH_LIMIT).collect();
    format!("{prefix}{}", cut.trim_end_matches('-'))
}

/// Typ zgłoszenia z `createmeta/{projekt}/issuetypes` (bez podzadań).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueType {
    pub id: String,
    pub name: String,
}

pub fn parse_issue_types(value: &Value) -> Result<Vec<IssueType>, String> {
    let list = value["issueTypes"]
        .as_array()
        .or_else(|| value["values"].as_array())
        .ok_or("Jira nie zwróciła typów zgłoszeń.")?;
    Ok(list
        .iter()
        .filter(|t| t["subtask"].as_bool() != Some(true))
        .filter_map(|t| {
            Some(IssueType {
                id: numeric_id(&t["id"])?,
                name: short(t["name"].as_str().unwrap_or(""), 80),
            })
        })
        .collect())
}

/// Wymagane pola typu zgłoszenia spoza formularza aplikacji (bez wartości domyślnej).
pub fn extra_required(value: &Value) -> Result<Vec<String>, String> {
    let list = value["fields"]
        .as_array()
        .or_else(|| value["values"].as_array())
        .ok_or("Jira nie zwróciła pól typu zgłoszenia.")?;
    let mut out: Vec<String> = list
        .iter()
        .filter(|f| {
            f["required"].as_bool() == Some(true) && f["hasDefaultValue"].as_bool() != Some(true)
        })
        .filter(|f| {
            let id = f["fieldId"].as_str().or(f["key"].as_str()).unwrap_or("");
            !CREATE_FIELDS.contains(&id)
        })
        .map(|f| {
            short(
                f["name"]
                    .as_str()
                    .or(f["fieldId"].as_str())
                    .unwrap_or("pole"),
                80,
            )
        })
        .collect();
    out.sort();
    out.dedup();
    Ok(out)
}

/// Dane formularza „Nowe zgłoszenie”.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NewIssue {
    pub project: String,
    pub issue_type_id: String,
    pub summary: String,
    pub description: String,
    pub assign_me: bool,
    pub create_task: bool,
    /// Dzień planu zadania w Todo Track (`RRRR-MM-DD`, pusty = bez dnia).
    pub planned_day: String,
}

/// Walidacja formularza; zwraca tytuł po przycięciu.
pub fn validate_new_issue(n: &NewIssue) -> Result<String, String> {
    if !valid_project(&n.project) {
        return Err("Wybierz projekt w Jirze.".into());
    }
    if !jira::valid_transition_id(&n.issue_type_id) {
        return Err("Wybierz typ zgłoszenia.".into());
    }
    let summary = n.summary.trim();
    if summary.is_empty() || summary.chars().count() > 255 || summary.contains(['\n', '\r']) {
        return Err("Tytuł zgłoszenia musi mieć od 1 do 255 znaków w jednej linii.".into());
    }
    if n.description.chars().count() > COMMENT_LIMIT {
        return Err(format!("Opis może mieć najwyżej {COMMENT_LIMIT} znaków."));
    }
    if !n.planned_day.is_empty() && !crate::settings::valid_day(&n.planned_day) {
        return Err("Dzień planu musi mieć format RRRR-MM-DD.".into());
    }
    Ok(summary.to_string())
}

/// Treść `POST /rest/api/3/issue`.
pub fn create_body(n: &NewIssue, summary: &str, me: Option<&str>) -> Result<Value, String> {
    let mut fields = json!({
        "project": {"key": n.project},
        "issuetype": {"id": n.issue_type_id},
        "summary": summary,
    });
    if !n.description.trim().is_empty() {
        fields["description"] = adf(&n.description)?;
    }
    if let Some(id) = me {
        fields["assignee"] = json!({"accountId": id});
    }
    Ok(json!({"fields": fields}))
}

/// Kolumny, projekt i statusy tablicy z pamięci podręcznej.
pub struct Layout {
    pub columns: Vec<Column>,
    pub project: String,
    pub statuses: Vec<StatusInfo>,
    pub fetched_at: i64,
}

impl Layout {
    pub fn categories(&self) -> HashMap<String, String> {
        self.statuses
            .iter()
            .map(|s| (s.id.clone(), s.category.clone()))
            .collect()
    }
}

/// Odczyt pamięci podręcznej tablicy dla adresu Jiry; `None`, gdy jej nie ma
/// albo pochodzi z innej Jiry.
pub fn cached_layout(
    db: &Connection,
    site: &str,
    board_id: u64,
) -> rusqlite::Result<Option<Layout>> {
    let meta: Option<(String, i64)> = db
        .query_row(
            "SELECT project, fetched_at FROM jira_board_meta WHERE board_id=?1 AND site=?2",
            params![board_id as i64, site],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((project, fetched_at)) = meta else {
        return Ok(None);
    };
    let mut stmt = db.prepare(
        "SELECT name, status_ids FROM jira_board_columns WHERE board_id=?1 ORDER BY position",
    )?;
    let columns: Vec<Column> = stmt
        .query_map([board_id as i64], |r| {
            let raw: String = r.get(1)?;
            Ok(Column {
                name: r.get(0)?,
                status_ids: serde_json::from_str(&raw).unwrap_or_default(),
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    if columns.is_empty() {
        return Ok(None);
    }
    let mut stmt = db.prepare("SELECT id, name, category FROM jira_statuses WHERE site=?1")?;
    let statuses = stmt
        .query_map([site], |r| {
            Ok(StatusInfo {
                id: r.get(0)?,
                name: r.get(1)?,
                category: r.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(Some(Layout {
        columns,
        project,
        statuses,
        fetched_at,
    }))
}

/// Zastępuje kolumny tablicy i dopisuje statusy w jednej transakcji.
pub fn store_layout(
    db: &mut Connection,
    site: &str,
    board_id: u64,
    layout: &Layout,
) -> rusqlite::Result<()> {
    let tx = db.transaction()?;
    tx.execute(
        "DELETE FROM jira_board_columns WHERE board_id=?1",
        [board_id as i64],
    )?;
    for (i, c) in layout.columns.iter().enumerate() {
        tx.execute(
            "INSERT INTO jira_board_columns(board_id,position,name,status_ids) VALUES(?1,?2,?3,?4)",
            params![
                board_id as i64,
                i as i64,
                c.name,
                serde_json::to_string(&c.status_ids).unwrap_or_else(|_| "[]".into())
            ],
        )?;
    }
    for s in &layout.statuses {
        tx.execute(
            "INSERT INTO jira_statuses(site,id,name,category) VALUES(?1,?2,?3,?4)
             ON CONFLICT(site,id) DO UPDATE SET name=excluded.name, category=excluded.category",
            params![site, s.id, s.name, s.category],
        )?;
    }
    tx.execute(
        "INSERT INTO jira_board_meta(board_id,site,project,fetched_at) VALUES(?1,?2,?3,?4)
         ON CONFLICT(board_id) DO UPDATE SET site=excluded.site, project=excluded.project, fetched_at=excluded.fetched_at",
        params![board_id as i64, site, layout.project, layout.fetched_at],
    )?;
    tx.commit()
}

/// Czy pamięć podręczną trzeba odświeżyć.
pub fn stale(layout: Option<&Layout>, refresh: bool, now: i64) -> bool {
    refresh || layout.is_none_or(|l| now - l.fetched_at > CACHE_TTL || now < l.fetched_at)
}

async fn fetch_layout(
    client: &Client,
    c: &Config,
    secret: &str,
    board_id: u64,
    now: i64,
) -> Result<Layout, String> {
    let config = jira::get(
        client,
        c,
        secret,
        &format!("/rest/agile/1.0/board/{board_id}/configuration"),
        &[],
    )
    .await?;
    let (columns, project) = parse_board_config(&config)?;
    let path = if project.is_empty() {
        "/rest/api/3/status".to_string()
    } else {
        format!("/rest/api/3/project/{project}/statuses")
    };
    let statuses = parse_statuses(&jira::get(client, c, secret, &path, &[]).await?)?;
    Ok(Layout {
        columns,
        project,
        statuses,
        fetched_at: now,
    })
}

/// Kolumny i statusy tablicy: z pamięci podręcznej albo (przy „Odśwież” i raz na
/// dobę) z Jiry. Błąd pobrania przy istniejącej pamięci podręcznej jej nie usuwa.
async fn layout(
    app: &tauri::AppHandle,
    client: &Client,
    c: &Config,
    secret: &str,
    board_id: u64,
    refresh: bool,
) -> Result<Layout, String> {
    let cached = {
        let state = app.state::<Database>();
        let db = state.0.lock().map_err(|e| e.to_string())?;
        cached_layout(&db, &c.site, board_id).map_err(|e| e.to_string())?
    };
    let now = db::now();
    if !stale(cached.as_ref(), refresh, now) {
        if let Some(cached) = cached {
            return Ok(cached);
        }
    }
    match fetch_layout(client, c, secret, board_id, now).await {
        Ok(fresh) => {
            let state = app.state::<Database>();
            let mut db = state.0.lock().map_err(|e| e.to_string())?;
            store_layout(&mut db, &c.site, board_id, &fresh).map_err(|e| e.to_string())?;
            // Statusy innych tablic tej samej Jiry zostają w odczycie.
            Ok(cached_layout(&db, &c.site, board_id)
                .map_err(|e| e.to_string())?
                .unwrap_or(fresh))
        }
        Err(e) => match cached {
            Some(cached) if !refresh => Ok(cached),
            _ => Err(e),
        },
    }
}

/// Połączone konto i wybrana tablica.
fn connected(app: &tauri::AppHandle) -> Result<(Config, String), String> {
    let c = jira::config(app)?;
    if c.email.is_empty() {
        return Err("Połącz konto Jiry w widoku Jira.".into());
    }
    let secret = jira::token(app, &c)?;
    Ok((c, secret))
}

fn board_name(c: &Config, board_id: u64) -> Result<String, String> {
    c.boards
        .iter()
        .find(|b| b.id == board_id)
        .map(|b| b.name.clone())
        .ok_or_else(|| "Tej tablicy nie ma wśród wybranych. Wybierz ją w widoku Jira.".into())
}

fn ensure_write(app: &tauri::AppHandle) -> Result<(), String> {
    jira::ensure_write(&crate::settings::current(app))
}

fn check_key(key: &str) -> Result<(), String> {
    if jira::valid_key(key) {
        Ok(())
    } else {
        Err("Niepoprawny klucz zgłoszenia Jiry.".into())
    }
}

/// Tablica: kolumny, karty (najwyżej 300) i przypisanie kart do kolumn.
#[tauri::command]
pub async fn jira_board(
    app: tauri::AppHandle,
    board_id: u64,
    scope: String,
    open_only: bool,
    refresh: bool,
) -> Result<BoardView, String> {
    let jql = board_jql(&scope, open_only)?;
    let (c, secret) = connected(&app)?;
    let name = board_name(&c, board_id)?;
    let client = jira::client()?;
    let layout = layout(&app, &client, &c, &secret, board_id, refresh).await?;
    let categories = layout.categories();
    let mut issues = Vec::new();
    jira::pages(
        &client,
        &c,
        &secret,
        &format!("/rest/software/1.0/board/{board_id}/issue"),
        jql,
        BOARD_FIELDS,
        |value| {
            collect_issues(
                value,
                &c.site,
                &layout.columns,
                &categories,
                &mut issues,
                ISSUE_LIMIT,
            )
        },
    )
    .await?;
    let truncated = issues.len() > ISSUE_LIMIT;
    issues.truncate(ISSUE_LIMIT);
    {
        let state = app.state::<Database>();
        let mut db = state.0.lock().map_err(|e| e.to_string())?;
        let tx = db.transaction().map_err(|e| e.to_string())?;
        jira::save_statuses(
            &tx,
            &c.site,
            issues.iter().map(|i| (i.key.as_str(), i.status.as_str())),
        )
        .and_then(|_| tx.commit())
        .map_err(|e| e.to_string())?;
    }
    jira::remember(
        &app,
        &issues
            .iter()
            .map(|i| (i.key.clone(), i.summary.clone(), i.status.clone()))
            .collect::<Vec<_>>(),
        &name,
    );
    let _ = app.emit("data-changed", ());
    Ok(BoardView {
        board_id,
        project: layout.project,
        columns: layout.columns,
        issues,
        truncated,
        limit: ISSUE_LIMIT,
        layout_at: layout.fetched_at,
    })
}

/// Przeciągnięcie karty do kolumny (i „Przesuń do…”): wybór przejścia, którego
/// status docelowy należy do kolumny, i wykonanie przez wspólny zapis z J1.
#[tauri::command]
pub async fn jira_board_move(
    app: tauri::AppHandle,
    board_id: u64,
    key: String,
    column: usize,
    transition_id: Option<String>,
) -> Result<MoveOutcome, String> {
    ensure_write(&app)?;
    check_key(&key)?;
    if transition_id
        .as_deref()
        .is_some_and(|id| !jira::valid_transition_id(id))
    {
        return Err("Niepoprawne przejście w Jirze.".into());
    }
    let (c, secret) = connected(&app)?;
    board_name(&c, board_id)?;
    let client = jira::client()?;
    let layout = layout(&app, &client, &c, &secret, board_id, false).await?;
    let target = layout
        .columns
        .get(column)
        .ok_or("Nie ma takiej kolumny na tablicy.")?
        .name
        .clone();
    let value = jira::get(
        &client,
        &c,
        &secret,
        &format!("/rest/api/3/issue/{key}/transitions"),
        &[("expand", "transitions.fields".to_string())],
    )
    .await?;
    let options = parse_move_options(&value)?;
    let categories = layout.categories();
    let url = format!("{}/browse/{}", c.site, key);
    match decide(
        &options,
        column,
        &layout.columns,
        &categories,
        transition_id.as_deref(),
    )? {
        Decision::NoTransition => Err(format!(
            "Jira nie ma przejścia {key} do kolumny „{target}”. Zgłoszenie zostaje w poprzedniej kolumnie."
        )),
        Decision::Choose(options) => Ok(MoveOutcome::Choose { options }),
        Decision::Screen(o) => Ok(MoveOutcome::Screen {
            message: format!(
                "Przejście „{}” wymaga wypełnienia pól w Jirze ({}). Otwieram {key} w przeglądarce — dokończ zmianę tam.",
                o.name,
                o.required.join(", ")
            ),
            url,
            fields: o.required,
        }),
        Decision::Perform(o) => {
            jira::transition(&app, &key, &o.id).await?;
            jira::store_status(&app, &c.site, &key, &o.to_status);
            let _ = app.emit("data-changed", ());
            Ok(MoveOutcome::Moved {
                column: column_for(&o.to_status_id, &o.to_category, &layout.columns, &categories),
                status: o.to_status,
                status_id: o.to_status_id,
                category: o.to_category,
            })
        }
    }
}

/// Lista osób z `GET /rest/api/3/user/assignable/search` (aktywne konta).
pub fn parse_people(value: &Value) -> Result<Vec<Person>, String> {
    let list = value.as_array().ok_or("Jira nie zwróciła listy osób.")?;
    Ok(list
        .iter()
        .filter(|u| u["active"].as_bool() != Some(false))
        .filter(|u| u["accountType"].as_str().is_none_or(|t| t == "atlassian"))
        .filter_map(person)
        .take(20)
        .collect())
}

/// „Zmień osobę…”: osoby, które można przypisać do zgłoszenia.
#[tauri::command]
pub async fn jira_assignable(
    app: tauri::AppHandle,
    key: String,
    query: String,
) -> Result<Vec<Person>, String> {
    check_key(&key)?;
    let query = query.trim();
    if query.chars().count() > 100 {
        return Err("Wpisz najwyżej 100 znaków.".into());
    }
    let (c, secret) = connected(&app)?;
    let value = jira::get(
        &jira::client()?,
        &c,
        &secret,
        "/rest/api/3/user/assignable/search",
        &[
            ("issueKey", key),
            ("query", query.to_string()),
            ("maxResults", "20".into()),
        ],
    )
    .await?;
    parse_people(&value)
}

async fn myself(client: &Client, c: &Config, secret: &str) -> Result<Person, String> {
    let value = jira::get(client, c, secret, "/rest/api/3/myself", &[]).await?;
    person(&value).ok_or_else(|| "Nie udało się potwierdzić konta Jiry.".into())
}

/// „Przypisz do mnie” (`account_id` = null) i „Zmień osobę…”.
#[tauri::command]
pub async fn jira_assign(
    app: tauri::AppHandle,
    key: String,
    account_id: Option<String>,
) -> Result<Person, String> {
    ensure_write(&app)?;
    check_key(&key)?;
    let who = match account_id {
        Some(id) if valid_account_id(&id) => Person {
            account_id: id,
            display_name: String::new(),
        },
        Some(_) => return Err("Niepoprawna osoba w Jirze.".into()),
        None => {
            let (c, secret) = connected(&app)?;
            myself(&jira::client()?, &c, &secret).await?
        }
    };
    jira::write(
        &app,
        Method::PUT,
        &format!("/rest/api/3/issue/{key}/assignee"),
        &json!({"accountId": who.account_id}),
    )
    .await?;
    Ok(who)
}

/// „Dodaj komentarz”: zwykły tekst zamieniony na ADF.
#[tauri::command]
pub async fn jira_comment(app: tauri::AppHandle, key: String, text: String) -> Result<(), String> {
    ensure_write(&app)?;
    check_key(&key)?;
    let body = json!({"body": adf(&text)?});
    jira::write(
        &app,
        Method::POST,
        &format!("/rest/api/3/issue/{key}/comment"),
        &body,
    )
    .await
    .map(|_| ())
}

/// „Kopiuj nazwę gałęzi” z prefiksem z ustawień.
#[tauri::command]
pub fn jira_branch_name(
    app: tauri::AppHandle,
    key: String,
    title: String,
) -> Result<String, String> {
    check_key(&key)?;
    Ok(branch_name(
        &crate::settings::current(&app).jira_branch_prefix,
        &key,
        &title,
    ))
}

/// Projekt tablicy w formularzu „Nowe zgłoszenie”.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    key: String,
    boards: Vec<String>,
}

/// Projekty wybranych tablic (z konfiguracji tablic w pamięci podręcznej).
#[tauri::command]
pub async fn jira_projects(app: tauri::AppHandle) -> Result<Vec<Project>, String> {
    let (c, secret) = connected(&app)?;
    let client = jira::client()?;
    let mut out: Vec<Project> = Vec::new();
    let mut errors = Vec::new();
    for board in &c.boards {
        match layout(&app, &client, &c, &secret, board.id, false).await {
            Ok(l) if !l.project.is_empty() => match out.iter_mut().find(|p| p.key == l.project) {
                Some(p) => p.boards.push(board.name.clone()),
                None => out.push(Project {
                    key: l.project,
                    boards: vec![board.name.clone()],
                }),
            },
            Ok(_) => {}
            Err(e) => errors.push(e),
        }
    }
    if out.is_empty() {
        return Err(errors
            .pop()
            .unwrap_or_else(|| "Wybrane tablice nie należą do projektu Jiry.".into()));
    }
    Ok(out)
}

/// Typy zgłoszeń projektu (`createmeta/{projekt}/issuetypes`).
#[tauri::command]
pub async fn jira_issue_types(
    app: tauri::AppHandle,
    project: String,
) -> Result<Vec<IssueType>, String> {
    if !valid_project(&project) {
        return Err("Wybierz projekt w Jirze.".into());
    }
    let (c, secret) = connected(&app)?;
    let value = jira::get(
        &jira::client()?,
        &c,
        &secret,
        &format!("/rest/api/3/issue/createmeta/{project}/issuetypes"),
        &[("maxResults", "100".into())],
    )
    .await?;
    parse_issue_types(&value)
}

async fn type_required(
    client: &Client,
    c: &Config,
    secret: &str,
    project: &str,
    type_id: &str,
) -> Result<Vec<String>, String> {
    let value = jira::get(
        client,
        c,
        secret,
        &format!("/rest/api/3/issue/createmeta/{project}/issuetypes/{type_id}"),
        &[("maxResults", "200".into())],
    )
    .await?;
    extra_required(&value)
}

/// Wymagane pola typu spoza formularza i adres tworzenia w Jirze.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeFields {
    extra_required: Vec<String>,
    create_url: String,
}

#[tauri::command]
pub async fn jira_issue_type_fields(
    app: tauri::AppHandle,
    project: String,
    issue_type_id: String,
) -> Result<TypeFields, String> {
    if !valid_project(&project) || !jira::valid_transition_id(&issue_type_id) {
        return Err("Wybierz projekt i typ zgłoszenia.".into());
    }
    let (c, secret) = connected(&app)?;
    Ok(TypeFields {
        extra_required: type_required(&jira::client()?, &c, &secret, &project, &issue_type_id)
            .await?,
        create_url: format!("{}{}", c.site, jira::CREATE_PATH),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Created {
    key: String,
    url: String,
    /// Dane po utworzeniu zadania w Todo Track (`create_task`).
    snapshot: Option<db::Snapshot>,
}

/// Zadanie w Todo Track dla nowego zgłoszenia (bez startu pomiaru).
pub fn create_linked_task(
    db: &mut Connection,
    site: &str,
    key: &str,
    summary: &str,
    planned_day: &str,
    now: i64,
) -> Result<db::Snapshot, String> {
    let tx = db.transaction().map_err(|e| e.to_string())?;
    let title: String = summary.chars().take(240).collect();
    tx.execute(
        "INSERT INTO tasks(title,created_at) VALUES(?1,?2)",
        params![title, now],
    )
    .map_err(|e| e.to_string())?;
    let id = tx.last_insert_rowid();
    tx.execute(
        "INSERT INTO jira_tasks(task_id,site,issue_key) VALUES(?1,?2,?3)",
        params![id, site, key],
    )
    .map_err(|e| e.to_string())?;
    if !planned_day.is_empty() {
        tx.execute(
            "INSERT INTO task_details(task_id,json) VALUES(?1,json_object('plannedDay',?2))",
            params![id, planned_day],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    db::snapshot(db).map_err(|e| e.to_string())
}

/// „Nowe zgłoszenie”: `POST /rest/api/3/issue`, opcjonalnie zadanie w Todo Track.
#[tauri::command]
pub async fn jira_create(app: tauri::AppHandle, issue: NewIssue) -> Result<Created, String> {
    ensure_write(&app)?;
    let summary = validate_new_issue(&issue)?;
    let (c, secret) = connected(&app)?;
    let client = jira::client()?;
    let extra = type_required(&client, &c, &secret, &issue.project, &issue.issue_type_id).await?;
    if !extra.is_empty() {
        return Err(format!(
            "Ten typ zgłoszenia wymaga pól, których Todo Track nie wypełnia ({}). Utwórz zgłoszenie w Jirze.",
            extra.join(", ")
        ));
    }
    let me = if issue.assign_me {
        Some(myself(&client, &c, &secret).await?.account_id)
    } else {
        None
    };
    let body = create_body(&issue, &summary, me.as_deref())?;
    let value = jira::write(&app, Method::POST, "/rest/api/3/issue", &body).await?;
    let key = value["key"]
        .as_str()
        .filter(|k| jira::valid_key(k))
        .ok_or("Jira utworzyła zgłoszenie, ale nie zwróciła jego klucza. Odśwież tablicę.")?
        .to_string();
    let snapshot = if issue.create_task {
        let state = app.state::<Database>();
        let mut db = state.0.lock().map_err(|e| e.to_string())?;
        Some(create_linked_task(
            &mut db,
            &c.site,
            &key,
            &summary,
            &issue.planned_day,
            db::now(),
        )?)
    } else {
        None
    };
    let _ = app.emit("data-changed", ());
    Ok(Created {
        url: format!("{}/browse/{}", c.site, key),
        key,
        snapshot,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(name: &str, ids: &[&str]) -> Column {
        Column {
            name: name.into(),
            status_ids: ids.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn board() -> (Vec<Column>, HashMap<String, String>) {
        let columns = vec![
            col("Do zrobienia", &["1"]),
            col("W toku", &["3"]),
            col("Review", &["4"]),
            col("Gotowe", &["5"]),
        ];
        let categories = [
            ("1", "new"),
            ("3", "indeterminate"),
            ("4", "indeterminate"),
            ("5", "done"),
            ("6", "indeterminate"),
        ]
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        (columns, categories)
    }

    #[test]
    fn parses_board_configuration_and_project() {
        let value = json!({
            "id": 1, "name": "WMS",
            "location": {"type": "project", "key": "WMS", "id": "10000"},
            "columnConfig": {"columns": [
                {"name": "Backlog", "statuses": []},
                {"name": "Do zrobienia", "statuses": [{"id": "10000", "self": "x"}, {"id": 10001}]},
                {"name": "", "statuses": [{"id": "3"}, {"id": "../x"}]}
            ]}
        });
        let (columns, project) = parse_board_config(&value).unwrap();
        assert_eq!(project, "WMS");
        assert_eq!(
            columns,
            vec![
                col("Backlog", &[]),
                col("Do zrobienia", &["10000", "10001"]),
                col("Kolumna 3", &["3"]),
            ]
        );
        // Lokalizacja bez klucza projektu: statusy z /rest/api/3/status.
        let (_, project) = parse_board_config(&json!({
            "location": {"type": "user", "projectKeyOrId": "10000"},
            "columnConfig": {"columns": [{"name": "A", "statuses": []}]}
        }))
        .unwrap();
        assert_eq!(project, "");
        assert!(parse_board_config(&json!({"columnConfig": {"columns": []}})).is_err());
        assert!(parse_board_config(&json!({})).is_err());
    }

    #[test]
    fn parses_project_statuses_without_duplicates() {
        let value = json!([
            {"id": "10001", "name": "Zadanie", "statuses": [
                {"id": "1", "name": "Do zrobienia", "statusCategory": {"key": "new"}},
                {"id": "3", "name": "W toku", "statusCategory": {"key": "indeterminate"}}
            ]},
            {"id": "10002", "name": "Błąd", "statuses": [
                {"id": "3", "name": "W toku", "statusCategory": {"key": "indeterminate"}},
                {"id": 5, "name": "Gotowe", "statusCategory": {"key": "done"}}
            ]}
        ]);
        let list = parse_statuses(&value).unwrap();
        assert_eq!(
            list.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            ["1", "3", "5"]
        );
        assert_eq!(list[2].name, "Gotowe");
        assert_eq!(list[2].category, "done");
        // Płaska lista z /rest/api/3/status.
        let flat = parse_statuses(&json!([
            {"id": "7", "name": "Testy", "statusCategory": {"key": "indeterminate"}}
        ]))
        .unwrap();
        assert_eq!(flat[0].name, "Testy");
        assert!(parse_statuses(&json!({"values": []})).is_err());
    }

    #[test]
    fn assigns_statuses_to_columns_with_category_fallback() {
        let (columns, categories) = board();
        // Status w kolumnie.
        assert_eq!(column_for("4", "indeterminate", &columns, &categories), 2);
        assert_eq!(column_for("5", "done", &columns, &categories), 3);
        // Status spoza kolumn: pierwsza kolumna z tą samą kategorią.
        assert_eq!(column_for("6", "indeterminate", &columns, &categories), 1);
        assert_eq!(column_for("99", "done", &columns, &categories), 3);
        // Kategoria bez kolumny: Do zrobienia → pierwsza, Gotowe → ostatnia.
        let only = vec![col("A", &["3"]), col("B", &["4"]), col("C", &["4"])];
        assert_eq!(column_for("1", "new", &only, &categories), 0);
        assert_eq!(column_for("5", "done", &only, &categories), 2);
        assert_eq!(column_for("9", "", &columns, &categories), 1);
        assert_eq!(column_for("9", "indeterminate", &[], &categories), 0);
    }

    #[test]
    fn collects_board_cards_with_limit() {
        let (columns, categories) = board();
        let value = json!({"issues": [
            {"key": "WMS-1", "fields": {
                "summary": "Import", "status": {"id": "6", "name": "Testy", "statusCategory": {"key": "indeterminate"}},
                "issuetype": {"name": "Zadanie"}, "priority": {"name": "High"},
                "assignee": {"accountId": "557058:abc", "displayName": "Anna Nowak"}}},
            {"key": "WMS-2", "fields": {"summary": "Eksport", "status": {"id": "5", "name": "Gotowe"}}},
            {"key": "../3", "fields": {}}
        ]});
        let mut out = Vec::new();
        assert!(collect_issues(
            &value,
            "https://a.atlassian.net",
            &columns,
            &categories,
            &mut out,
            300
        )
        .unwrap());
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].column, 1);
        assert_eq!(out[0].issue_type, "Zadanie");
        assert_eq!(out[0].priority, "High");
        assert_eq!(
            out[0].assignee,
            Some(Person {
                account_id: "557058:abc".into(),
                display_name: "Anna Nowak".into()
            })
        );
        assert_eq!(out[0].url, "https://a.atlassian.net/browse/WMS-1");
        // Kategoria z pamięci podręcznej, gdy odpowiedź jej nie ma.
        assert_eq!(out[1].category, "done");
        assert_eq!(out[1].column, 3);
        assert_eq!(out[1].assignee, None);
        // Powyżej limitu: koniec pobierania; powtórzony klucz nie dubluje karty.
        assert!(!collect_issues(&value, "s", &columns, &categories, &mut Vec::new(), 1).unwrap());
        let before = out.len();
        collect_issues(&value, "s", &columns, &categories, &mut out, 300).unwrap();
        assert_eq!(out.len(), before);
        assert!(board_jql("all", true)
            .unwrap()
            .starts_with("statusCategory != Done"));
        assert!(board_jql("mine", false)
            .unwrap()
            .starts_with("assignee = currentUser()"));
        assert!(board_jql("x", true).is_err());
    }

    fn option(id: &str, to: &str, to_id: &str, cat: &str, required: &[&str]) -> MoveOption {
        MoveOption {
            id: id.into(),
            name: to.into(),
            to_status: to.into(),
            to_status_id: to_id.into(),
            to_category: cat.into(),
            required: required.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn parses_transitions_with_required_fields() {
        let value = json!({"transitions": [
            {"id": "21", "name": "Start", "to": {"id": "3", "name": "W toku", "statusCategory": {"key": "indeterminate"}}, "fields": {}},
            {"id": "41", "name": "Zamknij", "to": {"id": "5", "name": "Gotowe", "statusCategory": {"key": "done"}},
             "fields": {
                "resolution": {"required": true, "name": "Resolution", "hasDefaultValue": false},
                "comment": {"required": false, "name": "Komentarz"},
                "fixVersions": {"required": true, "name": "Wersja", "hasDefaultValue": true}
             }},
            {"id": "x", "to": {}}
        ]});
        let list = parse_move_options(&value).unwrap();
        assert_eq!(
            list,
            vec![
                MoveOption {
                    name: "Start".into(),
                    ..option("21", "W toku", "3", "indeterminate", &[])
                },
                MoveOption {
                    name: "Zamknij".into(),
                    ..option("41", "Gotowe", "5", "done", &["Resolution"])
                },
            ]
        );
        assert!(parse_move_options(&json!({})).is_err());
    }

    #[test]
    fn chooses_transition_into_column() {
        let (columns, categories) = board();
        let options = vec![
            option("11", "Do zrobienia", "1", "new", &[]),
            option("21", "W toku", "3", "indeterminate", &[]),
            option("22", "Testy", "6", "indeterminate", &[]),
            option("31", "Review", "4", "indeterminate", &[]),
            option("41", "Gotowe", "5", "done", &["Resolution"]),
        ];
        // Jedno przejście do kolumny.
        assert_eq!(
            decide(&options, 2, &columns, &categories, None).unwrap(),
            Decision::Perform(options[3].clone())
        );
        // Kilka (status spoza kolumn według kategorii też trafia do „W toku”): lista.
        assert_eq!(
            decide(&options, 1, &columns, &categories, None).unwrap(),
            Decision::Choose(vec![options[1].clone(), options[2].clone()])
        );
        assert_eq!(
            decide(&options, 1, &columns, &categories, Some("22")).unwrap(),
            Decision::Perform(options[2].clone())
        );
        assert!(decide(&options, 1, &columns, &categories, Some("41")).is_err());
        // Wymagane pola: tylko ekran w Jirze.
        assert_eq!(
            decide(&options, 3, &columns, &categories, None).unwrap(),
            Decision::Screen(options[4].clone())
        );
        // Brak przejścia do kolumny.
        assert_eq!(
            decide(&options[..2], 3, &columns, &categories, None).unwrap(),
            Decision::NoTransition
        );
    }

    #[test]
    fn comment_text_becomes_adf_paragraphs() {
        let doc = adf("Pierwsza linia\r\ndruga\n\n\n  Nowy akapit  \n").unwrap();
        assert_eq!(
            doc,
            json!({"type": "doc", "version": 1, "content": [
                {"type": "paragraph", "content": [
                    {"type": "text", "text": "Pierwsza linia"},
                    {"type": "hardBreak"},
                    {"type": "text", "text": "druga"}
                ]},
                {"type": "paragraph", "content": [{"type": "text", "text": "  Nowy akapit"}]}
            ]})
        );
        assert_eq!(adf("  \n ").unwrap_err(), "Wpisz treść komentarza.");
        assert!(adf(&"x".repeat(COMMENT_LIMIT + 1)).is_err());
        assert!(adf(&"x".repeat(COMMENT_LIMIT)).is_ok());
        // Znaki sterujące nie trafiają do Jiry.
        assert_eq!(
            adf("a\u{0}b").unwrap()["content"][0]["content"][0]["text"],
            "ab"
        );
    }

    #[test]
    fn branch_name_strips_polish_letters_and_limits_length() {
        assert_eq!(
            branch_name("feature/", "WMS-123", "Krótki tytuł"),
            "feature/WMS-123-krotki-tytul"
        );
        assert_eq!(
            branch_name("", "WMS-1", "Zażółć gęślą jaźń — ŁÓDŹ!"),
            "WMS-1-zazolc-gesla-jazn-lodz"
        );
        let long = branch_name(
            "bugfix/",
            "WMS-42",
            &"Bardzo długi tytuł zgłoszenia ".repeat(5),
        );
        let name = long.strip_prefix("bugfix/").unwrap();
        assert!(name.len() <= BRANCH_LIMIT, "{name}");
        assert!(!name.ends_with('-'));
        assert!(name.starts_with("WMS-42-bardzo-dlugi"));
        assert_eq!(branch_name("feature/", "WMS-9", "!!!"), "feature/WMS-9");
        assert!(crate::settings::valid_branch_prefix("feature/"));
        assert!(crate::settings::valid_branch_prefix(""));
        assert!(!crate::settings::valid_branch_prefix("feature /"));
        assert!(!crate::settings::valid_branch_prefix("../x"));
        let patched = crate::settings::apply(
            &crate::settings::AppSettings::default(),
            &json!({"jiraBranchPrefix": " bugfix/ "}),
        )
        .unwrap();
        assert_eq!(patched.jira_branch_prefix, "bugfix/");
    }

    #[test]
    fn create_meta_reports_fields_outside_the_form() {
        let types = parse_issue_types(&json!({"issueTypes": [
            {"id": "10001", "name": "Zadanie", "subtask": false},
            {"id": "10003", "name": "Podzadanie", "subtask": true}
        ]}))
        .unwrap();
        assert_eq!(
            types,
            vec![IssueType {
                id: "10001".into(),
                name: "Zadanie".into()
            }]
        );
        let fields = json!({"fields": [
            {"fieldId": "summary", "name": "Summary", "required": true},
            {"fieldId": "issuetype", "name": "Typ", "required": true},
            {"fieldId": "reporter", "name": "Reporter", "required": true, "hasDefaultValue": true},
            {"fieldId": "customfield_1", "name": "Środowisko", "required": true, "hasDefaultValue": false},
            {"fieldId": "priority", "name": "Priorytet", "required": true, "hasDefaultValue": true},
            {"fieldId": "labels", "name": "Etykiety", "required": false}
        ]});
        assert_eq!(extra_required(&fields).unwrap(), ["Środowisko"]);
        assert!(extra_required(&json!({"values": []})).unwrap().is_empty());
    }

    #[test]
    fn validates_new_issue_and_builds_body() {
        let mut n = NewIssue {
            project: "WMS".into(),
            issue_type_id: "10001".into(),
            summary: "  Nowy raport  ".into(),
            description: "Opis\n\nDruga część".into(),
            assign_me: true,
            create_task: true,
            planned_day: "2026-10-16".into(),
        };
        assert_eq!(validate_new_issue(&n).unwrap(), "Nowy raport");
        let body = create_body(&n, "Nowy raport", Some("557058:me")).unwrap();
        assert_eq!(body["fields"]["project"]["key"], "WMS");
        assert_eq!(body["fields"]["issuetype"]["id"], "10001");
        assert_eq!(body["fields"]["assignee"]["accountId"], "557058:me");
        assert_eq!(
            body["fields"]["description"]["content"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        n.description.clear();
        assert!(create_body(&n, "x", None).unwrap()["fields"]
            .get("description")
            .is_none());
        for bad in [
            NewIssue {
                project: "wms".into(),
                ..n.clone()
            },
            NewIssue {
                issue_type_id: "a".into(),
                ..n.clone()
            },
            NewIssue {
                summary: " ".into(),
                ..n.clone()
            },
            NewIssue {
                summary: "a\nb".into(),
                ..n.clone()
            },
            NewIssue {
                summary: "x".repeat(256),
                ..n.clone()
            },
            NewIssue {
                planned_day: "2026-02-30".into(),
                ..n.clone()
            },
        ] {
            assert!(validate_new_issue(&bad).is_err());
        }
        assert!(valid_account_id("557058:0f1e-ab_c"));
        assert!(!valid_account_id("a/b"));
        assert!(!valid_account_id(""));
    }

    #[test]
    fn people_search_keeps_active_users() {
        let value = json!([
            {"accountId": "a1", "displayName": "Anna Nowak", "active": true, "accountType": "atlassian"},
            {"accountId": "a2", "displayName": "Stary", "active": false},
            {"accountId": "bot", "displayName": "Automat", "accountType": "app"},
            {"accountId": "a/3", "displayName": "Zły"}
        ]);
        assert_eq!(
            parse_people(&value).unwrap(),
            vec![Person {
                account_id: "a1".into(),
                display_name: "Anna Nowak".into()
            }]
        );
    }

    #[test]
    fn layout_cache_replaces_columns_and_keeps_site() {
        let mut conn = Connection::open_in_memory().unwrap();
        db::init(&conn).unwrap();
        let site = "https://a.atlassian.net";
        assert!(cached_layout(&conn, site, 7).unwrap().is_none());
        let first = Layout {
            columns: vec![col("A", &["1"]), col("B", &["3", "4"])],
            project: "WMS".into(),
            statuses: vec![StatusInfo {
                id: "1".into(),
                name: "Do zrobienia".into(),
                category: "new".into(),
            }],
            fetched_at: 1_000,
        };
        store_layout(&mut conn, site, 7, &first).unwrap();
        let read = cached_layout(&conn, site, 7).unwrap().unwrap();
        assert_eq!(read.columns, first.columns);
        assert_eq!(read.project, "WMS");
        assert_eq!(read.categories()["1"], "new");
        // Inna Jira: pamięć podręczna nie pasuje.
        assert!(cached_layout(&conn, "https://b.atlassian.net", 7)
            .unwrap()
            .is_none());
        // Odświeżenie zastępuje kolumny (mniej kolumn niż poprzednio).
        let second = Layout {
            columns: vec![col("Jedna", &["5"])],
            project: "WMS".into(),
            statuses: vec![StatusInfo {
                id: "1".into(),
                name: "To Do".into(),
                category: "new".into(),
            }],
            fetched_at: 2_000,
        };
        store_layout(&mut conn, site, 7, &second).unwrap();
        let read = cached_layout(&conn, site, 7).unwrap().unwrap();
        assert_eq!(read.columns, second.columns);
        assert_eq!(read.statuses[0].name, "To Do");
        assert_eq!(read.fetched_at, 2_000);
        // Raz na dobę albo przy „Odśwież”.
        assert!(!stale(Some(&read), false, 2_000 + CACHE_TTL));
        assert!(stale(Some(&read), false, 2_001 + CACHE_TTL));
        assert!(stale(Some(&read), true, 2_000));
        assert!(stale(None, false, 0));
    }

    #[test]
    fn new_issue_task_is_linked_and_planned_without_measurement() {
        let mut conn = Connection::open_in_memory().unwrap();
        db::init(&conn).unwrap();
        let s = create_linked_task(
            &mut conn,
            "https://a.atlassian.net",
            "WMS-77",
            "Nowe",
            "2026-10-17",
            5,
        )
        .unwrap();
        assert_eq!(s.tasks.len(), 1);
        assert_eq!(s.tasks[0].jira_key.as_deref(), Some("WMS-77"));
        assert_eq!(s.tasks[0].details.planned_day, "2026-10-17");
        assert!(s.sessions.is_empty());
        let s = create_linked_task(
            &mut conn,
            "https://a.atlassian.net",
            "WMS-78",
            "Bez dnia",
            "",
            6,
        )
        .unwrap();
        assert_eq!(s.tasks[0].details.planned_day, "");
    }

    /// Serwer HTTP na jedno połączenie: zwraca całe żądanie.
    fn serve_once(status: &'static str, body: String) -> (String, std::thread::JoinHandle<String>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 4096];
            loop {
                let n = socket.read(&mut buffer).unwrap();
                assert!(n > 0);
                request.extend_from_slice(&buffer[..n]);
                let text = String::from_utf8_lossy(&request).to_string();
                if let Some(end) = text.find("\r\n\r\n") {
                    let length = text
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            write!(socket, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
            String::from_utf8(request).unwrap()
        });
        (format!("http://{address}"), server)
    }

    fn config(site: String) -> Config {
        Config {
            site,
            email: "test@example.invalid".into(),
            ..Config::default()
        }
    }

    #[test]
    fn comment_and_assignee_writes_send_json_and_map_400_403() {
        let (site, server) = serve_once("201 Created", r#"{"id":"1"}"#.into());
        let body = json!({"body": adf("Gotowe do testów").unwrap()});
        tauri::async_runtime::block_on(jira::send_write(
            &jira::client().unwrap(),
            &config(site),
            "t",
            Method::POST,
            "/rest/api/3/issue/WMS-1/comment",
            &body,
        ))
        .unwrap();
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /rest/api/3/issue/WMS-1/comment "));
        assert!(request.contains(r#""type":"doc""#) && request.contains("Gotowe do testów"));
        // 403 przy przypisaniu: brak uprawnień, po polsku.
        let (site, server) = serve_once(
            "403 Forbidden",
            r#"{"errorMessages":["No permission"]}"#.into(),
        );
        let e = tauri::async_runtime::block_on(jira::send_write(
            &jira::client().unwrap(),
            &config(site),
            "t",
            Method::PUT,
            "/rest/api/3/issue/WMS-1/assignee",
            &json!({"accountId": "a1"}),
        ))
        .unwrap_err();
        assert!(server
            .join()
            .unwrap()
            .starts_with("PUT /rest/api/3/issue/WMS-1/assignee "));
        assert!(
            e.starts_with("Brak uprawnień do zmiany tego zgłoszenia w Jirze."),
            "{e}"
        );
        assert!(e.contains("No permission"));
        // 400 przy tworzeniu: komunikat z polami z Jiry.
        let (site, server) = serve_once(
            "400 Bad Request",
            r#"{"errors":{"customfield_1":"Środowisko jest wymagane."}}"#.into(),
        );
        let e = tauri::async_runtime::block_on(jira::send_write(
            &jira::client().unwrap(),
            &config(site),
            "t",
            Method::POST,
            "/rest/api/3/issue",
            &json!({"fields": {}}),
        ))
        .unwrap_err();
        server.join().unwrap();
        assert!(e.starts_with("Jira odrzuciła zmianę.") && e.contains("Środowisko jest wymagane."));
        // Wyłączony przełącznik zapisów: odrzucone przed siecią.
        let read_only = crate::settings::AppSettings {
            jira_write: false,
            ..Default::default()
        };
        assert_eq!(
            jira::ensure_write(&read_only).unwrap_err(),
            jira::WRITE_DISABLED
        );
    }

    #[test]
    fn board_layout_is_fetched_from_configuration_and_project_statuses() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let mut paths = Vec::new();
            for _ in 0..2 {
                let (mut socket, _) = listener.accept().unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 2048];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = socket.read(&mut buffer).unwrap();
                    assert!(n > 0);
                    request.extend_from_slice(&buffer[..n]);
                }
                let line = String::from_utf8(request)
                    .unwrap()
                    .lines()
                    .next()
                    .unwrap()
                    .to_string();
                let body = if line.contains("/configuration") {
                    json!({"location": {"key": "WMS"}, "columnConfig": {"columns": [
                        {"name": "Do zrobienia", "statuses": [{"id": "1"}]},
                        {"name": "Gotowe", "statuses": [{"id": "5"}]}
                    ]}})
                } else {
                    json!([{"name": "Zadanie", "statuses": [
                        {"id": "1", "name": "Do zrobienia", "statusCategory": {"key": "new"}},
                        {"id": "5", "name": "Gotowe", "statusCategory": {"key": "done"}}
                    ]}])
                }
                .to_string();
                write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
                paths.push(line);
            }
            paths
        });
        let layout = tauri::async_runtime::block_on(fetch_layout(
            &jira::client().unwrap(),
            &config(format!("http://{address}")),
            "t",
            9,
            42,
        ))
        .unwrap();
        let paths = server.join().unwrap();
        assert!(paths[0].starts_with("GET /rest/agile/1.0/board/9/configuration "));
        assert!(paths[1].starts_with("GET /rest/api/3/project/WMS/statuses "));
        assert_eq!(layout.columns.len(), 2);
        assert_eq!(layout.statuses.len(), 2);
        assert_eq!(layout.fetched_at, 42);
    }
}
