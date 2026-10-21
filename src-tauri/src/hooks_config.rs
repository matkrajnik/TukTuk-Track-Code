//! Rejestracja hooków agentów w plikach Claude Code i Codexa (PLAN_BACKLOG.md, B8).
//!
//! Wspólny plik aplikacji (`mod hooks_config`) i programu `todo-track-hook`
//! (`src/bin/hook.rs`, przez `#[path]`, dla `--uninstall` z deinstalatora). Tylko
//! `serde_json` i `std`, bez Tauri.
//!
//! - Claude Code: `<~/.claude>/settings.json`, Codex: `<~/.codex>/hooks.json`; w obu
//!   `{"hooks": {"<Zdarzenie>": [{"hooks": [{"type": "command", "command": …}]}]}}`.
//! - Scalamy JSON: dopisujemy albo usuwamy wyłącznie własne wpisy (polecenie z nazwą
//!   `todo-track-hook`), wpisy innych programów (np. Orki) zostają nietknięte.
//! - Plik, którego nie da się sparsować (np. z komentarzami), zostaje bez zmian,
//!   a użytkownik dostaje komunikat.
//! - Folder agenta, którego nie ma (agent nie jest zainstalowany), jest pomijany.
//!   Ścieżki są parametrem (`Dirs`), więc testy pracują w katalogu tymczasowym.
#![allow(dead_code)]

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Fragment polecenia wpisu hooka aplikacji; po nim rozpoznajemy własne wpisy.
pub const OWN: &str = "todo-track-hook";
/// Plik programu hooka obok aplikacji.
pub const HOOK_EXE: &str = "todo-track-hook.exe";
/// Limit czasu hooka (s); program kończy się w milisekundach.
const TIMEOUT: u64 = 5;

/// Zdarzenia Claude Code, które aplikacja czyta.
pub const CLAUDE_EVENTS: [&str; 10] = [
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PermissionRequest",
    "Notification",
    "SubagentStart",
    "SubagentStop",
    "Stop",
    "SessionEnd",
];
/// Zdarzenia Codexa, które aplikacja czyta.
pub const CODEX_EVENTS: [&str; 10] = [
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PermissionRequest",
    "PostToolUse",
    "SubagentStart",
    "SubagentStop",
    "Stop",
    "Interrupt",
    "SessionEnd",
];

/// Foldery konfiguracji agentów (`None`: nie znaleziono katalogu domowego).
#[derive(Clone, Debug, Default)]
pub struct Dirs {
    pub claude: Option<PathBuf>,
    pub codex: Option<PathBuf>,
}

impl Dirs {
    /// `CLAUDE_CONFIG_DIR` / `~/.claude` i `CODEX_HOME` / `~/.codex`.
    pub fn detect() -> Self {
        let home = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from);
        Self {
            claude: std::env::var_os("CLAUDE_CONFIG_DIR")
                .map(PathBuf::from)
                .or_else(|| home.as_ref().map(|h| h.join(".claude"))),
            codex: std::env::var_os("CODEX_HOME")
                .map(PathBuf::from)
                .or_else(|| home.as_ref().map(|h| h.join(".codex"))),
        }
    }
    pub fn claude_file(&self) -> Option<PathBuf> {
        self.claude.as_ref().map(|d| d.join("settings.json"))
    }
    pub fn codex_file(&self) -> Option<PathBuf> {
        self.codex.as_ref().map(|d| d.join("hooks.json"))
    }
    pub fn codex_config(&self) -> Option<PathBuf> {
        self.codex.as_ref().map(|d| d.join("config.toml"))
    }
}

/// Czy polecenie należy do hooka aplikacji.
pub fn is_own(command: &str) -> bool {
    command.to_ascii_lowercase().contains(OWN)
}

/// Polecenie hooka: ścieżka exe (ukośniki `/` działają w bashu, cmd i PowerShellu,
/// cudzysłów tylko gdy ścieżka go wymaga) i nazwa agenta.
pub fn hook_command(exe: &Path, agent: &str) -> String {
    let path = exe.to_string_lossy().replace('\\', "/");
    let path = path.strip_prefix("//?/").unwrap_or(&path).to_string();
    let quote = path
        .chars()
        .any(|c| c.is_whitespace() || "&()^;'`$%!".contains(c));
    if quote {
        format!("\"{path}\" {agent}")
    } else {
        format!("{path} {agent}")
    }
}

fn group(command: &str) -> Value {
    json!({ "hooks": [{ "type": "command", "command": command, "timeout": TIMEOUT }] })
}

