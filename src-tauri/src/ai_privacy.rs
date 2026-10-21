//! Filtr danych osobowych przed modelem AI (PLAN_BACKLOG.md, A0).
//!
//! Każdy tekst i JSON wysyłany do modelu (wywołania aplikacji, czat, odpowiedzi
//! narzędzi MCP) przechodzi przez [`redact`]: osoby zamieniane są na stałe
//! pseudonimy „Osoba N”, a mapa pseudonim → osoba ([`Pseudonyms`]) żyje tylko w
//! pamięci i służy do [`Pseudonyms::restore`] odpowiedzi modelu. Do modelu mapa
//! nigdy nie trafia.
//!
//! Dwa mechanizmy:
//! - **pola osobowe po kształcie danych** (JSON): przypisany, zgłaszający,
//!   autor, organizator, uczestnicy, pola Jiry typu osoba (`jira_fields.kind`),
//!   `displayName` — wartość jest podmieniana w całości;
//! - **słownik** dla zwykłego tekstu: imiona i nazwiska osób z Jiry/Redmine,
//!   własne imię i nazwisko użytkownika oraz `ai_hidden_names` z Ustawień.
//!   Dopasowanie bez rozróżniania wielkości liter i polskich znaków, po granicy
//!   słowa, z polską odmianą przez rdzeń i zamkniętą listę końcówek; imiona
//!   krótsze niż 3 znaki są pomijane.
//!
//! Ograniczenia (opisane w README i w Ustawieniach): nazwisko spoza słownika
//! wpisane ręcznie w tytuł albo notatkę przejdzie; nazwisko będące zwykłym
//! słowem (np. „Marzec”) jest podmieniane wszędzie; nieregularne odmiany
//! (np. „Marzec” → „Marca”) nie są rozpoznawane.
//!
//! Moduł nie ma interfejsu ani poleceń Tauri; korzystają z niego A1–A5.
#![allow(dead_code)]
use crate::settings::AppSettings;
use rusqlite::Connection;
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Najkrótsza część nazwy (znaki), która trafia do słownika.
const MIN_NAME_CHARS: usize = 3;
/// Zastępnik wartości osobowej bez nazwy (np. samo konto).
const HIDDEN: &str = "[ukryte]";

/// Klucze JSON (po normalizacji: małe litery bez `_`, `-` i spacji), których
/// wartość jest osobą i jest podmieniana w całości.
const PERSON_KEYS: &[&str] = &[
    "assignee",
    "reporter",
    "creator",
    "author",
    "authors",
    "authorname",
    "committer",
    "committername",
    "organizer",
    "attendees",
    "participants",
    "requester",
    "assignedto",
    "createdby",
    "updatedby",
    "lastmodifiedby",
    "displayname",
    "fullname",
    "przypisany",
    "zglaszajacy",
    "autor",
    "organizator",
    "uczestnicy",
];
/// Klucze JSON z identyfikatorem osoby (adres, konto, login): wartość znika.
const HIDDEN_KEYS: &[&str] = &[
    "emailaddress",
    "email",
    "mail",
    "authoremail",
    "accountid",
    "login",
    "username",
];
/// Klucze, pod którymi obiekt osoby trzyma nazwę, w kolejności preferencji.
const NAME_KEYS: &[&str] = &[
    "displayName",
    "display_name",
    "name",
    "fullName",
    "value",
    "emailAddress",
    "email",
    "login",
    "accountId",
];
/// Klucze obiektu pola (`{id, name, value}`), które wskazują identyfikator pola.
const FIELD_ID_KEYS: &[&str] = &["id", "fieldId", "field_id", "field"];

// Końcówki odmiany (zamknięte listy; rdzeń + końcówka musi dać znaną formę).
/// Imiona i nazwiska zakończone spółgłoską: Marcin, Krajnik, Jan.
const NOUN: &[&str] = &[
    "a", "owi", "em", "u", "ie", "iem", "owie", "ów", "om", "ami", "ach",
];
/// Imiona zakończone na -a: Anna, Ewa, Kuba.
const FEM: &[&str] = &["a", "y", "i", "ie", "ę", "ą", "o"];
/// Zdrobnienia po skróceniu podwojonej spółgłoski: Anna → Ania (bez „Ani”,
/// bo to zwykłe słowo).
const DIM: &[&str] = &["ia", "ią", "ii", "ię", "io"];
/// Nazwiska przymiotnikowe: Kowalski, Kowalska, Czarny.
const ADJ: &[&str] = &[
    "i", "y", "a", "ą", "iego", "ego", "iemu", "emu", "im", "ym", "iej", "ej", "ie", "e", "ich",
    "ych", "imi", "ymi",
];
/// Liczba mnoga nazwisk na -ski/-cki/-dzki po odjęciu „k”: Kowalscy.
const ADJ_PLURAL: &[&str] = &["cy", "cich", "cim", "cimi"];

/// Małe litery bez polskich znaków diakrytycznych: tak porównujemy nazwy.
fn fold(s: &str) -> String {
    s.chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'ą' => 'a',
            'ć' => 'c',
            'ę' => 'e',
            'ł' => 'l',
            'ń' => 'n',
            'ó' => 'o',
            'ś' => 's',
            'ź' | 'ż' => 'z',
            other => other,
        })
        .collect()
}

