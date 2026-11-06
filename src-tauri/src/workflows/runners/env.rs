//! Środowisko procesu agenta: czyste środowisko plus biała lista (E0, A05).
//! Nie przekazujemy np. tokenów z innych usług ani `TUKTUK_TRACK_AI`.

use std::ffi::OsString;

/// Zmienne potrzebne CLI do działania i logowania na Windows (porównanie bez
/// rozróżniania wielkości liter). Proxy, certyfikaty CA i ścieżka Git Bash są
/// potrzebne na maszynach firmowych (proxy, inspekcja TLS); nie są sekretami usług.
/// Proxy z danymi logowania w adresie nie przechodzi (zob. [`passes`]).
const ALLOWED: &[&str] = &[
    "SystemRoot",
    "windir",
    "ComSpec",
    "PATH",
    "PATHEXT",
    "TEMP",
    "TMP",
    "USERPROFILE",
    "HOMEDRIVE",
    "HOMEPATH",
    "APPDATA",
    "LOCALAPPDATA",
    "ProgramData",
    "ProgramFiles",
    "ProgramFiles(x86)",
    "ProgramW6432",
    "USERNAME",
    "CODEX_HOME",
    "CLAUDE_CONFIG_DIR",
    "SystemDrive",
    "HTTPS_PROXY",
    "HTTP_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
    "NODE_EXTRA_CA_CERTS",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "CLAUDE_CODE_GIT_BASH_PATH",
];

/// Zmienne proxy: przechodzą tylko bez danych logowania w adresie.
const PROXY_VARS: &[&str] = &["HTTPS_PROXY", "HTTP_PROXY", "ALL_PROXY"];

/// Zmienna oznaczająca przebieg i wykonanie; hook ją rozpoznaje (`hook_wire`) i pomija zdarzenia.
pub const WORKFLOW_VAR: &str = crate::hook_wire::WORKFLOW_ENV;

fn allowed(name: &str) -> bool {
    ALLOWED.iter().any(|a| a.eq_ignore_ascii_case(name))
}

/// Adres proxy z danymi logowania (`http://user:pass@host`).
fn has_credentials(url: &str) -> bool {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    authority.contains('@')
}

/// Zmienna przechodzi do procesu agenta. Proxy z hasłem w adresie odpada w
/// całości: danych logowania nie przekazujemy (proxy z uwierzytelnianiem
/// trzeba skonfigurować w samym CLI).
fn passes(name: &str, value: &str) -> bool {
    if !allowed(name) {
        return false;
    }
    let is_proxy = PROXY_VARS.iter().any(|p| p.eq_ignore_ascii_case(name));
    !(is_proxy && has_credentials(value))
}

/// Biała lista z podanego źródła zmiennych plus znacznik przebiegu.
pub fn build(
    vars: impl IntoIterator<Item = (OsString, OsString)>,
    run_id: i64,
    execution_id: &str,
) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = vars
        .into_iter()
        .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
        .filter(|(k, v)| passes(k, v))
        .collect();
    env.push((WORKFLOW_VAR.to_owned(), format!("{run_id}:{execution_id}")));
    env
}

/// Z prawdziwego środowiska aplikacji.
pub fn from_process(run_id: i64, execution_id: &str) -> Vec<(String, String)> {
    build(std::env::vars_os(), run_id, execution_id)
}