/// Usuwa własne wpisy z listy grup zdarzenia; `keep` to polecenie, którego pierwszy
/// wpis zostaje. Zwraca (czy zmieniono, czy zostawiono wpis `keep`). Grupy, które
/// opróżniliśmy, znikają; obce wpisy i grupy zostają.
fn strip_own(groups: &mut Vec<Value>, keep: Option<&str>) -> (bool, bool) {
    let mut changed = false;
    let mut kept = false;
    groups.retain_mut(|g| {
        let Some(list) = g.get_mut("hooks").and_then(Value::as_array_mut) else {
            return true;
        };
        let before = list.len();
        list.retain(|h| {
            let Some(cmd) = h.get("command").and_then(Value::as_str) else {
                return true;
            };
            if !is_own(cmd) {
                return true;
            }
            if Some(cmd) == keep && !kept {
                kept = true;
                return true;
            }
            false
        });
        if list.len() != before {
            changed = true;
            !list.is_empty()
        } else {
            true
        }
    });
    (changed, kept)
}

fn hooks_object(root: &mut Value) -> Result<&mut serde_json::Map<String, Value>, String> {
    root.as_object_mut()
        .ok_or("Plik konfiguracji nie jest obiektem JSON.")?
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| "Pole „hooks” w pliku konfiguracji nie jest obiektem.".to_string())
}

/// Dopisuje wpisy hooka dla `events` z poleceniem `command`: brakujące dodaje,
/// wpisy ze starym poleceniem (np. po zmianie ścieżki exe) zastępuje, własne wpisy
/// ze zdarzeń spoza listy usuwa. Zwraca, czy JSON się zmienił.
pub fn install(root: &mut Value, events: &[&str], command: &str) -> Result<bool, String> {
    let hooks = hooks_object(root)?;
    let mut changed = false;
    let stale: Vec<String> = hooks
        .keys()
        .filter(|k| !events.contains(&k.as_str()))
        .cloned()
        .collect();
    for key in stale {
        let Some(list) = hooks.get_mut(&key).and_then(Value::as_array_mut) else {
            continue;
        };
        let (c, _) = strip_own(list, None);
        if c {
            changed = true;
            if list.is_empty() {
                hooks.remove(&key);
            }
        }
    }
    for event in events {
        let list = hooks
            .entry(event.to_string())
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| format!("Pole hooks.{event} w pliku konfiguracji nie jest listą."))?;
        let (c, kept) = strip_own(list, Some(command));
        changed |= c;
        if !kept {
            list.push(group(command));
            changed = true;
        }
    }
    Ok(changed)
}

/// Usuwa wszystkie własne wpisy; obce zostają. Puste listy i pusty obiekt `hooks`
/// znikają tylko wtedy, gdy to my je opróżniliśmy.
pub fn uninstall(root: &mut Value) -> Result<bool, String> {
    let Some(hooks) = root.get_mut("hooks") else {
        return Ok(false);
    };
    let hooks = hooks
        .as_object_mut()
        .ok_or("Pole „hooks” w pliku konfiguracji nie jest obiektem.")?;
    let mut changed = false;
    let keys: Vec<String> = hooks.keys().cloned().collect();
    for key in keys {
        let Some(list) = hooks.get_mut(&key).and_then(Value::as_array_mut) else {
            continue;
        };
        let (c, _) = strip_own(list, None);
        if c {
            changed = true;
            if list.is_empty() {
                hooks.remove(&key);
            }
        }
    }
    if changed && hooks.is_empty() {
        root.as_object_mut().map(|o| o.remove("hooks"));
    }
    Ok(changed)
}

/// Stan rejestracji w pliku.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Registration {
    /// Brak własnych wpisów.
    Missing,
    /// Są własne wpisy, ale nie wszystkie, albo ze starym poleceniem.
    Stale,
    /// Wszystkie zdarzenia mają wpis z aktualnym poleceniem.
    Current,
}

impl Registration {
    pub fn as_str(self) -> &'static str {
        match self {
            Registration::Missing => "missing",
            Registration::Stale => "stale",
            Registration::Current => "current",
        }
    }
}

pub fn registration(root: &Value, events: &[&str], command: &str) -> Registration {
    let Some(hooks) = root.get("hooks").and_then(Value::as_object) else {
        return Registration::Missing;
    };
    fn own(h: &Value) -> Option<&str> {
        h.get("command").and_then(Value::as_str)
    }
    let commands = |event: &str| -> Vec<String> {
        hooks
            .get(event)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|g| g.get("hooks").and_then(Value::as_array))
            .flatten()
            .filter_map(own)
            .filter(|c| is_own(c))
            .map(str::to_string)
            .collect()
    };
    let any_own = hooks.keys().any(|k| !commands(k).is_empty());
    if !any_own {
        return Registration::Missing;
    }
    let all_current = events
        .iter()
        .all(|e| commands(e).iter().any(|c| c == command))
        && hooks
            .keys()
            .filter(|k| !events.contains(&k.as_str()))
            .all(|k| commands(k).is_empty());
    if all_current {
        Registration::Current
    } else {
        Registration::Stale
    }
}

