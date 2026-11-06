//! Artefakty przebiegu: diff adresowany skrótem (PLAN_WDROZENIA.md, 7.1 i 7.3; E5).
//!
//! Zgoda dotyczy dokładnie tych bajtów, które zobaczył użytkownik, więc diff
//! zapisuje się pod nazwą z jego SHA-256, w katalogu danych aplikacji (nigdy w
//! repozytorium użytkownika). Klient prosi o artefakt przez (runId, skrót), nigdy
//! przez ścieżkę; ścieżkę składa host z wiersza w `workflow_artifacts`.
//!
//! Sekrety (8.3): plik i skrót są surowe (zgoda dotyczy bajtów worktree), a tekst
//! zwracany do widoku przechodzi przez `redact_for_view` z jawną liczbą ukrytych
//! fragmentów.

use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};
use workflow_core::ports::RunId;

use super::runners::is_secret_word;
use super::workspace::{canonical, is_under, workflows_dir, SharedDb};

/// Największy fragment diffu zwracany do widoku (z jawną flagą przycięcia).
pub const MAX_DIFF_VIEW_BYTES: usize = 2 * 1024 * 1024;

const ERR_UNAVAILABLE: &str = "Artefakt jest niedostępny lub uszkodzony.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactError {
    /// Brak wiersza dla pary (przebieg, skrót) albo zły skrót.
    Unknown,
    /// Plik zniknął, wyszedł poza katalog artefaktów albo skrót się nie zgadza.
    Unavailable,
    /// Plik usunęła retencja (wiersz indeksu zostaje z `purged_at`).
    Purged,
    /// Brak miejsca na dysku przy zapisie; plik częściowy jest usuwany.
    DiskFull,
    Storage(String),
}

impl fmt::Display for ArtifactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArtifactError::Unknown => f.write_str("Nie znaleziono artefaktu tego przebiegu."),
            ArtifactError::Unavailable => f.write_str(ERR_UNAVAILABLE),
            ArtifactError::Purged => f.write_str(
                "Diff usunięto po okresie przechowywania (Ustawienia → Workflows); historia przebiegu zostaje.",
            ),
            ArtifactError::DiskFull => f.write_str(
                "Brak miejsca na dysku: nie zapisano diffu. Zwolnij miejsce i uruchom przebieg ponownie.",
            ),
            ArtifactError::Storage(m) => write!(f, "Błąd zapisu artefaktu: {m}"),
        }
    }
}

impl std::error::Error for ArtifactError {}

fn storage(e: impl fmt::Display) -> ArtifactError {
    ArtifactError::Storage(e.to_string())
}

/// Błąd zapisu pliku: pełny dysk ma własny, czytelny komunikat.
fn write_error(e: std::io::Error) -> ArtifactError {
    if super::disk::is_disk_full(&e) {
        ArtifactError::DiskFull
    } else {
        storage(e)
    }
}

/// Domyślny zapis pliku tymczasowego: treść i `sync_all`.
fn write_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Widok diffu dla interfejsu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffView {
    /// Tekst do pokazania; słowa przypominające sekrety zastąpione `[ukryte]`.
    pub text: String,
    /// Ile fragmentów zamaskowano (interfejs pokazuje to jawnie).
    pub redacted: usize,
    pub truncated: bool,
    pub size: u64,
    pub hash: String,
}

pub(crate) fn is_hash(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| b.is_ascii_hexdigit())
}

pub(crate) fn runs_dir(app_data: &Path) -> PathBuf {
    workflows_dir(app_data).join("runs")
}

pub(crate) fn rel_path(run_id: RunId, hash: &str) -> String {
    format!("workflows/runs/{run_id}/diff-{hash}.patch")
}

/// Szczegóły przebiegu z dostępnością plików diffów: zgoda, której plik zniknął
/// (np. kopia zapasowa bez diffu), ma `artifact_available = false`, więc monitor
/// mówi o braku pliku zamiast pokazywać błąd odczytu. Wołać bez blokady bazy.
pub fn with_file_availability(
    detail: super::runs::RunDetail,
    app_data: &Path,
) -> super::runs::RunDetail {
    let run_id = detail.summary.id;
    let approvals = detail
        .approvals
        .into_iter()
        .map(|approval| {
            let available = approval.artifact_available
                && is_hash(&approval.artifact_hash)
                && app_data
                    .join(rel_path(run_id, &approval.artifact_hash))
                    .is_file();
            super::runs::ApprovalView {
                artifact_available: available,
                ..approval
            }
        })
        .collect();
    super::runs::RunDetail {
        approvals,
        ..detail
    }
}

