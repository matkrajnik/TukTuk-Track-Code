//! Eksport przedziałów czasu do CSV (pod polski Excel) i XLSX.
//!
//! Przedziały dzieli się przez lokalną północ jak w raportach (`stats::day_bounds`),
//! a trwający pomiar liczy się do chwili eksportu z dopiskiem „trwa”.
//! Odpowiednik CSV w trybie podglądu: `src/export.ts`.
use crate::{db, planner, stats, Database};
use chrono::{Local, NaiveDate, NaiveDateTime, TimeZone};
use rust_xlsxwriter::{ExcelDateTime, Format, Workbook, XlsxError};
use std::collections::HashMap;
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

/// Jeden wiersz eksportu: część przedziału przypadająca na jeden dzień.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub day: NaiveDate,
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    /// Koniec części to koniec doby (lokalna północ następnego dnia).
    pub to_midnight: bool,
    pub ms: i64,
    pub running: bool,
    pub task_id: i64,
    pub task: String,
    pub jira_key: String,
    pub project: String,
    pub note: String,
    /// Zgłoszenie Redmine jako `#1234` (kolumna na końcu, PLAN_1.9.md R2);
    /// kolumna „Klucz Jiry” zostaje bez zmian dla zgodności arkuszy.
    pub redmine: String,
}

pub const HEADERS: [&str; 11] = [
    "Data",
    "Start",
    "Koniec",
    "Minuty",
    "Godziny",
    "Zadanie",
    "Klucz Jiry",
    "Projekt",
    "Notatka dnia",
    "Uwagi",
    "Redmine",
];
const RUNNING: &str = "trwa";
/// Najwięcej dni w jednym eksporcie (z zapasem ponad rok).
const MAX_DAYS: usize = 400;

fn local<Tz: TimeZone>(tz: &Tz, ms: i64) -> NaiveDateTime {
    tz.timestamp_millis_opt(ms)
        .earliest()
        .map(|t| t.naive_local())
        .unwrap_or_default()
}

/// Wiersze eksportu dla wskazanych dni (bez zadań w koszu), posortowane
/// według dnia i początku. `now` zamyka trwający pomiar.
pub fn rows<Tz: TimeZone>(snap: &db::Snapshot, tz: &Tz, days: &[NaiveDate], now: i64) -> Vec<Row> {
    let tasks: HashMap<i64, &db::Task> = snap.tasks.iter().map(|t| (t.id, t)).collect();
    let mut days = days.to_vec();
    days.sort();
    days.dedup();
    let mut out = Vec::new();
    for day in days {
        let (from, to) = stats::day_bounds(tz, day);
        let key = day.format("%Y-%m-%d").to_string();
        for s in &snap.sessions {
            let Some(task) = tasks.get(&s.task_id) else {
                continue;
            };
            let until = s.ended_at.unwrap_or(now);
            let start = s.started_at.max(from);
            let end = until.min(to);
            if end <= start {
                continue;
            }
            out.push(Row {
                day,
                start: local(tz, start),
                end: local(tz, end),
                to_midnight: end == to,
                ms: end - start,
                running: s.ended_at.is_none() && end == until,
                task_id: task.id,
                task: task.title.clone(),
                jira_key: task.jira_key.clone().unwrap_or_default(),
                project: task.details.project.trim().to_string(),
                note: task
                    .details
                    .notes
                    .get(&key)
                    .map(|n| n.trim().to_string())
                    .unwrap_or_default(),
                redmine: task
                    .redmine_id
                    .map(|id| format!("#{id}"))
                    .unwrap_or_default(),
            });
        }
    }
    out.sort_by(|a, b| (a.day, a.start, a.task_id).cmp(&(b.day, b.start, b.task_id)));
    out
}