// --- kolejność kluczy ---
//
// `serde_json::Value` bez cechy `preserve_order` trzyma klucze obiektów posortowane.
// Żeby zapis nie przestawiał kluczy w plikach agentów (i żeby nie włączać tej cechy
// dla całej aplikacji), przy odczycie zapamiętujemy kolejność kluczy z pliku
// ([`Order`]), a przy zapisie wypisujemy obiekty w tej kolejności; nowe klucze
// trafiają na koniec obiektu.

/// Kolejność kluczy dokumentu JSON (same klucze, bez wartości prostych).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Order {
    keys: Vec<(String, Order)>,
    items: Vec<Order>,
}

impl<'de> serde::Deserialize<'de> for Order {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = Order;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("JSON")
            }
            fn visit_bool<E>(self, _: bool) -> Result<Order, E> {
                Ok(Order::default())
            }
            fn visit_i64<E>(self, _: i64) -> Result<Order, E> {
                Ok(Order::default())
            }
            fn visit_u64<E>(self, _: u64) -> Result<Order, E> {
                Ok(Order::default())
            }
            fn visit_f64<E>(self, _: f64) -> Result<Order, E> {
                Ok(Order::default())
            }
            fn visit_str<E>(self, _: &str) -> Result<Order, E> {
                Ok(Order::default())
            }
            fn visit_unit<E>(self) -> Result<Order, E> {
                Ok(Order::default())
            }
            fn visit_none<E>(self) -> Result<Order, E> {
                Ok(Order::default())
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Order, A::Error> {
                let mut items = Vec::new();
                while let Some(item) = seq.next_element::<Order>()? {
                    items.push(item);
                }
                Ok(Order {
                    keys: Vec::new(),
                    items,
                })
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Order, A::Error> {
                let mut keys = Vec::new();
                while let Some(key) = map.next_key::<String>()? {
                    let sub = map.next_value::<Order>()?;
                    keys.push((key, sub));
                }
                Ok(Order {
                    keys,
                    items: Vec::new(),
                })
            }
        }
        deserializer.deserialize_any(V)
    }
}

/// Wartość do zapisu z podpowiedzią kolejności: oryginalna wartość z pliku i jej
/// kolejność kluczy (`None`: brak odpowiednika w pliku, klucze posortowane).
struct Ordered<'a> {
    value: &'a Value,
    hint: Option<(&'a Value, &'a Order)>,
}

impl serde::Serialize for Ordered<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::{SerializeMap, SerializeSeq};
        match self.value {
            Value::Object(map) => {
                let mut out = serializer.serialize_map(Some(map.len()))?;
                let mut done: Vec<&str> = Vec::new();
                if let Some((Value::Object(orig), order)) = self.hint {
                    for (key, sub) in &order.keys {
                        if done.contains(&key.as_str()) {
                            continue;
                        }
                        if let Some(value) = map.get(key) {
                            done.push(key);
                            let hint = orig.get(key).map(|o| (o, sub));
                            out.serialize_entry(key, &Ordered { value, hint })?;
                        }
                    }
                }
                for (key, value) in map {
                    if !done.contains(&key.as_str()) {
                        out.serialize_entry(key, &Ordered { value, hint: None })?;
                    }
                }
                out.end()
            }
            Value::Array(list) => {
                let mut out = serializer.serialize_seq(Some(list.len()))?;
                let orig = match self.hint {
                    Some((Value::Array(orig), order)) => Some((orig, order)),
                    _ => None,
                };
                let mut used = vec![false; orig.map_or(0, |(o, _)| o.len())];
                for (i, value) in list.iter().enumerate() {
                    // Element bez zmian (także po usunięciu wcześniejszych) ma swój
                    // odpowiednik w pliku; zmieniony bierze kolejność z tej samej pozycji.
                    let hint = orig.and_then(|(o, order)| {
                        let at = o
                            .iter()
                            .enumerate()
                            .position(|(j, v)| !used[j] && v == value)
                            .or_else(|| (i < o.len() && !used[i]).then_some(i))?;
                        used[at] = true;
                        Some((&o[at], order.items.get(at)?))
                    });
                    out.serialize_element(&Ordered { value, hint })?;
                }
                out.end()
            }
            other => other.serialize(serializer),
        }
    }
}

/// Plik JSON agenta: wartość do scalania i kolejność kluczy z dysku.
#[derive(Clone, Debug, Default)]
pub struct Doc {
    pub value: Value,
    original: Value,
    order: Order,
}