/// Zapisuje diff atomowo (plik tymczasowy + rename) i indeksuje go w bazie.
/// Plik nosi skrót w nazwie, więc ponowny zapis jest idempotentny. Wiersz
/// powstaje w osobnej, krótkiej transakcji, już po zapisie pliku.
pub fn store_diff(
    db: &SharedDb,
    app_data: &Path,
    run_id: RunId,
    bytes: &[u8],
    hash: &str,
    now: i64,
) -> Result<(), ArtifactError> {
    store_diff_with(
        db,
        app_data,
        run_id,
        DiffBytes { bytes, hash },
        now,
        &FileOps::real(),
    )
}

/// Diff z jego skrótem.
#[derive(Clone, Copy)]
pub(crate) struct DiffBytes<'a> {
    pub bytes: &'a [u8],
    pub hash: &'a str,
}

/// Operacje plikowe zapisu atomowego (testy wstrzykują błędy).
pub(crate) struct FileOps<'a> {
    pub write: &'a dyn Fn(&Path, &[u8]) -> std::io::Result<()>,
    pub rename: &'a dyn Fn(&Path, &Path) -> std::io::Result<()>,
}

impl FileOps<'static> {
    /// Zwykły zapis na dysk.
    pub(crate) fn real() -> Self {
        FileOps {
            write: &write_file,
            rename: &|from, to| std::fs::rename(from, to),
        }
    }
}

/// Licznik nazw plików tymczasowych: dwa równoległe zapisy tego samego skrótu
/// (także z dwóch procesów: pid w nazwie) nie piszą do jednego pliku.
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Plik tymczasowy usuwany przy każdym błędzie (Drop), dopóki nie zostanie przeniesiony.
struct TempFile {
    path: PathBuf,
    keep: bool,
}

impl Drop for TempFile {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

pub(crate) fn write_atomic(
    target: &Path,
    hash: &str,
    bytes: &[u8],
    ops: &FileOps<'_>,
) -> Result<(), ArtifactError> {
    let dir = target.parent().ok_or(ArtifactError::Unavailable)?;
    std::fs::create_dir_all(dir).map_err(write_error)?;
    let unique = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut tmp = TempFile {
        path: dir.join(format!("diff-{hash}.{}-{unique}.tmp", std::process::id())),
        keep: false,
    };
    // Błąd zapisu (także pełny dysk) usuwa plik tymczasowy w `Drop`.
    (ops.write)(&tmp.path, bytes).map_err(write_error)?;
    match (ops.rename)(&tmp.path, target) {
        Ok(()) => {
            tmp.keep = true;
            Ok(())
        }
        // Równoległy zapis tego samego skrótu zdążył pierwszy: treść jest ta sama.
        Err(_) if target.is_file() => Ok(()),
        Err(e) => Err(storage(e)),
    }
}

pub(crate) fn store_diff_with(
    db: &SharedDb,
    app_data: &Path,
    run_id: RunId,
    diff: DiffBytes,
    now: i64,
    ops: &FileOps<'_>,
) -> Result<(), ArtifactError> {
    let DiffBytes { bytes, hash } = diff;
    if !is_hash(hash) || hex_of(bytes) != hash {
        return Err(ArtifactError::Storage("Skrót nie pasuje do diffu.".into()));
    }
    let rel = rel_path(run_id, hash);
    let target = app_data.join(&rel);
    if !target.exists() {
        write_atomic(&target, hash, bytes, ops)?;
    }
    let conn = db
        .lock()
        .map_err(|_| ArtifactError::Storage("Blokada bazy jest zatruta.".into()))?;
    conn.execute(
        "INSERT OR IGNORE INTO workflow_artifacts(run_id, kind, hash, rel_path, size, created_at)
         VALUES(?1, 'diff', ?2, ?3, ?4, ?5)",
        params![run_id, hash, rel, bytes.len() as i64, now],
    )
    .map_err(storage)?;
    Ok(())
}

const HIDDEN: &str = "[ukryte]";

fn is_key_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || "-_.=".contains(c)
}

