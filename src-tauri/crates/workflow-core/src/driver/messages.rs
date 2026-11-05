//! Zdarzenia runnera (komunikaty i narzędzia) zbierane w partie z limitami.

use serde_json::{json, Value};

use crate::engine::RunEvent;
use crate::ports::RunnerEvent;

/// Najwięcej zdarzeń runnera zapisanych dla jednego kroku; kolejne to jeden znacznik.
pub const MAX_RUNNER_EVENTS_PER_EXECUTION: usize = 500;
/// Największy zapisany ładunek jednego zdarzenia runnera w bajtach.
pub const MAX_RUNNER_PAYLOAD_BYTES: usize = 8 * 1024;
/// Po tylu zdarzeniach runnera zapisujemy partię bez czekania na tyknięcie.
pub const RUNNER_EVENT_BATCH: usize = 32;

/// Zbiera zdarzenia runnera w partie, z limitem liczby i rozmiaru (znacznik `truncated`).
pub(super) struct MessageBuffer {
    execution_id: String,
    emitted: usize,
    pub(super) pending: Vec<RunEvent>,
}

impl MessageBuffer {
    pub(super) fn new(execution_id: &str) -> Self {
        Self {
            execution_id: execution_id.to_owned(),
            emitted: 0,
            pending: Vec::new(),
        }
    }

    fn payload(value: Value) -> Value {
        let size = value.to_string().len();
        if size > MAX_RUNNER_PAYLOAD_BYTES {
            json!({"truncated": true, "originalBytes": size})
        } else {
            value
        }
    }

    pub(super) fn push(&mut self, event: RunnerEvent) {
        self.emitted += 1;
        if self.emitted > MAX_RUNNER_EVENTS_PER_EXECUTION + 1 {
            return;
        }
        let execution_id = self.execution_id.clone();
        if self.emitted == MAX_RUNNER_EVENTS_PER_EXECUTION + 1 {
            self.pending.push(RunEvent::Message {
                execution_id,
                payload: json!({"truncated": true, "reason": "limit zdarzeń kroku"}),
            });
            return;
        }
        self.pending.push(match event {
            RunnerEvent::Message { text } => RunEvent::Message {
                execution_id,
                payload: Self::payload(json!({"text": text})),
            },
            RunnerEvent::ToolStarted { name } => RunEvent::Tool {
                execution_id,
                payload: Self::payload(json!({"phase": "started", "name": name})),
            },
            RunnerEvent::ToolFinished { name, ok } => RunEvent::Tool {
                execution_id,
                payload: Self::payload(json!({"phase": "finished", "name": name, "ok": ok})),
            },
        });
    }
}
