//! Rekonsyliacja po starcie i świadome odzyskanie przerwanego przebiegu
//! (PLAN_WDROZENIA.md, 5.2, 5.4, 7.3 i 8.3; etap E6).
//!
//! `reconcile_at_startup` działa przed `Runtime::start`, więc sterownik nie
//! uruchomi niczego, zanim:
//! 1. każdy proces z rejestru bez potwierdzonego końca zostanie sprawdzony po PID
//!    i czasie utworzenia (ponownie użytego PID nie zabijamy, ten sam żywy proces
//!    kończymy z drzewem, niepotwierdzony koniec zostaje `unknown`);
//! 2. rezerwacje przebiegów końcowych (poza `Interrupted`) zostaną zwolnione;
//! 3. przebieg w kolejce bez rezerwacji, a także przebieg w kolejce albo na granicy
//!    kroków z naruszonym worktree (np. podmieniony `.git`), skończy się
//!    `Failed`/`workspaceUnavailable` zamiast uruchomić agenta w ciemno;
//! 4. przebieg w kolejce albo na granicy kroków z pewnym worktree przejdzie w
//!    `Paused` (`Input::PauseAfterRestart`, decyzja po E7): po restarcie nic nie
//!    rusza samo, użytkownik wznawia przebieg w monitorze (`workflow_resume`).
//!
//! Przy starcie usuwane są też pliki `diff-*.tmp` po przerwanym zapisie artefaktu
//! (`remove_orphan_tmp`), a problemy trafiają do interfejsu przez `FAULT_EVENT`
//! (`startup_faults`).
//!
//! Krok bez wyniku sterownik po starcie oznacza `Interrupted` (`Input::Recover`),
//! nie uruchamiając go ponownie. `recover` (komenda `workflow_recover`) tylko
//! sprawdza procesy przerwanego przebiegu, zwalnia rezerwację po potwierdzonym
//! końcu i dopisuje ślad audytowy; graf nie jest wznawiany.

use std::path::Path;

use serde::Serialize;
use workflow_core::engine::{decide, Input, RunState};
use workflow_core::ports::{RunId, RunRepository, StoredEvent};
use workflow_core::{replay_stored, FailureCode, RunStatus, WorkflowGraph};

use super::processes::{open_processes, settle, ProcState, ProcessControl};
use super::runs::SqliteRunRepository;
use super::workspace::{reserved, workspace_problem, SharedDb};

const ERR_NOT_INTERRUPTED: &str = "Odzyskać można tylko przerwany przebieg.";

fn lock_error() -> String {
    "Blokada bazy jest zatruta.".to_owned()
}

fn load(repo: &SqliteRunRepository, run_id: RunId) -> Result<(WorkflowGraph, RunState), String> {
    let graph = repo.graph(run_id).map_err(|e| e.to_string())?;
    let events = repo.load_events(run_id, 0).map_err(|e| e.to_string())?;
    let state = replay_stored(&graph, &events).map_err(|e| e.to_string())?;
    Ok((graph, state))
}

/// Stosuje jedno wejście silnika i zapisuje zdarzenia (konflikt sekwencji to błąd).
pub(crate) fn apply_input(
    repo: &SqliteRunRepository,
    run_id: RunId,
    input: Input,
    now: i64,
) -> Result<Vec<StoredEvent>, String> {
    let (graph, state) = load(repo, run_id)?;
    let transition = decide(&graph, &state, input).map_err(|e| e.to_string())?;
    repo.append(run_id, state.next_sequence(), &transition.events, now)
        .map_err(|e| e.to_string())
}

/// Wynik rekonsyliacji przy starcie.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StartupReport {
    /// Procesy zakończone przez rekonsyliację (ten sam proces nadal działał).
    pub killed: usize,
    /// Procesy, których końca nie potwierdzono (rezerwacja zostaje).
    pub unknown: usize,
    /// Przebieg i PID każdego z tych procesów (komunikat dla użytkownika).
    pub unknown_processes: Vec<(RunId, u32)>,
    /// Rezerwacje przebiegów końcowych zwolnione przy starcie.
    pub released: usize,
    /// Przebiegi zakończone `workspaceUnavailable` zamiast uruchomienia agenta.
    pub failed_runs: Vec<RunId>,
    /// Przebiegi wstrzymane do ręcznego wznowienia (kolejka albo granica kroków).
    pub paused_runs: Vec<RunId>,
    /// Błędy, które nie zatrzymały rekonsyliacji (do logu).
    pub errors: Vec<String>,
}

