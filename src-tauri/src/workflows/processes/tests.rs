//! Rejestr procesów i rozstrzyganie wpisów na atrapie procesów systemu.

use agent_cli::{KillResult, Probe, ProcessIdentity};

use super::fake::FakeProcesses;
use super::*;
use crate::workflows::test_db::TestDb;

fn identity(pid: u32) -> ProcessIdentity {
    ProcessIdentity {
        pid,
        created_ft: 1_000 + u64::from(pid),
        job_attached: true,
    }
}

#[test]
fn ledger_records_spawn_and_end() {
    let t = TestDb::new();
    let run = t.create("r1");
    let ledger = SqliteLedger::new(t.db.clone(), || 5);
    ledger.spawned(run, "e1", &identity(42)).unwrap();
    assert_eq!(t.process_state("e1"), "running");
    let open = open_processes(&t.db, Some(run)).unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].identity, identity(42));
    ledger.ended("e1", ProcState::Exited).unwrap();
    assert_eq!(t.process_state("e1"), "exited");
    assert!(open_processes(&t.db, None).unwrap().is_empty());
}

#[test]
fn ledger_without_a_run_row_is_an_error_not_a_silent_skip() {
    let t = TestDb::new();
    let ledger = SqliteLedger::new(t.db.clone(), || 5);
    assert!(ledger.spawned(999, "e1", &identity(1)).is_err());
}

#[test]
fn settle_maps_probe_and_kill_results() {
    let t = TestDb::new();
    let run = t.create("r1");
    let ledger = SqliteLedger::new(t.db.clone(), || 5);
    for (exec, pid) in [("gone", 1), ("reused", 2), ("alive", 3), ("stuck", 4)] {
        ledger.spawned(run, exec, &identity(pid)).unwrap();
    }
    let fake = FakeProcesses::default();
    fake.probes.lock().unwrap().insert(2, Probe::Reused);
    fake.probes.lock().unwrap().insert(3, Probe::SameAlive);
    fake.probes.lock().unwrap().insert(4, Probe::SameAlive);
    fake.kills
        .lock()
        .unwrap()
        .insert(4, KillResult::Unconfirmed);
    let states: Vec<ProcState> = open_processes(&t.db, None)
        .unwrap()
        .iter()
        .map(|p| settle(&t.db, &fake, p, 9).unwrap())
        .collect();
    assert_eq!(
        states,
        // Kolejność: alive, gone, reused, stuck (po execution_id).
        [
            ProcState::Killed,
            ProcState::Exited,
            ProcState::Exited,
            ProcState::Unknown
        ]
    );
    assert_eq!(
        fake.killed(),
        [3, 4],
        "ponownie użyty PID nie jest zabijany"
    );
    let open = open_processes(&t.db, None).unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].state, ProcState::Unknown);
}