/// Klucz pola JSON: małe litery i cyfry, bez separatorów.
fn norm_key(key: &str) -> String {
    fold(key).chars().filter(|c| c.is_alphanumeric()).collect()
}

/// Wszystkie znane formy wyrazu dla nazwy (już po `fold`).
fn forms_of(name: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![name.to_string()];
    let chars: Vec<char> = name.chars().collect();
    let n = chars.len();
    let without_last: String = chars[..n - 1].iter().collect();
    let adjectival_a = ["ska", "cka", "dzka"].iter().any(|e| name.ends_with(e));
    let add = |out: &mut Vec<String>, stem: &str, suffixes: &[&str]| {
        for s in suffixes {
            out.push(format!("{stem}{}", fold(s)));
        }
    };
    if adjectival_a || name.ends_with('i') || name.ends_with('y') {
        add(&mut out, &without_last, ADJ);
        if without_last.ends_with('k') {
            let plural_stem = &without_last[..without_last.len() - 1];
            add(&mut out, plural_stem, ADJ_PLURAL);
        }
    } else if name.ends_with('a') {
        add(&mut out, &without_last, FEM);
        let mut tail = without_last.chars().rev();
        if let (Some(a), Some(b)) = (tail.next(), tail.next()) {
            if a == b && !"aeiouy".contains(a) {
                let collapsed = &without_last[..without_last.len() - a.len_utf8()];
                add(&mut out, collapsed, DIM);
            }
        }
    } else {
        add(&mut out, name, NOUN);
        // Ruchome „e”: Marek → Marka, Jacek → Jacka, Marzec → Marzca.
        if ["ek", "ec", "el"].iter().any(|e| name.ends_with(e)) && n > 3 {
            let stem: String = chars[..n - 2].iter().chain(&chars[n - 1..]).collect();
            add(&mut out, &stem, NOUN);
        }
    }
    out
}

/// Osoba ze słownika: wyświetlana nazwa i jej części (po `fold`).
#[derive(Clone, Debug)]
struct Person {
    key: String,
    display: String,
    parts: Vec<String>,
}

/// Słownik nazw osób do ukrycia w zwykłym tekście i lista pól Jiry typu osoba.
#[derive(Clone, Debug, Default)]
pub struct Dictionary {
    people: Vec<Person>,
    by_key: HashMap<String, usize>,
    /// Forma wyrazu (po `fold`) → osoby, których część ona jest.
    forms: HashMap<String, Vec<usize>>,
    /// Identyfikatory pól Jiry typu osoba (małe litery).
    person_fields: Vec<String>,
}

/// Słowa nazwy: ciągi liter i cyfr.
fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
}

