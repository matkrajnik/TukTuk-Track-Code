//! Drobne wspólne funkcje sterownika i pomocników testów.

use std::sync::{Mutex, MutexGuard};

use crate::engine::{RunState, SequencedEvent};
use crate::model::WorkflowGraph;
use crate::ports::{RepoError, StoredEvent};

/// Blokada odporna na zatrucie: stan chroniony tu jest zawsze spójny.
pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Odtwarza stan z zapisanych zdarzeń.
pub fn replay_stored(graph: &WorkflowGraph, events: &[StoredEvent]) -> Result<RunState, RepoError> {
    let sequenced = events
        .iter()
        .map(|s| {
            s.to_run_event().map(|event| SequencedEvent {
                sequence: s.sequence,
                event,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    RunState::replay(graph, &sequenced).map_err(|e| RepoError::Storage(e.to_string()))
}
