//! Zadania cykliczne: wzorzec (zadanie z regułą) tworzy osobne zadanie na każdą
//! pasującą datę, z uzupełnianiem zaległych do 366 dni i wstrzymaniem przez
//! archiwizację. Reguły: dni tygodnia (`repeat_days`, jak przed 1.7) i
//! `repeat_rule` (co N tygodni, dzień miesiąca, ostatni dzień roboczy miesiąca,
//! n-ty dzień tygodnia w miesiącu). Odpowiednik w podglądzie: `src/recurrence.ts`.
use crate::{
    db,
    planner::{
        is_recurring, Details, RepeatRule, RULE_LAST_WORKDAY, RULE_MONTH_DAY, RULE_NTH_WEEKDAY,
        RULE_WEEKS,
    },
    settings::AppSettings,
};
use chrono::{Datelike, Duration, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};

/// Ostatni dzień miesiąca.
pub fn month_end(year: i32, month: u32) -> NaiveDate {
    let (y, m) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    NaiveDate::from_ymd_opt(y, m, 1)
        .and_then(|d| d.pred_opt())
        .unwrap_or(NaiveDate::MAX)
}

/// Ostatni dzień roboczy miesiąca (dni robocze i święta z ustawień); `None`, gdy
/// w miesiącu nie ma dnia roboczego (np. puste dni robocze).
pub fn last_work_day(s: &AppSettings, year: i32, month: u32) -> Option<NaiveDate> {
    let mut date = month_end(year, month);
    while date.month() == month {
        if crate::holidays::is_work_day(s, date) {
            return Some(date);
        }
        date = date.pred_opt()?;
    }
    None
}

fn monday(date: NaiveDate) -> NaiveDate {
    date - Duration::days(date.weekday().num_days_from_monday() as i64)
}

/// Czy reguła wzorca wypada w dniu `date` (od początku cyklu `from`).
pub fn occurs(details: &Details, from: NaiveDate, date: NaiveDate, s: &AppSettings) -> bool {
    let weekday = date.weekday().number_from_monday();
    let rule: &RepeatRule = &details.repeat_rule;
    match rule.kind.as_str() {
        RULE_WEEKS => {
            let weeks = (monday(date) - monday(from)).num_days().div_euclid(7);
            details.repeat_days.contains(&weekday)
                && weeks.rem_euclid(rule.interval.max(1) as i64) == 0
        }
        RULE_MONTH_DAY => {
            let last = month_end(date.year(), date.month()).day();
            rule.day > 0 && date.day() == rule.day.min(last)
        }
        RULE_LAST_WORKDAY => last_work_day(s, date.year(), date.month()) == Some(date),
        RULE_NTH_WEEKDAY => {
            let last = month_end(date.year(), date.month()).day();
            weekday == rule.weekday
                && match rule.nth {
                    -1 => date.day() + 7 > last,
                    n @ 1..=4 => (date.day() - 1) / 7 + 1 == n as u32,
                    _ => false,
                }
        }
        _ => details.repeat_days.contains(&weekday),
    }
}