/// Pole tekstowe CSV: cudzysłów przy średniku, cudzysłowie lub nowej linii;
/// apostrof przed `=`, `+`, `-`, `@`, żeby Excel nie czytał tekstu jako formuły.
fn text(value: &str) -> String {
    let value = if value.starts_with(['=', '+', '-', '@']) {
        format!("'{value}")
    } else {
        value.to_string()
    };
    if value.contains([';', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value
    }
}

pub fn minutes(ms: i64) -> i64 {
    (ms + 30_000) / 60_000
}
/// Godziny dziesiętne, dwa miejsca, przecinek; zaokrąglenie połówek w górę
/// na liczbach całkowitych, żeby podgląd (`src/export.ts`) dawał ten sam tekst.
pub fn hours(ms: i64) -> String {
    let hundredths = (ms + 18_000) / 36_000;
    format!("{},{:02}", hundredths / 100, hundredths % 100)
}
fn end_text(row: &Row) -> String {
    if row.to_midnight {
        "24:00".into()
    } else {
        row.end.format("%H:%M").to_string()
    }
}

/// CSV pod polski Excel: UTF-8 z BOM, średnik, przecinek dziesiętny, CRLF.
pub fn csv(rows: &[Row]) -> String {
    let mut out = String::from("\u{feff}");
    out.push_str(&HEADERS.join(";"));
    out.push_str("\r\n");
    for row in rows {
        let fields = [
            row.day.format("%Y-%m-%d").to_string(),
            row.start.format("%H:%M").to_string(),
            end_text(row),
            minutes(row.ms).to_string(),
            hours(row.ms),
            text(&row.task),
            text(&row.jira_key),
            text(&row.project),
            text(&row.note),
            if row.running {
                RUNNING.into()
            } else {
                String::new()
            },
            text(&row.redmine),
        ];
        out.push_str(&fields.join(";"));
        out.push_str("\r\n");
    }
    out
}

fn excel_time(t: NaiveDateTime) -> Result<ExcelDateTime, XlsxError> {
    use chrono::{Datelike, Timelike};
    ExcelDateTime::from_ymd(t.year() as u16, t.month() as u8, t.day() as u8)?.and_hms(
        t.hour() as u16,
        t.minute() as u8,
        t.second(),
    )
}

/// Godzina jako ułamek doby (format czasu Excela bez daty).
fn day_fraction(t: NaiveDateTime) -> f64 {
    use chrono::Timelike;
    t.time().num_seconds_from_midnight() as f64 / 86_400.0
}

/// Skoroszyt z arkuszami „Przedziały” i „Podsumowanie” (dni × zadania, sumy w godzinach).
pub fn xlsx(rows: &[Row], days: &[NaiveDate]) -> Result<Vec<u8>, XlsxError> {
    let mut book = Workbook::new();
    let bold = Format::new().set_bold();
    let date = Format::new().set_num_format("yyyy-mm-dd");
    // Czas doby jako ułamek dnia; „[hh]” pokazuje koniec doby jako 24:00, jak w CSV.
    let time = Format::new().set_num_format("[hh]:mm");
    let whole = Format::new().set_num_format("0");
    let hundredths = Format::new().set_num_format("0.00");
    let hundredths_bold = Format::new().set_num_format("0.00").set_bold();

    let sheet = book.add_worksheet();
    sheet.set_name("Przedziały")?;
    for (col, name) in HEADERS.iter().enumerate() {
        sheet.write_string_with_format(0, col as u16, *name, &bold)?;
    }
    for (i, row) in rows.iter().enumerate() {
        let r = i as u32 + 1;
        sheet.write_datetime_with_format(
            r,
            0,
            excel_time(row.day.and_time(Default::default()))?,
            &date,
        )?;
        sheet.write_number_with_format(r, 1, day_fraction(row.start), &time)?;
        let end = if row.to_midnight {
            1.0
        } else {
            day_fraction(row.end)
        };
        sheet.write_number_with_format(r, 2, end, &time)?;
        sheet.write_number_with_format(r, 3, row.ms as f64 / 60_000.0, &whole)?;
        sheet.write_number_with_format(r, 4, row.ms as f64 / 3_600_000.0, &hundredths)?;
        sheet.write_string(r, 5, &row.task)?;
        sheet.write_string(r, 6, &row.jira_key)?;
        sheet.write_string(r, 7, &row.project)?;
        sheet.write_string(r, 8, &row.note)?;
        if row.running {
            sheet.write_string(r, 9, RUNNING)?;
        }
        sheet.write_string(r, 10, &row.redmine)?;
    }
    for (col, width) in [11.0, 7.0, 7.0, 8.0, 8.0, 40.0, 12.0, 16.0, 40.0, 8.0, 10.0]
        .into_iter()
        .enumerate()
    {
        sheet.set_column_width(col as u16, width)?;
    }
    sheet.set_freeze_panes(1, 0)?;

    // Zadania według łącznego czasu malejąco, potem id; w nagłówku klucz
    // zgłoszenia (Jira albo `#numer` Redmine) w nawiasie.
    let mut totals: Vec<(i64, String, i64)> = Vec::new();
    for row in rows {
        match totals.iter_mut().find(|t| t.0 == row.task_id) {
            Some(t) => t.2 += row.ms,
            None => {
                let issue = if row.jira_key.is_empty() {
                    &row.redmine
                } else {
                    &row.jira_key
                };
                let label = if issue.is_empty() {
                    row.task.clone()
                } else {
                    format!("{} ({})", row.task, issue)
                };
                totals.push((row.task_id, label, row.ms));
            }
        }
    }
    totals.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
    let mut days = days.to_vec();
    days.sort();
    days.dedup();
    let mut cells: HashMap<(NaiveDate, i64), i64> = HashMap::new();
    for row in rows {
        *cells.entry((row.day, row.task_id)).or_default() += row.ms;
    }
    let to_h = |ms: i64| ms as f64 / 3_600_000.0;
    let sum_col = totals.len() as u16 + 1;
    let summary = book.add_worksheet();
    summary.set_name("Podsumowanie")?;
    summary.write_string_with_format(0, 0, "Data", &bold)?;
    for (i, (_, label, _)) in totals.iter().enumerate() {
        summary.write_string_with_format(0, i as u16 + 1, label, &bold)?;
        summary.set_column_width(i as u16 + 1, 14.0)?;
    }
    summary.write_string_with_format(0, sum_col, "Suma", &bold)?;
    summary.set_column_width(0, 11.0)?;
    for (i, day) in days.iter().enumerate() {
        let r = i as u32 + 1;
        summary.write_datetime_with_format(
            r,
            0,
            excel_time(day.and_time(Default::default()))?,
            &date,
        )?;
        let mut day_sum = 0;
        for (c, (id, _, _)) in totals.iter().enumerate() {
            if let Some(ms) = cells.get(&(*day, *id)) {
                summary.write_number_with_format(r, c as u16 + 1, to_h(*ms), &hundredths)?;
                day_sum += ms;
            }
        }
        summary.write_number_with_format(r, sum_col, to_h(day_sum), &hundredths_bold)?;
    }
    let last = days.len() as u32 + 1;
    summary.write_string_with_format(last, 0, "Suma", &bold)?;
    for (c, (_, _, ms)) in totals.iter().enumerate() {
        summary.write_number_with_format(last, c as u16 + 1, to_h(*ms), &hundredths_bold)?;
    }
    let all: i64 = rows.iter().map(|r| r.ms).sum();
    summary.write_number_with_format(last, sum_col, to_h(all), &hundredths_bold)?;
    summary.set_freeze_panes(1, 0)?;
    book.save_to_buffer()
}

/// Nazwa pliku bez znaków niedozwolonych w Windows.
fn file_stem(name: &str) -> String {
    let stem: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | ' ' | '.'))
        .take(80)
        .collect();
    let stem = stem.trim_matches(|c: char| c == '.' || c == ' ');
    if stem.is_empty() {
        "Todo-Track".into()
    } else {
        stem.into()
    }
}