/// Nazwa osoby bez adresu w nawiasach ostrych: „Jan Kowalski <jan@x.pl>”.
fn identity(raw: &str) -> String {
    let cut = raw.split('<').next().unwrap_or(raw).trim();
    let text = if cut.is_empty() { raw.trim() } else { cut };
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl Dictionary {
    pub fn new() -> Self {
        Self::default()
    }

    /// Liczba osób w słowniku.
    pub fn len(&self) -> usize {
        self.people.len()
    }

    pub fn is_empty(&self) -> bool {
        self.people.is_empty()
    }

    /// Dodaje osobę (albo samo imię, albo nazwisko). Części krótsze niż 3
    /// znaki są pomijane, a nazwa bez żadnej takiej części nie trafia do
    /// słownika. Zwraca `true`, gdy osoba została dodana.
    pub fn add_name(&mut self, name: &str) -> bool {
        let name = identity(name);
        if words(&name).all(|w| w.chars().count() < MIN_NAME_CHARS) {
            return false;
        }
        self.insert(&name);
        true
    }

    /// Dodaje nazwy z listy ustawień (`ai_hidden_names`).
    pub fn add_hidden_names(&mut self, names: &[String]) {
        for name in names {
            self.add_name(name);
        }
    }

    /// Dodaje pole Jiry typu osoba (po identyfikatorze, np. `customfield_10050`).
    pub fn add_person_field(&mut self, id: &str) {
        let id = id.trim().to_lowercase();
        if !id.is_empty() && !self.person_fields.contains(&id) {
            self.person_fields.push(id);
        }
    }

    fn is_person_field(&self, id: &str) -> bool {
        let id = id.trim().to_lowercase();
        !id.is_empty()
            && (self.person_fields.contains(&id) || PERSON_KEYS.contains(&norm_key(&id).as_str()))
    }

    /// Wstawia osobę o danej nazwie (bez powtórek po kluczu) i zwraca indeks.
    fn insert(&mut self, display: &str) -> usize {
        let display = identity(display);
        let key = fold(&display);
        if let Some(&idx) = self.by_key.get(&key) {
            return idx;
        }
        let mut parts: Vec<String> = Vec::new();
        for w in words(&display).filter(|w| w.chars().count() >= MIN_NAME_CHARS) {
            let w = fold(w);
            if !parts.contains(&w) {
                parts.push(w);
            }
        }
        let idx = self.people.len();
        for part in &parts {
            for form in forms_of(part) {
                let list = self.forms.entry(form).or_default();
                if !list.contains(&idx) {
                    list.push(idx);
                }
            }
        }
        self.by_key.insert(key.clone(), idx);
        self.people.push(Person {
            key,
            display,
            parts,
        });
        idx
    }

    /// Osoba dla wartości z pola osobowego: istniejąca ze słownika, gdy cała
    /// wartość jest jej nazwą (w dowolnej odmianie i kolejności), inaczej nowa.
    fn ensure_person(&mut self, raw: &str) -> usize {
        let name = identity(raw);
        let (tokens, runs) = self.find_runs(&name);
        if let [run] = runs.as_slice() {
            if run.first == 0 && run.last + 1 == tokens.len() {
                return run.person;
            }
        }
        self.insert(&name)
    }

    /// Słowa tekstu z ich położeniem oraz połączone w osoby ciągi trafień.
    fn find_runs(&self, text: &str) -> (Vec<Token>, Vec<Run>) {
        let mut tokens: Vec<Token> = Vec::new();
        let mut start: Option<usize> = None;
        for (i, c) in text.char_indices() {
            match (c.is_alphanumeric(), start) {
                (true, None) => start = Some(i),
                (false, Some(s)) => {
                    tokens.push(Token::new(text, s, i));
                    start = None;
                }
                _ => {}
            }
        }
        if let Some(s) = start {
            tokens.push(Token::new(text, s, text.len()));
        }
        let mut runs = Vec::new();
        let mut i = 0;
        while i < tokens.len() {
            let Some(first) = self.forms.get(&tokens[i].folded) else {
                i += 1;
                continue;
            };
            let mut common: Vec<usize> = first.clone();
            let mut j = i;
            // Kolejne słowa rozdzielone samymi spacjami łączą się w jedno
            // trafienie, gdy pasują do tej samej osoby („Jan Kowalskiego”);
            // ciąg nie jest dłuższy niż liczba części nazwy osoby.
            while j + 1 < tokens.len() && only_blanks(&text[tokens[j].end..tokens[j + 1].start]) {
                let Some(next) = self.forms.get(&tokens[j + 1].folded) else {
                    break;
                };
                let len = j + 2 - i;
                let inter: Vec<usize> = common
                    .iter()
                    .copied()
                    .filter(|p| next.contains(p) && self.people[*p].parts.len() >= len)
                    .collect();
                if inter.is_empty() {
                    break;
                }
                common = inter;
                j += 1;
            }
            runs.push(Run {
                first: i,
                last: j,
                person: common[0],
            });
            i = j + 1;
        }
        (tokens, runs)
    }

    /// Zamienia osoby ze słownika w zwykłym tekście na pseudonimy.
    fn redact_text(&self, text: &str, ps: &mut Pseudonyms) -> String {
        let (tokens, runs) = self.find_runs(text);
        if runs.is_empty() {
            return text.to_string();
        }
        let mut out = String::with_capacity(text.len());
        let mut last = 0;
        for run in runs {
            out.push_str(&text[last..tokens[run.first].start]);
            out.push_str(&ps.label(&self.people[run.person]));
            last = tokens[run.last].end;
        }
        out.push_str(&text[last..]);
        out
    }

    /// Słownik z bazy: `ai_hidden_names` z ustawień, `extra_names` od wywołującego
    /// (np. własne imię i nazwisko z konta Jiry, osoby z Redmine) oraz osoby z pól
    /// Jiry typu osoba (`jira_fields.kind`) zapisanych przy zgłoszeniach
    /// (`jira_issue_fields`). Błąd bazy to błąd: filtr nie może po cichu
    /// działać na pustym słowniku.
    pub fn from_db(
        db: &Connection,
        settings: &AppSettings,
        extra_names: &[String],
    ) -> rusqlite::Result<Dictionary> {
        let mut dict = Dictionary::new();
        for id in ["assignee", "reporter", "creator"] {
            dict.add_person_field(id);
        }
        dict.add_hidden_names(&settings.ai_hidden_names);
        dict.add_hidden_names(extra_names);
        let ids: Vec<String> = db
            .prepare("SELECT DISTINCT id FROM jira_fields WHERE lower(kind) LIKE '%user%'")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<_, _>>()?;
        for id in &ids {
            dict.add_person_field(id);
        }
        let mut stmt =
            db.prepare("SELECT DISTINCT value FROM jira_issue_fields WHERE field_id=?1")?;
        let mut fields: Vec<String> = dict.person_fields.clone();
        fields.sort();
        for field in fields {
            let values: Vec<String> = stmt
                .query_map([&field], |r| r.get::<_, String>(0))?
                .collect::<Result<_, _>>()?;
            for value in values {
                for name in split_people(&value) {
                    dict.add_name(name);
                }
            }
        }
        Ok(dict)
    }
}

/// Wartość pola osobowego może zawierać kilka osób („Anna Nowak, Jan Kowalski”).
fn split_people(value: &str) -> impl Iterator<Item = &str> {
    value
        .split([',', ';'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn only_blanks(gap: &str) -> bool {
    !gap.is_empty()
        && gap
            .chars()
            .all(|c| matches!(c, ' ' | '\t' | '\u{a0}' | '-'))
}

#[derive(Debug)]
struct Token {
    start: usize,
    end: usize,
    folded: String,
}

impl Token {
    fn new(text: &str, start: usize, end: usize) -> Self {
        Token {
            start,
            end,
            folded: fold(&text[start..end]),
        }
    }
}

/// Ciąg słów (indeksy `first..=last`) będący jedną osobą ze słownika.
#[derive(Debug)]
struct Run {
    first: usize,
    last: usize,
    person: usize,
}

/// Pseudonimy nadane w obrębie wywołania (albo rozmowy czatu): osoba → „Osoba N”.
/// Mapa jest tylko w pamięci i nigdy nie trafia do modelu.
#[derive(Clone, Debug, Default)]
pub struct Pseudonyms {
    by_key: HashMap<String, usize>,
    /// Prawdziwe nazwy w kolejności nadania: „Osoba N” to `originals[N - 1]`.
    originals: Vec<String>,
}

impl Pseudonyms {
    pub fn new() -> Self {
        Self::default()
    }

    /// Liczba osób, którym nadano pseudonim.
    pub fn len(&self) -> usize {
        self.originals.len()
    }

    pub fn is_empty(&self) -> bool {
        self.originals.is_empty()
    }

    /// Prawdziwa nazwa dla numeru pseudonimu (od 1).
    pub fn original(&self, number: usize) -> Option<&str> {
        number
            .checked_sub(1)
            .and_then(|i| self.originals.get(i))
            .map(String::as_str)
    }

    fn label(&mut self, person: &Person) -> String {
        let next = self.originals.len();
        let idx = *self.by_key.entry(person.key.clone()).or_insert(next);
        if idx == next {
            self.originals.push(person.display.clone());
        }
        format!("Osoba {}", idx + 1)
    }

    /// Odwraca pseudonimy w tekście (odpowiedź modelu): „Osoba 2” i odmienione
    /// formy („Osoby 2”, „Osobie 2”, „Osobą 2”) wracają jako prawdziwa nazwa.
    /// Numer spoza mapy zostaje bez zmian.
    pub fn restore(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(pos) = rest.find(['O', 'o']) {
            let (before, here) = rest.split_at(pos);
            out.push_str(before);
            let boundary = out
                .chars()
                .next_back()
                .map_or(true, |c| !c.is_alphanumeric());
            if boundary {
                if let Some((len, name)) = self.match_label(here) {
                    out.push_str(name);
                    rest = &here[len..];
                    continue;
                }
            }
            let c = here.chars().next().unwrap_or('o');
            out.push(c);
            rest = &here[c.len_utf8()..];
        }
        out.push_str(rest);
        out
    }

    /// Dopasowuje „Osoba N” (także odmienioną) na początku `text`.
    fn match_label(&self, text: &str) -> Option<(usize, &str)> {
        const ENDINGS: &[&str] = &["a", "y", "ie", "ę", "ą", "o", "om", "ami", "ach", "ów"];
        let head = text.get(..4)?;
        if head.to_lowercase() != "osob" {
            return None;
        }
        let tail = &text[4..];
        let lower: String = tail
            .chars()
            .take(3)
            .flat_map(|c| c.to_lowercase())
            .collect();
        let ending = ENDINGS
            .iter()
            .filter(|e| lower.starts_with(**e))
            .max_by_key(|e| e.chars().count())?;
        let ending_len: usize = tail
            .chars()
            .take(ending.chars().count())
            .map(char::len_utf8)
            .sum();
        let after = &tail[ending_len..];
        let space = after
            .chars()
            .next()
            .filter(|c| *c == ' ' || *c == '\u{a0}')?;
        let digits = &after[space.len_utf8()..];
        let count = digits.chars().take_while(|c| c.is_ascii_digit()).count();
        if count == 0 || count > 6 {
            return None;
        }
        let number: usize = digits[..count].parse().ok()?;
        let name = self.original(number)?;
        Some((4 + ending_len + space.len_utf8() + count, name))
    }

    /// `restore` dla wartości JSON (odpowiedź modelu w JSON): zamienia teksty
    /// i klucze, nie rusza liczb ani struktury.
    pub fn restore_value(&self, value: &Value) -> Value {
        match value {
            Value::String(s) => Value::String(self.restore(s)),
            Value::Array(items) => {
                Value::Array(items.iter().map(|v| self.restore_value(v)).collect())
            }
            Value::Object(map) => Value::Object(
                map.iter()
                    .map(|(k, v)| (self.restore(k), self.restore_value(v)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }
}

/// Filtruje tekst albo JSON (tekst zaczynający się od `{` lub `[`, który jest
/// poprawnym JSON, traktowany jest jako JSON) i zwraca wynik oraz pseudonimy
/// nadane w tym wywołaniu.
pub fn redact(input: &str, dict: &Dictionary) -> (String, Pseudonyms) {
    let mut ps = Pseudonyms::new();
    let out = redact_with(input, dict, &mut ps);
    (out, ps)
}

/// Jak [`redact`], ale z istniejącą mapą pseudonimów: ta sama osoba ma ten sam
/// pseudonim w całej rozmowie czatu.
pub fn redact_with(input: &str, dict: &Dictionary, ps: &mut Pseudonyms) -> String {
    let trimmed = input.trim_start();
    if trimmed.starts_with(['{', '[']) {
        if let Ok(value) = serde_json::from_str::<Value>(input) {
            let out = redact_value(&value, dict, ps);
            let text = if input.contains('\n') {
                serde_json::to_string_pretty(&out)
            } else {
                serde_json::to_string(&out)
            };
            if let Ok(text) = text {
                return text;
            }
        }
    }
    dict.redact_text(input, ps)
}

/// Filtr wartości JSON. Osoby z pól osobowych trafiają przed przejściem właściwym
/// do roboczej kopii słownika, więc to samo nazwisko wpisane w zwykły tekst
/// w tym samym wywołaniu też zostaje ukryte.
pub fn redact_value(value: &Value, dict: &Dictionary, ps: &mut Pseudonyms) -> Value {
    let mut working = dict.clone();
    let mut scratch = Pseudonyms::new();
    walk(value, &mut working, &mut scratch);
    walk(value, &mut working, ps)
}

/// Przejście po JSON: pola osobowe w całości, reszta tekstów przez słownik.
fn walk(value: &Value, dict: &mut Dictionary, ps: &mut Pseudonyms) -> Value {
    match value {
        Value::String(s) => Value::String(dict.redact_text(s, ps)),
        Value::Array(items) => Value::Array(items.iter().map(|v| walk(v, dict, ps)).collect()),
        Value::Object(map) => {
            let field_is_person = FIELD_ID_KEYS.iter().any(|k| {
                map.get(*k)
                    .and_then(Value::as_str)
                    .is_some_and(|id| dict.is_person_field(id))
            });
            let mut out = Map::new();
            for (key, val) in map {
                let nk = norm_key(key);
                let new_val = if PERSON_KEYS.contains(&nk.as_str())
                    || dict.is_person_field(key)
                    || (field_is_person && key == "value")
                {
                    person_value(val, dict, ps)
                } else if HIDDEN_KEYS.contains(&nk.as_str()) && val.is_string() {
                    Value::String(HIDDEN.to_string())
                } else {
                    walk(val, dict, ps)
                };
                let mut new_key = dict.redact_text(key, ps);
                while out.contains_key(&new_key) {
                    new_key.push('\'');
                }
                out.insert(new_key, new_val);
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

/// Wartość pola osobowego: tekst (też lista po przecinku), lista albo obiekt
/// osoby; wszystko zamieniane w całości na pseudonimy.
fn person_value(value: &Value, dict: &mut Dictionary, ps: &mut Pseudonyms) -> Value {
    match value {
        Value::String(s) => {
            let labels: Vec<String> = split_people(s)
                .map(|name| pseudonym_of(name, dict, ps))
                .collect();
            if labels.is_empty() {
                Value::String(s.clone())
            } else {
                Value::String(labels.join(", "))
            }
        }
        Value::Array(items) => {
            Value::Array(items.iter().map(|v| person_value(v, dict, ps)).collect())
        }
        Value::Object(map) => {
            let name = NAME_KEYS.iter().find_map(|k| {
                map.get(*k)
                    .and_then(Value::as_str)
                    .filter(|s| !s.trim().is_empty())
            });
            match name {
                Some(name) => Value::String(pseudonym_of(name, dict, ps)),
                None => Value::String(HIDDEN.to_string()),
            }
        }
        other => other.clone(),
    }
}

fn pseudonym_of(name: &str, dict: &mut Dictionary, ps: &mut Pseudonyms) -> String {
    let idx = dict.ensure_person(name);
    ps.label(&dict.people[idx])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn dict(names: &[&str]) -> Dictionary {
        let mut d = Dictionary::new();
        for n in names {
            d.add_name(n);
        }
        d
    }

    fn text(d: &Dictionary, input: &str) -> String {
        redact(input, d).0
    }

    #[test]
    fn nested_json_and_lists_replace_person_fields_whole() {
        let d = Dictionary::new();
        let input = json!({
            "task": {"title": "Raport", "assignee": {"accountId": "a1", "displayName": "Anna Nowak", "emailAddress": "anna@x.pl"}},
            "issues": [
                {"key": "WMS-1", "reporter": "Jan Kowalski", "assignee": {"displayName": "Anna Nowak"}},
                {"key": "WMS-2", "assignee": null, "reporter": {"name": "Jan Kowalski"}}
            ],
            "commits": [{"author": "Ewa Zielińska <ewa@x.pl>", "message": "poprawka"}],
            "meeting": {"organizer": "Jan Kowalski", "attendees": ["Anna Nowak", "Piotr Wójcik"]},
            "minutes": 90
        });
        let (out, ps) = redact(&input.to_string(), &d);
        let out: Value = serde_json::from_str(&out).unwrap();
        // Klucze JSON są posortowane, więc numery zależą od kolejności kluczy:
        // sprawdzamy, kto kryje się pod pseudonimem, nie sam numer.
        let who = |v: &Value| -> String {
            let n: usize = v
                .as_str()
                .unwrap()
                .strip_prefix("Osoba ")
                .unwrap()
                .parse()
                .unwrap();
            ps.original(n).unwrap().to_string()
        };
        assert_eq!(who(&out["task"]["assignee"]), "Anna Nowak");
        assert_eq!(who(&out["issues"][0]["reporter"]), "Jan Kowalski");
        assert_eq!(out["issues"][0]["assignee"], out["task"]["assignee"]);
        assert_eq!(out["issues"][1]["assignee"], Value::Null);
        assert_eq!(out["issues"][1]["reporter"], out["issues"][0]["reporter"]);
        assert_eq!(who(&out["commits"][0]["author"]), "Ewa Zielińska");
        assert_eq!(out["commits"][0]["message"], "poprawka");
        assert_eq!(out["meeting"]["organizer"], out["issues"][0]["reporter"]);
        assert_eq!(who(&out["meeting"]["attendees"][0]), "Anna Nowak");
        assert_eq!(who(&out["meeting"]["attendees"][1]), "Piotr Wójcik");
        assert_eq!(out["minutes"], 90);
        assert_eq!(out["task"]["title"], "Raport");
        assert_eq!(ps.len(), 4);
        let flat = out.to_string();
        for leaked in [
            "Nowak",
            "Kowalski",
            "Zielińska",
            "anna@x.pl",
            "ewa@x.pl",
            "a1",
        ] {
            assert!(!flat.contains(leaked), "{leaked} w {flat}");
        }
    }

    #[test]
    fn person_fields_by_jira_field_id_and_field_value_shape() {
        let mut d = Dictionary::new();
        d.add_person_field("customfield_10050");
        let input = json!({
            "fields": [
                {"id": "customfield_10050", "name": "Tester", "value": "Marek Lis, Anna Nowak"},
                {"id": "labels", "name": "Etykiety", "value": "backend"}
            ],
            "customfield_10050": "Marek Lis"
        });
        let (out, _) = redact(&input.to_string(), &d);
        let out: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(out["fields"][0]["value"], "Osoba 1, Osoba 2");
        assert_eq!(out["fields"][0]["name"], "Tester");
        assert_eq!(out["fields"][1]["value"], "backend");
        assert_eq!(out["customfield_10050"], "Osoba 1");
    }

    #[test]
    fn pseudonyms_are_stable_within_a_call_and_conversation() {
        let d = dict(&["Anna Nowak", "Jan Kowalski"]);
        let (first, mut ps) = redact("Anna prosi Kowalskiego o review, potem Anny Nowak.", &d);
        assert_eq!(first, "Osoba 1 prosi Osoba 2 o review, potem Osoba 1.");
        // Ta sama mapa w kolejnej wiadomości czatu.
        let second = redact_with("Kowalski pisał do ANNA", &d, &mut ps);
        assert_eq!(second, "Osoba 2 pisał do Osoba 1");
        assert_eq!(ps.len(), 2);
        // Kolejność nadawania zależy od pierwszego wystąpienia, nie od słownika.
        let (reverse, _) = redact("Kowalski i Anna", &d);
        assert_eq!(reverse, "Osoba 1 i Osoba 2");
    }

    #[test]
    fn field_person_and_dictionary_person_share_one_pseudonym() {
        let d = dict(&["Anna Nowak"]);
        let input = json!({"assignee": "Nowak Anna", "note": "Pytanie do Anny Nowak"});
        let (out, ps) = redact(&input.to_string(), &d);
        let out: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(out["assignee"], "Osoba 1");
        assert_eq!(out["note"], "Pytanie do Osoba 1");
        assert_eq!(ps.len(), 1);
    }

    #[test]
    fn names_from_person_fields_are_hidden_in_plain_text_of_the_same_call() {
        let d = Dictionary::new();
        let input = json!({
            "commits": [{"author": "Zenon Nowy", "message": "Poprawki po uwagach Zenona Nowego"}]
        });
        let (out, _) = redact(&input.to_string(), &d);
        let out: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(out["commits"][0]["author"], "Osoba 1");
        assert_eq!(out["commits"][0]["message"], "Poprawki po uwagach Osoba 1");
    }

    /// Fixture z 20+ formami odmiany trzech osób: wszystkie znikają, każda z
    /// pseudonimem właściwej osoby.
    #[test]
    fn dictionary_covers_polish_inflection() {
        let d = dict(&["Jan Kowalski", "Anna Nowak", "Marek Zieliński"]);
        let kowalski = [
            "Kowalski",
            "Kowalskiego",
            "Kowalskiemu",
            "Kowalskim",
            "Kowalska",
            "Kowalskiej",
            "Kowalską",
            "Kowalscy",
            "Kowalskich",
            "Kowalskimi",
            "Kowalskie",
            "KOWALSKI",
        ];
        let anna = [
            "Anna", "Anny", "Annie", "Anną", "Annę", "Anno", "Ania", "Anią",
        ];
        let marek = [
            "Marek",
            "Marka",
            "Markowi",
            "Markiem",
            "Marku",
            "Zieliński",
            "Zielińskiego",
            "Zielinskim",
        ];
        assert!(kowalski.len() + anna.len() + marek.len() >= 20);
        for form in kowalski {
            assert_eq!(
                text(&d, &format!("To zrobił {form} wczoraj")),
                "To zrobił Osoba 1 wczoraj",
                "{form}"
            );
        }
        for form in anna {
            assert_eq!(
                text(&d, &format!("Pytanie do {form}.")),
                "Pytanie do Osoba 1.",
                "{form}"
            );
        }
        for form in marek {
            assert_eq!(text(&d, &format!("({form})")), "(Osoba 1)", "{form}");
        }
        // Pełne nazwy w różnej odmianie i kolejności są jednym trafieniem.
        assert_eq!(
            text(&d, "Jana Kowalskiego i Anna Nowak"),
            "Osoba 1 i Osoba 2"
        );
        assert_eq!(text(&d, "Kowalskiemu Janowi"), "Osoba 1");
    }

    #[test]
    fn short_names_are_skipped_and_words_are_matched_on_boundaries() {
        let mut d = Dictionary::new();
        assert!(!d.add_name("Jo"));
        assert!(!d.add_name("  "));
        assert!(d.add_name("Jan Li"));
        assert_eq!(text(&d, "Li pisze do Jana"), "Li pisze do Osoba 1");
        // „Annapurna” i „Hanna” tylko zawierają „Anna”.
        let d = dict(&["Anna"]);
        assert_eq!(
            text(&d, "Wyprawa na Annapurna, Annapurny i Hanna"),
            "Wyprawa na Annapurna, Annapurny i Hanna"
        );
        assert_eq!(
            text(&d, "Anna, Annapurna; (Anna)"),
            "Osoba 1, Annapurna; (Osoba 1)"
        );
        // „ani” (spójnik) nie jest formą imienia Anna.
        assert_eq!(text(&d, "ani Anna"), "ani Osoba 1");
    }

    /// Nazwisko będące zwykłym słowem jest podmieniane wszędzie: filtr działa
    /// po słowniku, nie po sensie (opisane w README i w Ustawieniach).
    #[test]
    fn surname_that_is_a_common_word_is_replaced_everywhere() {
        let d = dict(&["Piotr Marzec"]);
        assert_eq!(
            text(
                &d,
                "Raport za marzec: Marzec, MARZEC i zadanie Piotra Marzec"
            ),
            "Raport za Osoba 1: Osoba 1, Osoba 1 i zadanie Osoba 1"
        );
        // Nieregularnej odmiany („marca”) słownik nie zna.
        assert_eq!(text(&d, "do 5 marca"), "do 5 marca");
    }

    #[test]
    fn polish_letters_are_folded_both_ways() {
        let d = dict(&["Łukasz Wójcik"]);
        assert_eq!(text(&d, "Lukasz Wojcik"), "Osoba 1");
        assert_eq!(text(&d, "ŁUKASZA WÓJCIKA"), "Osoba 1");
        let d = dict(&["Lukasz Wojcik"]);
        assert_eq!(text(&d, "Łukasz Wójcik"), "Osoba 1");
    }

    #[test]
    fn restore_reverses_pseudonyms_in_text_and_json() {
        let d = dict(&["Anna Nowak", "Jan Kowalski"]);
        let (out, ps) = redact("Anna oddała zadanie Kowalskiemu", &d);
        assert_eq!(out, "Osoba 1 oddała zadanie Osoba 2");
        assert_eq!(
            ps.restore("Przypisz do Osoby 2, bo Osoba 1 jest zajęta; Osoba 3 i Osoba 12 zostają."),
            "Przypisz do Jan Kowalski, bo Anna Nowak jest zajęta; Osoba 3 i Osoba 12 zostają."
        );
        assert_eq!(
            ps.restore("OSOBA 1, osobie 2 i Osoba 1."),
            "Anna Nowak, Jan Kowalski i Anna Nowak."
        );
        assert_eq!(ps.restore("Nieosoba 1 i Osoba"), "Nieosoba 1 i Osoba");
        let value = json!({"Osoba 1": ["Osoba 2", 5], "n": null});
        assert_eq!(
            ps.restore_value(&value),
            json!({"Anna Nowak": ["Jan Kowalski", 5], "n": null})
        );
        // Mapa nie trafia do wyniku filtra.
        assert!(!out.contains("Nowak") && !out.contains("Kowalski"));
    }

    #[test]
    fn plain_text_that_looks_like_json_but_is_not_stays_text() {
        let d = dict(&["Anna Nowak"]);
        assert_eq!(text(&d, "[WMS-1] zadanie Anny"), "[WMS-1] zadanie Osoba 1");
        assert_eq!(text(&d, "{nie json Anna"), "{nie json Osoba 1");
    }

    #[test]
    fn json_text_values_and_keys_go_through_the_dictionary() {
        let d = dict(&["Anna Nowak"]);
        let input = json!({"byPerson": {"Anna Nowak": 120}, "title": "Review od Anny", "tags": ["anna", "x"]});
        let (out, _) = redact(&input.to_string(), &d);
        let out: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(out["byPerson"]["Osoba 1"], 120);
        assert_eq!(out["title"], "Review od Osoba 1");
        assert_eq!(out["tags"], json!(["Osoba 1", "x"]));
    }

    #[test]
    fn dictionary_is_built_from_db_settings_and_extra_names() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE jira_fields (site TEXT NOT NULL, id TEXT NOT NULL, name TEXT NOT NULL, custom INTEGER NOT NULL DEFAULT 0, kind TEXT NOT NULL DEFAULT '', PRIMARY KEY(site, id));
             CREATE TABLE jira_issue_fields (site TEXT NOT NULL, issue_key TEXT NOT NULL, field_id TEXT NOT NULL, value TEXT NOT NULL DEFAULT '', PRIMARY KEY(site, issue_key, field_id));
             INSERT INTO jira_fields VALUES ('s','assignee','Assignee',0,'user'), ('s','customfield_10050','Tester',1,'array:user'), ('s','labels','Labels',0,'array:string');
             INSERT INTO jira_issue_fields VALUES ('s','WMS-1','assignee','Anna Nowak'), ('s','WMS-1','customfield_10050','Marek Lis, Ewa Zielińska'), ('s','WMS-1','labels','Backend');",
        )
        .unwrap();
        let settings = AppSettings {
            ai_hidden_names: vec!["Piotr Marzec".into()],
            ..AppSettings::default()
        };
        let d = Dictionary::from_db(&db, &settings, &["Mateusz Krajnik".to_string()]).unwrap();
        for hit in ["Anna", "Lisa", "Ewie", "Marzec", "Krajnikiem"] {
            assert_eq!(text(&d, hit), "Osoba 1", "{hit}");
        }
        assert_eq!(text(&d, "Backend"), "Backend");
        let (out, _) = redact(&json!({"customfield_10050": "Marek Lis"}).to_string(), &d);
        assert_eq!(out, r#"{"customfield_10050":"Osoba 1"}"#);
        // Brak tabel to błąd, nie pusty słownik.
        let empty = Connection::open_in_memory().unwrap();
        assert!(Dictionary::from_db(&empty, &settings, &[]).is_err());
    }

    /// Szczelność: dzień pełen danych (zadania, notatki, Jira, spotkania, commity,
    /// czat); żadna nazwa ze słownika nie występuje w wyjściu.
    #[test]
    fn full_day_fixture_leaks_no_dictionary_name() {
        let names = [
            "Anna Nowak",
            "Jan Kowalski",
            "Marek Zieliński",
            "Łukasz Wójcik",
            "Ewa Kaczmarek-Lis",
            "Piotr Marzec",
        ];
        let d = dict(&names);
        let day = json!({
            "day": "2026-10-20",
            "user": "Mateusz Krajnik",
            "tasks": [
                {"id": 1, "title": "Poprawki po review Kowalskiego", "notes": "Anna prosiła o testy, Nowak sprawdzi w piątek", "jira": {
                    "key": "WMS-12", "assignee": {"accountId": "x1", "displayName": "Anna Nowak"},
                    "reporter": {"displayName": "Łukasz Wójcik", "emailAddress": "l.wojcik@firma.pl"},
                    "fields": [{"id": "labels", "name": "Etykiety", "value": "backend"}, {"id": "assignee", "name": "Przypisany", "value": "Anna Nowak"}]}},
                {"id": 2, "title": "Spotkanie z Markiem Zielińskim i Ewą Kaczmarek-Lis", "notes": "LUKASZ dostarczy dane; Jan K. nie dotarł"},
                {"id": 3, "title": "Raport za marzec", "notes": "uzgodnić z Piotrem Marzec"}
            ],
            "meetings": [{"subject": "Planning z Janem Kowalskim", "organizer": "Marek Zieliński", "attendees": ["Anna Nowak", "Łukasz Wójcik", "Nieznany Gość"]}],
            "commits": [{"author": "Jan Kowalski <jan.kowalski@firma.pl>", "message": "fix: poprawka dla Wójcika"}, {"author": "Nieznany Gość", "message": "Gość poprawił"}],
            "chat": "Zapytaj Annę Nowak albo Kowalskiemu, ewentualnie Zielińskiej"
        });
        let (out, ps) = redact(&day.to_string(), &d);
        let folded = fold(&out);
        for name in names {
            for part in words(name) {
                let part = fold(part);
                if part.chars().count() >= MIN_NAME_CHARS {
                    assert!(!folded.contains(&part), "{part} w {out}");
                }
            }
        }
        for leaked in ["nieznany", "gosc", "firma.pl", "x1"] {
            assert!(!folded.contains(leaked), "{leaked} w {out}");
        }
        // Pola bez osób zostają: struktura i dane techniczne.
        let out: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(out["day"], "2026-10-20");
        assert_eq!(out["tasks"][0]["jira"]["key"], "WMS-12");
        assert_eq!(out["tasks"][0]["jira"]["fields"][0]["value"], "backend");
        // Własne nazwisko spoza słownika też nie wychodzi, gdy jest w słowniku
        // (A1 dodaje je z konta Jiry); tu go nie ma, więc zostaje: test dokumentuje granicę.
        assert_eq!(out["user"], "Mateusz Krajnik");
        assert!(ps.len() >= 7);
    }
}