/// Rekonsyliacja przed startem sterownika; patrz opis modułu.
pub fn reconcile_at_startup(
    db: &SharedDb,
    control: &dyn ProcessControl,
    emit: &dyn Fn(&StoredEvent),
    now: i64,
) -> Result<StartupReport, String> {
    let mut report = StartupReport::default();
    for process in open_processes(db, None)? {
        match settle(db, control, &process, now) {
            Ok(ProcState::Killed) => report.killed += 1,
            Ok(ProcState::Unknown) => {
                report.unknown += 1;
                report
                    .unknown_processes
                    .push((process.run_id, process.identity.pid));
            }
            Ok(_) => {}
            Err(e) => report
                .errors
                .push(format!("Proces kroku {}: {e}", process.execution_id)),
        }
    }
    report.released = release_terminal(db, now)?;
    let repo = SqliteRunRepository::new(db.clone());
    for run_id in repo.active_runs().map_err(|e| e.to_string())? {
        let (input, outcome) = match startup_action(db, &repo, run_id) {
            Ok(StartupAction::Leave) => continue,
            Ok(StartupAction::Pause) => (Input::PauseAfterRestart, &mut report.paused_runs),
            Ok(StartupAction::Fail(reason)) => (
                Input::Fault {
                    code: FailureCode::WorkspaceUnavailable,
                    message: reason,
                },
                &mut report.failed_runs,
            ),
            Err(e) => {
                report.errors.push(format!("Przebieg {run_id}: {e}"));
                continue;
            }
        };
        match apply_input(&repo, run_id, input, now) {
            Ok(events) => {
                events.iter().for_each(emit);
                outcome.push(run_id);
            }
            Err(e) => report.errors.push(format!("Przebieg {run_id}: {e}")),
        }
    }
    Ok(report)
}

/// Błędy rekonsyliacji dla interfejsu (`FAULT_EVENT`): przebieg (0, gdy błąd nie
/// dotyczy jednego przebiegu) i komunikat po polsku. Pusta lista, gdy nic nie wymaga
/// uwagi; zwykłe zwolnienie rezerwacji albo zakończenie procesu jej nie wymaga.
pub fn startup_faults(report: &StartupReport, tmp: &TmpReport) -> Vec<(RunId, String)> {
    let processes = report.unknown_processes.iter().map(|(run_id, pid)| {
        (
            *run_id,
            format!(
                "Po starcie nie potwierdzono końca procesu agenta (PID {pid}); katalog roboczy przebiegu pozostaje zablokowany."
            ),
        )
    });
    let failed = report.failed_runs.iter().map(|run_id| {
        (
            *run_id,
            "Przebieg nie został uruchomiony po starcie: katalog roboczy jest niedostępny."
                .to_owned(),
        )
    });
    let errors = report
        .errors
        .iter()
        .chain(tmp.errors.iter())
        .map(|e| (0, format!("Sprawdzenie Workflows po starcie: {e}")));
    processes.chain(failed).chain(errors).collect()
}

/// Wynik sprzątania plików tymczasowych artefaktów.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TmpReport {
    pub removed: usize,
    pub errors: Vec<String>,
}

/// Usuwa pliki `diff-*.tmp` w `workflows/runs/*` pozostawione przez przerwany
/// zapis artefaktu (plik docelowy powstaje tylko przez rename, więc gotowe diffy
/// zostają). Bez blokady bazy; wołać przed startem sterownika, gdy nic nie pisze.
/// Dowiązań nie śledzi: usuwa tylko zwykłe pliki.
pub fn remove_orphan_tmp(app_data: &Path) -> TmpReport {
    let mut report = TmpReport::default();
    let runs = super::artifacts::runs_dir(app_data);
    let Ok(entries) = std::fs::read_dir(&runs) else {
        return report;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_dir() {
            continue;
        }
        let Ok(files) = std::fs::read_dir(entry.path()) else {
            continue;
        };
        for file in files.flatten() {
            let name = file.file_name();
            let name = name.to_string_lossy();
            let is_tmp = name.starts_with("diff-") && name.ends_with(".tmp");
            if !is_tmp || !file.file_type().is_ok_and(|t| t.is_file()) {
                continue;
            }
            match std::fs::remove_file(file.path()) {
                Ok(()) => report.removed += 1,
                Err(e) => report
                    .errors
                    .push(format!("Nie usunięto pliku tymczasowego {name}: {e}")),
            }
        }
    }
    report
}

/// Zwalnia rezerwacje przebiegów w stanie końcowym innym niż `Interrupted`
/// (np. pozostawione przez wersję sprzed zwalniania w transakcji stanu, E5).
fn release_terminal(db: &SharedDb, now: i64) -> Result<usize, String> {
    let conn = db.lock().map_err(|_| lock_error())?;
    conn.execute(
        "UPDATE workflow_workspaces
         SET state = 'released', released_at = ?1, release_reason = 'terminal'
         WHERE state = 'reserved' AND run_id IN (
           SELECT id FROM workflow_runs
           WHERE status IN ('succeeded', 'failed', 'rejected', 'cancelled'))",
        [now],
    )
    .map_err(|e| e.to_string())
}