fn parse_days(days: &[String]) -> Result<Vec<NaiveDate>, String> {
    if days.is_empty() {
        return Err("Wybierz co najmniej jeden dzień do eksportu.".into());
    }
    if days.len() > MAX_DAYS {
        return Err(format!("Eksport obejmuje najwyżej {MAX_DAYS} dni."));
    }
    days.iter()
        .map(|d| {
            if !planner::date_valid(d) {
                return Err(format!("Niepoprawna data: {d}"));
            }
            NaiveDate::parse_from_str(d, "%Y-%m-%d").map_err(|_| format!("Niepoprawna data: {d}"))
        })
        .collect()
}

/// Zapisuje eksport wybranych dni do pliku wskazanego w oknie zapisu.
/// Zwraca ścieżkę albo `None`, gdy użytkownik anulował.
#[tauri::command]
pub async fn export_report(
    app: tauri::AppHandle,
    format: String,
    days: Vec<String>,
    name: String,
) -> Result<Option<String>, String> {
    let (ext, filter) = match format.as_str() {
        "csv" => ("csv", "CSV (Excel, średnik)"),
        "xlsx" => ("xlsx", "Skoroszyt Excel"),
        _ => return Err("Nieznany format eksportu.".into()),
    };
    let dates = parse_days(&days)?;
    tauri::async_runtime::spawn_blocking(move || {
        let Some(file) = app
            .dialog()
            .file()
            .set_file_name(format!("{}.{ext}", file_stem(&name)))
            .add_filter(filter, &[ext])
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = file.into_path().map_err(|e| e.to_string())?;
        // Chwila eksportu: po zamknięciu okna zapisu, trwający pomiar do teraz.
        let (snap, now) = {
            let state = app.state::<Database>();
            let db = state.0.lock().map_err(|e| e.to_string())?;
            (db::snapshot(&db).map_err(|e| e.to_string())?, db::now())
        };
        let list = rows(&snap, &Local, &dates, now);
        let bytes = match ext {
            "csv" => csv(&list).into_bytes(),
            _ => xlsx(&list, &dates).map_err(|e| e.to_string())?,
        };
        std::fs::write(&path, bytes)
            .map_err(|e| format!("Nie można zapisać pliku (może jest otwarty w Excelu?): {e}"))?;
        Ok(Some(path.to_string_lossy().into_owned()))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use calamine::{open_workbook_from_rs, Data, Reader, Xlsx};
    use chrono_tz::Europe::Warsaw;
    use rusqlite::{params, Connection};

    const H: i64 = 3_600_000;

    fn date(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }
    fn at(s: &str) -> i64 {
        Warsaw
            .from_local_datetime(&NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap())
            .earliest()
            .unwrap()
            .timestamp_millis()
    }
    /// Baza z zadaniem z polskimi znakami (WMS-7, notatka 2.11), zadaniem
    /// z średnikiem i cudzysłowem, zadaniem w koszu i trwającym pomiarem.
    fn fixture() -> db::Snapshot {
        let db = Connection::open_in_memory().unwrap();
        db::init(&db).unwrap();
        db.execute_batch(
            "INSERT INTO tasks(id,title,done,created_at) VALUES
               (1,'Zażółć gęślą jaźń',0,0),(2,'Spotkanie; \"pilne\"',0,0),(3,'Usunięte',0,0);
             INSERT INTO jira_tasks(task_id,site,issue_key) VALUES (1,'https://x.atlassian.net','WMS-7');
             INSERT INTO task_details(task_id,json) VALUES
               (1,'{\"project\":\"Magazyn Łódź\",\"notes\":{\"2026-11-02\":\"Poprawki źródeł\"}}');
             INSERT INTO task_lifecycle(task_id,archived,deleted_at) VALUES (3,0,1);",
        )
        .unwrap();
        let add = |task: i64, a: &str, b: Option<&str>| {
            db.execute(
                "INSERT INTO sessions(task_id,started_at,ended_at) VALUES (?1,?2,?3)",
                params![task, at(a), b.map(at)],
            )
            .unwrap();
        };
        // Przez północ 1.11 → 2.11: dwie części.
        add(1, "2026-11-01 23:00", Some("2026-11-02 01:30"));
        add(2, "2026-11-02 09:00", Some("2026-11-02 10:15"));
        add(3, "2026-11-02 11:00", Some("2026-11-02 12:00"));
        // Trwa od 14:00.
        add(1, "2026-11-03 14:00", None);
        db::snapshot(&db).unwrap()
    }
    fn days() -> Vec<NaiveDate> {
        ["2026-11-03", "2026-11-01", "2026-11-02", "2026-11-02"]
            .map(date)
            .to_vec()
    }

    #[test]
    fn rows_split_at_midnight_skip_trash_and_count_running_to_now() {
        let list = rows(&fixture(), &Warsaw, &days(), at("2026-11-03 15:20"));
        let brief: Vec<_> = list
            .iter()
            .map(|r| (r.day.to_string(), r.ms, r.running, r.task_id))
            .collect();
        assert_eq!(
            brief,
            vec![
                ("2026-11-01".into(), H, false, 1),
                ("2026-11-02".into(), H + H / 2, false, 1),
                ("2026-11-02".into(), H + H / 4, false, 2),
                ("2026-11-03".into(), H + H / 3, true, 1),
            ]
        );
        assert!(list[0].to_midnight);
        assert_eq!(list[1].note, "Poprawki źródeł");
        assert_eq!(list[0].note, "");
        // Dzień bez wybranego dnia nie trafia do eksportu.
        let only = rows(
            &fixture(),
            &Warsaw,
            &[date("2026-11-02")],
            at("2026-11-03 15:20"),
        );
        assert_eq!(only.len(), 2);
    }

    #[test]
    fn csv_has_bom_semicolons_decimal_comma_and_polish_text() {
        let list = rows(&fixture(), &Warsaw, &days(), at("2026-11-03 15:20"));
        let out = csv(&list);
        assert!(out.as_bytes().starts_with(&[0xEF, 0xBB, 0xBF]));
        let lines: Vec<_> = out.trim_start_matches('\u{feff}').split("\r\n").collect();
        assert_eq!(
            lines,
            vec![
                "Data;Start;Koniec;Minuty;Godziny;Zadanie;Klucz Jiry;Projekt;Notatka dnia;Uwagi;Redmine",
                "2026-11-01;23:00;24:00;60;1,00;Zażółć gęślą jaźń;WMS-7;Magazyn Łódź;;;",
                "2026-11-02;00:00;01:30;90;1,50;Zażółć gęślą jaźń;WMS-7;Magazyn Łódź;Poprawki źródeł;;",
                "2026-11-02;09:00;10:15;75;1,25;\"Spotkanie; \"\"pilne\"\"\";;;;;",
                "2026-11-03;14:00;15:20;80;1,33;Zażółć gęślą jaźń;WMS-7;Magazyn Łódź;;trwa;",
                "",
            ]
        );
        assert_eq!(text("=SUM(A1)"), "'=SUM(A1)");
        // 7,5 minuty = 0,125 h: połówka w górę, jak w podglądzie.
        assert_eq!(hours(450_000), "0,13");
        assert_eq!(minutes(90_000), 2);
        assert_eq!(file_stem("Todo-Track 2026-11/<x>"), "Todo-Track 2026-11x");
        assert_eq!(file_stem("..."), "Todo-Track");
    }

    #[test]
    fn xlsx_reads_back_with_matching_totals() {
        let list = rows(&fixture(), &Warsaw, &days(), at("2026-11-03 15:20"));
        let bytes = xlsx(&list, &days()).unwrap();
        let mut book: Xlsx<_> = open_workbook_from_rs(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(book.sheet_names(), vec!["Przedziały", "Podsumowanie"]);

        let ranges = book.worksheet_range("Przedziały").unwrap();
        let rows: Vec<_> = ranges.rows().collect();
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0][0], Data::String("Data".into()));
        assert_eq!(rows[1][5], Data::String("Zażółć gęślą jaźń".into()));
        assert_eq!(rows[4][9], Data::String("trwa".into()));
        let num = |d: &Data| match d {
            Data::Float(f) => *f,
            Data::Int(i) => *i as f64,
            other => panic!("oczekiwano liczby, jest {other:?}"),
        };
        let time = |d: &Data| match d {
            Data::Float(f) => *f,
            Data::DateTime(t) => t.as_f64(),
            other => panic!("oczekiwano czasu, jest {other:?}"),
        };
        // Część kończąca się o północy: 23:00–24:00 jak w CSV, nie 00:00.
        assert!((time(&rows[1][1]) - 23.0 / 24.0).abs() < 1e-9);
        assert!((time(&rows[1][2]) - 1.0).abs() < 1e-9);
        assert!(time(&rows[2][1]).abs() < 1e-9);
        assert!((time(&rows[2][2]) - 1.5 / 24.0).abs() < 1e-9);
        let minutes: f64 = rows[1..].iter().map(|r| num(&r[3])).sum();
        let hours: f64 = rows[1..].iter().map(|r| num(&r[4])).sum();
        assert!((minutes - 305.0).abs() < 1e-9);
        assert!((hours - 305.0 / 60.0).abs() < 1e-9);

        let summary = book.worksheet_range("Podsumowanie").unwrap();
        let s: Vec<_> = summary.rows().collect();
        // Nagłówek, trzy dni, wiersz sumy; zadania: WMS-7 (3:50) przed spotkaniem (1:15).
        assert_eq!(s.len(), 5);
        assert_eq!(s[0][1], Data::String("Zażółć gęślą jaźń (WMS-7)".into()));
        assert_eq!(s[0][2], Data::String("Spotkanie; \"pilne\"".into()));
        assert_eq!(s[4][0], Data::String("Suma".into()));
        let total = num(&s[4][3]);
        assert!((total - hours).abs() < 1e-9);
        let day_sums: f64 = s[1..4].iter().map(|r| num(&r[3])).sum();
        assert!((day_sums - total).abs() < 1e-9);
        assert!((num(&s[4][1]) - (230.0 / 60.0)).abs() < 1e-9);
        assert!((num(&s[2][3]) - 2.75).abs() < 1e-9);
    }

    #[test]
    fn parse_days_rejects_bad_input() {
        assert!(parse_days(&[]).is_err());
        assert!(parse_days(&["2026-02-30".into()]).is_err());
        assert_eq!(
            parse_days(&["2026-11-02".into()]).unwrap(),
            vec![date("2026-11-02")]
        );
    }

    #[test]
    fn redmine_issue_fills_last_column_and_summary_label() {
        let db = Connection::open_in_memory().unwrap();
        db::init(&db).unwrap();
        db.execute_batch(
            "INSERT INTO tasks(id,title,done,created_at) VALUES (1,'Import CSV',0,0),(2,'Bez zgłoszenia',0,0);
             INSERT INTO redmine_tasks(task_id,site,issue_id) VALUES (1,'https://redmine.example.pl',1234);",
        )
        .unwrap();
        for (task, a, b) in [
            (1, "2026-11-02 09:00", "2026-11-02 10:00"),
            (2, "2026-11-02 10:00", "2026-11-02 10:30"),
        ] {
            db.execute(
                "INSERT INTO sessions(task_id,started_at,ended_at) VALUES (?1,?2,?3)",
                params![task, at(a), at(b)],
            )
            .unwrap();
        }
        let snap = db::snapshot(&db).unwrap();
        let day = [date("2026-11-02")];
        let list = rows(&snap, &Warsaw, &day, at("2026-11-02 12:00"));
        assert_eq!(list[0].redmine, "#1234");
        assert_eq!(list[0].jira_key, "");
        assert_eq!(list[1].redmine, "");
        let out = csv(&list);
        let lines: Vec<_> = out.trim_start_matches('\u{feff}').split("\r\n").collect();
        assert_eq!(
            lines[1],
            "2026-11-02;09:00;10:00;60;1,00;Import CSV;;;;;#1234"
        );
        assert_eq!(
            lines[2],
            "2026-11-02;10:00;10:30;30;0,50;Bez zgłoszenia;;;;;"
        );
        let bytes = xlsx(&list, &day).unwrap();
        let mut book: Xlsx<_> = open_workbook_from_rs(std::io::Cursor::new(bytes)).unwrap();
        let ranges = book.worksheet_range("Przedziały").unwrap();
        let rows: Vec<_> = ranges.rows().collect();
        assert_eq!(rows[0][10], Data::String("Redmine".into()));
        assert_eq!(rows[1][10], Data::String("#1234".into()));
        let summary = book.worksheet_range("Podsumowanie").unwrap();
        let s: Vec<_> = summary.rows().collect();
        assert_eq!(s[0][1], Data::String("Import CSV (#1234)".into()));
        assert_eq!(s[0][2], Data::String("Bez zgłoszenia".into()));
    }
}