pub fn generate(db: &mut Connection, today: NaiveDate) -> Result<(), String> {
    let settings = crate::settings::load(db);
    let sources=db.prepare("SELECT t.id,t.title,d.json FROM tasks t JOIN task_details d ON d.task_id=t.id LEFT JOIN task_lifecycle l ON l.task_id=t.id WHERE l.deleted_at IS NULL AND COALESCE(l.archived,0)=0").map_err(|e|e.to_string())?.query_map([],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
    let tx = db.transaction().map_err(|e| e.to_string())?;
    for (id, title, json) in sources {
        let details: Details = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        if !is_recurring(&details) {
            continue;
        }
        let from = NaiveDate::parse_from_str(&details.repeat_from, "%Y-%m-%d")
            .map_err(|_| "Niepoprawny początek cyklu")?;
        let last: Option<String> = tx
            .query_row(
                "SELECT last_day FROM recurrence_state WHERE task_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let mut date = last
            .and_then(|v| NaiveDate::parse_from_str(&v, "%Y-%m-%d").ok())
            .and_then(|d| d.succ_opt())
            .unwrap_or(from)
            .max(from)
            .max(today - Duration::days(365));
        while date <= today {
            let key = date.to_string();
            let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM recurrence_occurrences WHERE source_id=?1 AND day=?2)",params![id,key],|r|r.get(0)).map_err(|e|e.to_string())?;
            if occurs(&details, from, date, &settings) && !exists {
                let instance = if details.planned_day == key {
                    id
                } else {
                    let mut copy = details.clone();
                    copy.repeat_days.clear();
                    copy.repeat_from.clear();
                    copy.repeat_rule = RepeatRule::default();
                    copy.notes.clear();
                    copy.waiting.clear();
                    copy.waiting_since = 0;
                    copy.waiting_status.clear();
                    copy.waiting_dismissed.clear();
                    copy.pinned_at = 0;
                    copy.waiting_remind_day.clear();
                    copy.planned_day = key.clone();
                    copy.due_day.clear();
                    for item in &mut copy.checklist {
                        item.done = false;
                    }
                    tx.execute(
                        "INSERT INTO tasks(title,created_at) VALUES(?1,?2)",
                        params![title, db::now()],
                    )
                    .map_err(|e| e.to_string())?;
                    let new_id = tx.last_insert_rowid();
                    tx.execute(
                        "INSERT INTO task_details(task_id,json) VALUES(?1,?2)",
                        params![
                            new_id,
                            serde_json::to_string(&copy).map_err(|e| e.to_string())?
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                    new_id
                };
                tx.execute(
                    "INSERT INTO recurrence_occurrences(source_id,day,task_id) VALUES(?1,?2,?3)",
                    params![id, key, instance],
                )
                .map_err(|e| e.to_string())?;
            }
            date = date.succ_opt().ok_or("Data poza zakresem")?;
        }
        if from <= today {
            tx.execute("INSERT INTO recurrence_state(task_id,last_day) VALUES(?1,?2) ON CONFLICT(task_id) DO UPDATE SET last_day=excluded.last_day",params![id,today.to_string()]).map_err(|e|e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeats_once_per_selected_day_and_preserves_history() {
        let mut conn = Connection::open_in_memory().unwrap();
        db::init(&conn).unwrap();
        db::change(&mut conn, "add", None, Some("Daily".into()), 0).unwrap();
        crate::planner::save(
            &mut conn,
            1,
            Details {
                repeat_days: vec![1, 3, 5],
                repeat_from: "2026-10-12".into(),
                planned_day: "2026-10-12".into(),
                ..Details::default()
            },
        )
        .unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 10, 16).unwrap();
        generate(&mut conn, today).unwrap();
        generate(&mut conn, today).unwrap();
        let s = db::snapshot(&conn).unwrap();
        assert_eq!(s.tasks.len(), 3);
        assert_eq!(s.tasks[0].details.planned_day, "2026-10-16");
        assert!(s.tasks[0].details.repeat_days.is_empty());
        db::change(&mut conn, "archive", Some(1), None, 0).unwrap();
        generate(&mut conn, today + Duration::days(7)).unwrap();
        assert_eq!(db::snapshot(&conn).unwrap().tasks.len(), 3);
    }

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }
    fn rule(kind: &str) -> RepeatRule {
        RepeatRule {
            kind: kind.into(),
            ..RepeatRule::default()
        }
    }
    /// Dni wystąpień reguły od `from` do `to` włącznie.
    fn days(details: &Details, from: &str, to: &str, s: &AppSettings) -> Vec<String> {
        let (from, to) = (d(from), d(to));
        let mut out = Vec::new();
        let mut date = from;
        while date <= to {
            if occurs(details, from, date, s) {
                out.push(date.to_string());
            }
            date = date.succ_opt().unwrap();
        }
        out
    }

    #[test]
    fn month_day_uses_last_day_in_shorter_months_across_years() {
        let s = AppSettings::default();
        let day31 = Details {
            repeat_rule: RepeatRule {
                day: 31,
                ..rule(RULE_MONTH_DAY)
            },
            ..Details::default()
        };
        assert_eq!(
            days(&day31, "2025-11-15", "2026-04-30", &s),
            [
                "2025-11-30",
                "2025-12-31",
                "2026-01-31",
                "2026-02-28",
                "2026-03-31",
                "2026-04-30"
            ]
        );
        // Rok przestępny: 29 lutego; dzień 30 w lutym też przechodzi na ostatni.
        let day30 = Details {
            repeat_rule: RepeatRule {
                day: 30,
                ..rule(RULE_MONTH_DAY)
            },
            ..Details::default()
        };
        assert_eq!(
            days(&day30, "2028-01-01", "2028-03-31", &s),
            ["2028-01-30", "2028-02-29", "2028-03-30"]
        );
        let day1 = Details {
            repeat_rule: RepeatRule {
                day: 1,
                ..rule(RULE_MONTH_DAY)
            },
            ..Details::default()
        };
        assert_eq!(
            days(&day1, "2026-12-01", "2027-02-15", &s),
            ["2026-12-01", "2027-01-01", "2027-02-01"]
        );
    }

    #[test]
    fn last_work_day_skips_weekends_and_holidays_from_settings() {
        let s = AppSettings::default();
        let last = Details {
            repeat_rule: rule(RULE_LAST_WORKDAY),
            ..Details::default()
        };
        // Sobota 31.10.2026 → piątek 30.10; czwartek 31.12.2026 (przełom roku);
        // niedziela 31.01.2027 → piątek 29.01; luty 2027 kończy się w niedzielę.
        assert_eq!(
            days(&last, "2026-10-01", "2027-02-28", &s),
            [
                "2026-10-30",
                "2026-11-30",
                "2026-12-31",
                "2027-01-29",
                "2027-02-26"
            ]
        );
        // Boże Ciało w czwartek 31.05.2029: ostatni dzień roboczy to środa 30.05.
        assert_eq!(last_work_day(&s, 2029, 5), Some(d("2029-05-30")));
        assert_eq!(days(&last, "2029-05-01", "2029-05-31", &s), ["2029-05-30"]);
        // Bez świąt w ustawieniach: czwartek 31.05.
        let no_holidays = AppSettings {
            holidays_in_target: false,
            ..AppSettings::default()
        };
        assert_eq!(last_work_day(&no_holidays, 2029, 5), Some(d("2029-05-31")));
        // Sobota jako dzień roboczy: 31.10.2026.
        let saturday = AppSettings {
            work_days: vec![1, 2, 3, 4, 5, 6],
            ..AppSettings::default()
        };
        assert_eq!(last_work_day(&saturday, 2026, 10), Some(d("2026-10-31")));
        // Brak dni roboczych: reguła nie wypada wcale.
        let none = AppSettings {
            work_days: Vec::new(),
            ..AppSettings::default()
        };
        assert!(days(&last, "2026-10-01", "2026-12-31", &none).is_empty());
    }

    #[test]
    fn nth_weekday_in_month_including_last_across_years() {
        let s = AppSettings::default();
        let first_monday = Details {
            repeat_rule: RepeatRule {
                nth: 1,
                weekday: 1,
                ..rule(RULE_NTH_WEEKDAY)
            },
            ..Details::default()
        };
        assert_eq!(
            days(&first_monday, "2026-11-15", "2027-02-28", &s),
            ["2026-12-07", "2027-01-04", "2027-02-01"]
        );
        let last_friday = Details {
            repeat_rule: RepeatRule {
                nth: -1,
                weekday: 5,
                ..rule(RULE_NTH_WEEKDAY)
            },
            ..Details::default()
        };
        // 25.12.2026 to święto, ale reguła dnia tygodnia go nie omija.
        assert_eq!(
            days(&last_friday, "2026-12-01", "2027-02-28", &s),
            ["2026-12-25", "2027-01-29", "2027-02-26"]
        );
        let fourth_thursday = Details {
            repeat_rule: RepeatRule {
                nth: 4,
                weekday: 4,
                ..rule(RULE_NTH_WEEKDAY)
            },
            ..Details::default()
        };
        assert_eq!(
            days(&fourth_thursday, "2026-11-01", "2026-12-31", &s),
            ["2026-11-26", "2026-12-24"]
        );
    }

    #[test]
    fn every_n_weeks_counts_from_the_week_of_repeat_from() {
        let s = AppSettings::default();
        // Co 2 tygodnie w poniedziałki i środy od środy 23.12.2026: tydzień
        // 21.12 jest pierwszym (środa 23.12), potem 4.01 i 6.01.2027, 18.01.
        let every2 = Details {
            repeat_days: vec![1, 3],
            repeat_rule: RepeatRule {
                interval: 2,
                ..rule(RULE_WEEKS)
            },
            ..Details::default()
        };
        assert_eq!(
            days(&every2, "2026-12-23", "2027-01-20", &s),
            [
                "2026-12-23",
                "2027-01-04",
                "2027-01-06",
                "2027-01-18",
                "2027-01-20"
            ]
        );
        // Odstęp 1 to zwykły cykl tygodniowy.
        let every1 = Details {
            repeat_rule: RepeatRule {
                interval: 1,
                ..rule(RULE_WEEKS)
            },
            ..every2.clone()
        };
        let weekly = Details {
            repeat_days: vec![1, 3],
            ..Details::default()
        };
        assert_eq!(
            days(&every1, "2026-12-23", "2027-01-20", &s),
            days(&weekly, "2026-12-23", "2027-01-20", &s)
        );
    }

    #[test]
    fn old_repeat_days_without_rule_behave_as_before() {
        // Zapis sprzed 1.7: bez `repeatRule` i bez pól blokady.
        let old: Details = serde_json::from_str(
            r#"{"priority":0,"plannedDay":"2026-10-12","dueDay":"","project":"","waiting":"","notes":{},"estimateMinutes":0,"checklist":[],"repeatDays":[1,3,5],"repeatFrom":"2026-10-12"}"#,
        )
        .unwrap();
        assert_eq!(old.repeat_rule, RepeatRule::default());
        assert!(is_recurring(&old));
        let s = AppSettings::default();
        assert_eq!(
            days(&old, "2026-12-28", "2027-01-08", &s),
            [
                "2026-12-28",
                "2026-12-30",
                "2027-01-01",
                "2027-01-04",
                "2027-01-06",
                "2027-01-08"
            ]
        );
        // Bez dni i bez reguły nie ma cyklu.
        assert!(!is_recurring(&Details::default()));
    }

    #[test]
    fn monthly_rule_generates_dated_copies_and_validates() {
        let mut conn = Connection::open_in_memory().unwrap();
        db::init(&conn).unwrap();
        db::change(&mut conn, "add", None, Some("Faktury".into()), 0).unwrap();
        // Reguła miesięczna czyści dni tygodnia (starsza wersja nie uzna jej za
        // cykl tygodniowy), a bez początku i planu dostaje dzień zapisu.
        let snap = crate::planner::save(
            &mut conn,
            1,
            Details {
                repeat_days: vec![2],
                repeat_from: "2026-01-01".into(),
                repeat_rule: RepeatRule {
                    day: 31,
                    interval: 5,
                    ..rule(RULE_MONTH_DAY)
                },
                checklist: vec![crate::planner::CheckItem {
                    text: "Wyślij".into(),
                    done: true,
                }],
                ..Details::default()
            },
        )
        .unwrap();
        let source = &snap.tasks[0].details;
        assert!(source.repeat_days.is_empty());
        assert_eq!(source.repeat_rule.interval, 0);
        assert_eq!(source.planned_day, "2026-01-01");
        generate(&mut conn, d("2026-03-02")).unwrap();
        generate(&mut conn, d("2026-03-02")).unwrap();
        let s = db::snapshot(&conn).unwrap();
        let mut planned: Vec<_> = s
            .tasks
            .iter()
            .map(|t| t.details.planned_day.as_str())
            .collect();
        planned.sort();
        assert_eq!(planned, ["2026-01-01", "2026-01-31", "2026-02-28"]);
        let copy = s
            .tasks
            .iter()
            .find(|t| t.details.planned_day == "2026-02-28")
            .unwrap();
        assert_eq!(copy.details.repeat_rule, RepeatRule::default());
        assert!(!is_recurring(&copy.details));
        assert!(!copy.details.checklist[0].done);
        // Niepoprawne reguły są odrzucane.
        for bad in [
            RepeatRule {
                day: 32,
                ..rule(RULE_MONTH_DAY)
            },
            RepeatRule {
                interval: 9,
                ..rule(RULE_WEEKS)
            },
            RepeatRule {
                nth: 5,
                weekday: 1,
                ..rule(RULE_NTH_WEEKDAY)
            },
            RepeatRule {
                nth: 1,
                weekday: 8,
                ..rule(RULE_NTH_WEEKDAY)
            },
            rule("yearly"),
        ] {
            let details = Details {
                repeat_rule: bad.clone(),
                ..Details::default()
            };
            assert!(
                crate::planner::save(&mut conn, 1, details).is_err(),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn last_work_day_rule_follows_holiday_settings_when_generating() {
        let mut conn = Connection::open_in_memory().unwrap();
        db::init(&conn).unwrap();
        db::change(&mut conn, "add", None, Some("Raport".into()), 0).unwrap();
        crate::planner::save(
            &mut conn,
            1,
            Details {
                repeat_from: "2029-05-01".into(),
                planned_day: "2029-05-01".into(),
                repeat_rule: rule(RULE_LAST_WORKDAY),
                ..Details::default()
            },
        )
        .unwrap();
        generate(&mut conn, d("2029-05-31")).unwrap();
        let s = db::snapshot(&conn).unwrap();
        assert!(s
            .tasks
            .iter()
            .any(|t| t.details.planned_day == "2029-05-30"));
        assert!(!s
            .tasks
            .iter()
            .any(|t| t.details.planned_day == "2029-05-31"));
    }
}