/// Maskuje w jednej linii diffu słowa przypominające sekrety (jak
/// `runners::redact`), ale zachowuje układ: odstępy, znacznik `+`/`-` i cudzysłowy.
fn redact_line(line: &str, hidden: &mut usize, out: &mut String) {
    let (marker, mut rest) = match line.chars().next() {
        Some(c @ ('+' | '-' | ' ')) => (c.len_utf8(), &line[c.len_utf8()..]),
        _ => (0, line),
    };
    out.push_str(&line[..marker]);
    let mut hide_next = false;
    while !rest.is_empty() {
        let gap = rest
            .find(|c: char| !c.is_whitespace())
            .unwrap_or(rest.len());
        out.push_str(&rest[..gap]);
        rest = &rest[gap..];
        if rest.is_empty() {
            break;
        }
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let word = &rest[..end];
        rest = &rest[end..];
        let start = word.find(is_key_char).unwrap_or(word.len());
        let stop = word.rfind(is_key_char).map_or(start, |i| i + 1);
        let core = &word[start..stop];
        let lower = word.to_ascii_lowercase();
        if !core.is_empty() && (hide_next || is_secret_word(core)) {
            out.push_str(&word[..start]);
            out.push_str(HIDDEN);
            out.push_str(&word[stop..]);
            *hidden += 1;
            hide_next = false;
        } else {
            out.push_str(word);
            hide_next = core.eq_ignore_ascii_case("bearer") || lower.ends_with("token:");
        }
    }
}

/// Tekst diffu do widoku zgody z zamaskowanymi sekretami i ich liczbą
/// (PLAN_WDROZENIA.md, 8.3). Plik i skrót zostają surowe: zgoda dotyczy bajtów
/// worktree, a redakcja tylko tego, co trafia na ekran.
pub fn redact_for_view(text: &str) -> (String, usize) {
    let mut out = String::with_capacity(text.len());
    let mut hidden = 0;
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        redact_line(line, &mut hidden, &mut out);
    }
    (out, hidden)
}

pub(crate) fn hex_of(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Czyta artefakt (runId, skrót). Weryfikuje, że bajty pliku mają zapisany skrót.
pub fn read_diff(
    db: &SharedDb,
    app_data: &Path,
    run_id: RunId,
    hash: &str,
    max_bytes: usize,
) -> Result<DiffView, ArtifactError> {
    if !is_hash(hash) {
        return Err(ArtifactError::Unknown);
    }
    let row: Option<(String, Option<i64>)> = {
        let conn = db
            .lock()
            .map_err(|_| ArtifactError::Storage("Blokada bazy jest zatruta.".into()))?;
        conn.query_row(
            "SELECT rel_path, purged_at FROM workflow_artifacts WHERE run_id = ?1 AND kind = 'diff' AND hash = ?2",
            params![run_id, hash],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(storage)?
    };
    let (rel, purged_at) = row.ok_or(ArtifactError::Unknown)?;
    if purged_at.is_some() {
        return Err(ArtifactError::Purged);
    }
    let root = canonical(&runs_dir(app_data)).map_err(|_| ArtifactError::Unavailable)?;
    let path = canonical(&app_data.join(rel)).map_err(|_| ArtifactError::Unavailable)?;
    if !is_under(&path, &root) {
        return Err(ArtifactError::Unavailable);
    }
    let (shown, size, digest) = read_prefix_hashing(&path, max_bytes)?;
    if digest != hash {
        return Err(ArtifactError::Unavailable);
    }
    let (text, redacted) = redact_for_view(&String::from_utf8_lossy(&shown));
    Ok(DiffView {
        text,
        redacted,
        truncated: size > shown.len() as u64,
        size,
        hash: hash.to_owned(),
    })
}

/// Czyta plik strumieniowo: skrót liczy się z całości, w pamięci zostaje tylko
/// pierwsze `max_bytes` do pokazania (duży diff nie ląduje cały w RAM).
fn read_prefix_hashing(
    path: &Path,
    max_bytes: usize,
) -> Result<(Vec<u8>, u64, String), ArtifactError> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|_| ArtifactError::Unavailable)?;
    let mut hasher = Sha256::new();
    let mut shown = Vec::new();
    let mut size = 0u64;
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        let n = file
            .read(&mut chunk)
            .map_err(|_| ArtifactError::Unavailable)?;
        if n == 0 {
            break;
        }
        hasher.update(&chunk[..n]);
        size += n as u64;
        let room = max_bytes.saturating_sub(shown.len());
        shown.extend_from_slice(&chunk[..n.min(room)]);
    }
    let digest = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Ok((shown, size, digest))
}

#[cfg(test)]
mod tests;
