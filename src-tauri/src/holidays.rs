//! Polskie święta ustawowe (dni wolne od pracy według ustawy z 18 stycznia 1951 r.):
//! stałe oraz ruchome liczone od Wielkanocy. Odpowiednik w TypeScript: `src/holidays.ts`.
//! Cel dnia nie liczy święta jako dnia roboczego, gdy włączone jest ustawienie
//! „Uwzględniaj święta w celu” (`holidays_in_target`).
use crate::settings::AppSettings;
use chrono::{Datelike, Duration, NaiveDate};

/// Niedziela Wielkanocna w kalendarzu gregoriańskim (algorytm Meeusa/Jonesa/Butchera).
pub fn easter(year: i32) -> NaiveDate {
    let a = year % 19;
    let b = year / 100;
    let c = year % 100;
    let d = b / 4;
    let e = b % 4;
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    let month = (h + l - 7 * m + 114) / 31;
    let day = (h + l - 7 * m + 114) % 31 + 1;
    NaiveDate::from_ymd_opt(year, month as u32, day as u32).expect("data Wielkanocy")
}

/// Święta ustawowe danego roku, rosnąco po dacie.
pub fn holidays(year: i32) -> Vec<(NaiveDate, &'static str)> {
    let fixed = |m: u32, d: u32| NaiveDate::from_ymd_opt(year, m, d).expect("stała data");
    let e = easter(year);
    let mut out = vec![
        (fixed(1, 1), "Nowy Rok"),
        (fixed(1, 6), "Trzech Króli"),
        (e, "Wielkanoc"),
        (e + Duration::days(1), "Poniedziałek Wielkanocny"),
        (fixed(5, 1), "Święto Pracy"),
        (fixed(5, 3), "Święto Konstytucji 3 Maja"),
        (e + Duration::days(49), "Zielone Świątki"),
        (e + Duration::days(60), "Boże Ciało"),
        (fixed(8, 15), "Wniebowzięcie NMP"),
        (fixed(11, 1), "Wszystkich Świętych"),
        (fixed(11, 11), "Święto Niepodległości"),
        (fixed(12, 25), "Boże Narodzenie"),
        (fixed(12, 26), "Drugi dzień Bożego Narodzenia"),
    ];
    // Wigilia jest dniem wolnym od 2025 r.
    if year >= 2025 {
        out.push((fixed(12, 24), "Wigilia"));
    }
    out.sort_by_key(|(date, _)| *date);
    out
}

/// Nazwa święta przypadającego w danym dniu.
pub fn holiday(date: NaiveDate) -> Option<&'static str> {
    holidays(date.year())
        .into_iter()
        .find(|(d, _)| *d == date)
        .map(|(_, name)| name)
}

/// Czy dzień jest roboczy: dzień tygodnia z ustawień i, gdy święta są uwzględniane, nie święto.
pub fn is_work_day(s: &AppSettings, date: NaiveDate) -> bool {
    let weekday = date.weekday().number_from_monday() as u8;
    s.work_days.contains(&weekday) && !(s.holidays_in_target && holiday(date).is_some())
}

/// Cel dnia w minutach; 0 dla dnia wolnego i święta.
pub fn day_target_minutes(s: &AppSettings, date: NaiveDate) -> u32 {
    if is_work_day(s, date) {
        s.target_day_minutes
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    /// Ta sama lista, z którą porównuje się `src/holidays.ts` (tests/holidays.spec.ts).
    #[test]
    fn holidays_2024_2030_match_the_reference_list() {
        let expected: Vec<(String, String)> = serde_json::from_str(include_str!(
            "../../tests/fixtures/holidays-pl-2024-2030.json"
        ))
        .unwrap();
        let actual: Vec<(String, String)> = (2024..=2030)
            .flat_map(holidays)
            .map(|(d, name)| (d.format("%Y-%m-%d").to_string(), name.to_string()))
            .collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn easter_for_known_years() {
        for (year, day) in [
            (1818, "1818-03-22"),
            (1943, "1943-04-25"),
            (2019, "2019-04-21"),
            (2038, "2038-04-25"),
            (2285, "2285-03-22"),
        ] {
            assert_eq!(easter(year), date(day), "{year}");
        }
    }

    #[test]
    fn christmas_eve_is_a_holiday_from_2025() {
        assert_eq!(holiday(date("2024-12-24")), None);
        assert_eq!(holiday(date("2025-12-24")), Some("Wigilia"));
        assert_eq!(holiday(date("2026-06-04")), Some("Boże Ciało"));
        assert_eq!(holiday(date("2026-06-05")), None);
    }

    #[test]
    fn holidays_are_not_work_days_unless_disabled() {
        let mut s = AppSettings::default();
        // Środa 11.11.2026 (święto) i czwartek 12.11.2026.
        assert!(!is_work_day(&s, date("2026-11-11")));
        assert_eq!(day_target_minutes(&s, date("2026-11-11")), 0);
        assert_eq!(day_target_minutes(&s, date("2026-11-12")), 480);
        // Święto w niedzielę i tak nie jest dniem roboczym.
        assert!(!is_work_day(&s, date("2026-11-01")));
        s.holidays_in_target = false;
        assert!(is_work_day(&s, date("2026-11-11")));
        assert_eq!(day_target_minutes(&s, date("2026-11-11")), 480);
        // Dzień spoza dni roboczych pozostaje wolny.
        s.work_days = vec![1, 2];
        assert!(!is_work_day(&s, date("2026-11-11")));
    }
}