/// Co rekonsyliacja robi z aktywnym przebiegiem przy starcie.
enum StartupAction {
    /// Krok bez wyniku, zgoda, anulowanie albo już wstrzymany: zostawia sterownikowi.
    Leave,
    /// Przebieg w kolejce albo na granicy kroków z pewnym worktree: `Paused`.
    Pause,
    /// Brak rezerwacji albo worktree, którego Git nie potwierdza: `Failed`.
    Fail(String),
}

/// Przebieg, któremu sterownik dałby `Start`/`Continue`, nie rusza po restarcie
/// sam: z pewnym worktree czeka (`Pause`), z naruszonym kończy się (`Fail`).
fn startup_action(
    db: &SharedDb,
    repo: &SqliteRunRepository,
    run_id: RunId,
) -> Result<StartupAction, String> {
    let (_, state) = load(repo, run_id)?;
    let launches = match state.status {
        RunStatus::Queued => true,
        RunStatus::Running => state.unfinished().is_none(),
        _ => false,
    };
    if !launches {
        return Ok(StartupAction::Leave);
    }
    Ok(match workspace_problem(db, run_id)? {
        Some(reason) => StartupAction::Fail(reason),
        None => StartupAction::Pause,
    })
}

/// Wynik `workflow_recover`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoverOutcome {
    /// Koniec wszystkich procesów przebiegu jest potwierdzony.
    pub processes_confirmed: bool,
    /// Rezerwacja katalogu roboczego jest zwolniona (teraz albo wcześniej).
    pub workspace_released: bool,
    /// Nic nie było do zrobienia (ponowne wywołanie po udanym odzyskaniu).
    pub already_recovered: bool,
    /// PID procesów, których końca nie potwierdzono (rezerwacja zostaje).
    pub unconfirmed_pids: Vec<u32>,
}

/// Komunikat dla użytkownika o procesach bez potwierdzonego końca (z PID).
pub fn unconfirmed_note(pids: &[u32]) -> String {
    let list = pids
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "Nie potwierdzono końca procesu agenta (PID {list}); katalog roboczy pozostaje zablokowany. Zakończ proces ręcznie i spróbuj ponownie."
    )
}

fn run_status(db: &SharedDb, run_id: RunId) -> Result<String, String> {
    let conn = db.lock().map_err(|_| lock_error())?;
    conn.query_row(
        "SELECT status FROM workflow_runs WHERE id = ?1",
        [run_id],
        |r| r.get(0),
    )
    .map_err(|_| "Nie znaleziono przebiegu.".to_owned())
}

/// Odzyskanie przerwanego przebiegu: sprawdza (i w razie potrzeby kończy) jego
/// procesy, a po potwierdzonym końcu zwalnia rezerwację w tej samej transakcji co
/// zdarzenie `recovery`. Bez potwierdzenia rezerwacja zostaje. Nigdy nie woła
/// runnera i nie wznawia grafu; działa bez klucza modułu (odzyskiwanie dostępu).
pub fn recover(
    db: &SharedDb,
    control: &dyn ProcessControl,
    emit: &dyn Fn(&StoredEvent),
    run_id: RunId,
    now: i64,
) -> Result<RecoverOutcome, String> {
    if run_status(db, run_id)? != "interrupted" {
        return Err(ERR_NOT_INTERRUPTED.into());
    }
    let open = open_processes(db, Some(run_id))?;
    let has_reservation = reserved(db, run_id).map_err(|e| e.to_string())?.is_some();
    if open.is_empty() && !has_reservation {
        return Ok(RecoverOutcome {
            processes_confirmed: true,
            workspace_released: true,
            already_recovered: true,
            unconfirmed_pids: Vec::new(),
        });
    }
    let mut unconfirmed_pids = Vec::new();
    for process in &open {
        if !settle(db, control, process, now)?.is_settled() {
            unconfirmed_pids.push(process.identity.pid);
        }
    }
    let confirmed = unconfirmed_pids.is_empty();
    let release = confirmed && has_reservation;
    let note = (!confirmed).then(|| unconfirmed_note(&unconfirmed_pids));
    let repo = SqliteRunRepository::new(db.clone());
    let events = apply_input(
        &repo,
        run_id,
        Input::Reconcile {
            processes_confirmed: confirmed,
            workspace_released: release,
            note,
        },
        now,
    )?;
    events.iter().for_each(emit);
    Ok(RecoverOutcome {
        processes_confirmed: confirmed,
        workspace_released: release || !has_reservation,
        already_recovered: false,
        unconfirmed_pids,
    })
}

#[cfg(test)]
mod tests;