impl Doc {
    /// Dokument z tekstu pliku (pusty tekst to pusty obiekt).
    pub fn parse(text: &str) -> Result<Self, serde_json::Error> {
        let text = text.trim_start_matches('\u{feff}');
        if text.trim().is_empty() {
            return Ok(Self {
                value: json!({}),
                original: json!({}),
                order: Order::default(),
            });
        }
        let value: Value = serde_json::from_str(text)?;
        let order: Order = serde_json::from_str(text)?;
        Ok(Self {
            original: value.clone(),
            value,
            order,
        })
    }

    /// Tekst do zapisu: obiekty w kolejności kluczy z pliku, nowe klucze na końcu.
    pub fn to_text(&self) -> Result<String, String> {
        let ordered = Ordered {
            value: &self.value,
            hint: Some((&self.original, &self.order)),
        };
        let mut text = serde_json::to_string_pretty(&ordered).map_err(|e| e.to_string())?;
        text.push('\n');
        Ok(text)
    }
}

/// Odczyt pliku JSON: brak pliku to pusty obiekt, niepoprawny JSON to błąd
/// (plik zostaje nietknięty).
pub fn read_doc(path: &Path) -> Result<Doc, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("Nie udało się odczytać {}: {e}", path.display())),
    };
    Doc::parse(&text).map_err(|_| {
        format!(
            "Plik {} nie jest poprawnym JSON-em (może zawierać komentarze); nie zmieniam go.",
            path.display()
        )
    })
}

/// Sama wartość pliku JSON (jak [`read_doc`]).
pub fn read_json(path: &Path) -> Result<Value, String> {
    read_doc(path).map(|d| d.value)
}

/// Zapis przez plik tymczasowy i podmianę, żeby agent nie przeczytał połowy pliku.
pub fn write_doc(path: &Path, doc: &Doc) -> Result<(), String> {
    let text = doc.to_text()?;
    let tmp = path.with_extension("json.todo-track-tmp");
    std::fs::write(&tmp, text)
        .and_then(|_| std::fs::rename(&tmp, path))
        .map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("Nie udało się zapisać {}: {e}", path.display())
        })
}

/// Wynik dla jednego agenta.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Folder agenta istnieje.
    pub found: bool,
    pub changed: bool,
    pub error: String,
}

/// Stosuje rejestrację w pliku agenta: `command = Some` dopisuje wpisy, `None`
/// usuwa własne. Brak folderu agenta to brak działania.
fn sync_file(
    dir: Option<&Path>,
    file: Option<PathBuf>,
    events: &[&str],
    command: Option<&str>,
) -> Outcome {
    let (Some(dir), Some(file)) = (dir, file) else {
        return Outcome::default();
    };
    if !dir.is_dir() {
        return Outcome::default();
    }
    let mut out = Outcome {
        found: true,
        ..Outcome::default()
    };
    let result = (|| {
        let mut doc = read_doc(&file)?;
        let changed = match command {
            Some(c) => install(&mut doc.value, events, c)?,
            None => {
                if !file.exists() {
                    return Ok(false);
                }
                uninstall(&mut doc.value)?
            }
        };
        if changed {
            write_doc(&file, &doc)?;
        }
        Ok::<bool, String>(changed)
    })();
    match result {
        Ok(changed) => out.changed = changed,
        Err(e) => out.error = e,
    }
    out
}

/// Rejestruje (`exe = Some`) albo usuwa (`None`) hooki w obu agentach.
pub fn sync(dirs: &Dirs, exe: Option<&Path>) -> (Outcome, Outcome) {
    let cmd = |agent: &str| exe.map(|e| hook_command(e, agent));
    (
        sync_file(
            dirs.claude.as_deref(),
            dirs.claude_file(),
            &CLAUDE_EVENTS,
            cmd("claude").as_deref(),
        ),
        sync_file(
            dirs.codex.as_deref(),
            dirs.codex_file(),
            &CODEX_EVENTS,
            cmd("codex").as_deref(),
        ),
    )
}

/// Stan rejestracji w pliku agenta: (folder istnieje, rejestracja, błąd odczytu).
pub fn inspect(
    dir: Option<&Path>,
    file: Option<PathBuf>,
    events: &[&str],
    command: &str,
) -> (bool, Registration, String) {
    let (Some(dir), Some(file)) = (dir, file) else {
        return (false, Registration::Missing, String::new());
    };
    if !dir.is_dir() {
        return (false, Registration::Missing, String::new());
    }
    match read_json(&file) {
        Ok(root) => (true, registration(&root, events, command), String::new()),
        Err(e) => (true, Registration::Missing, e),
    }
}
