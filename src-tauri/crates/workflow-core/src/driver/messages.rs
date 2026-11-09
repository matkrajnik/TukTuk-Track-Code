//! Zdarzenia runnera (komunikaty i narzędzia) zbierane w partie z limitami.

use serde_json::{json, Map, Value};

use crate::engine::RunEvent;
use crate::model::AgentRole;
use crate::ports::RunnerEvent;

/// Najwięcej zdarzeń runnera zapisanych dla jednego kroku; kolejne to jeden znacznik.
pub const MAX_RUNNER_EVENTS_PER_EXECUTION: usize = 500;
/// Orkiestrator z subagentami emituje wielokrotnie więcej zdarzeń (E8, D7).
pub const MAX_RUNNER_EVENTS_PER_ORCHESTRATOR: usize = 3000;
/// Największy zapisany ładunek jednego zdarzenia runnera w bajtach.
pub const MAX_RUNNER_PAYLOAD_BYTES: usize = 8 * 1024;
/// Po tylu zdarzeniach runnera zapisujemy partię bez czekania na tyknięcie.
pub const RUNNER_EVENT_BATCH: usize = 32;

/// Limit zapisanych zdarzeń runnera dla roli kroku.
pub(super) fn event_budget(role: AgentRole) -> usize {
    match role {
        AgentRole::Orchestrator => MAX_RUNNER_EVENTS_PER_ORCHESTRATOR,
        AgentRole::Analysis | AgentRole::Implementation | AgentRole::Fix | AgentRole::Review => {
            MAX_RUNNER_EVENTS_PER_EXECUTION
        }
    }
}

/// Zbiera zdarzenia runnera w partie, z limitem liczby i rozmiaru (znacznik `truncated`).
pub(super) struct MessageBuffer {
    execution_id: String,
    emitted: usize,
    limit: usize,
    pub(super) pending: Vec<RunEvent>,
}

impl MessageBuffer {
    pub(super) fn new(execution_id: &str, limit: usize) -> Self {
        Self {
            execution_id: execution_id.to_owned(),
            emitted: 0,
            limit,
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
        if self.emitted > self.limit + 1 {
            return;
        }
        let execution_id = self.execution_id.clone();
        if self.emitted == self.limit + 1 {
            self.pending.push(RunEvent::Message {
                execution_id,
                payload: json!({"truncated": true, "reason": "limit zdarzeń kroku"}),
            });
            return;
        }
        self.pending.push(match event {
            RunnerEvent::Message { text, parent } => {
                let mut payload = Map::new();
                payload.insert("text".into(), json!(text));
                put_opt(&mut payload, "parentToolUseId", parent.map(Value::from));
                RunEvent::Message {
                    execution_id,
                    payload: Self::payload(Value::Object(payload)),
                }
            }
            RunnerEvent::ToolStarted {
                name,
                id,
                parent,
                args,
            } => {
                let mut payload = tool_payload("started", name, id, parent);
                put_opt(&mut payload, "args", args);
                RunEvent::Tool {
                    execution_id,
                    payload: Self::payload(Value::Object(payload)),
                }
            }
            RunnerEvent::ToolFinished {
                name,
                id,
                parent,
                ok,
            } => {
                let mut payload = tool_payload("finished", name, id, parent);
                payload.insert("ok".into(), json!(ok));
                RunEvent::Tool {
                    execution_id,
                    payload: Self::payload(Value::Object(payload)),
                }
            }
        });
    }
}

/// Dopisuje pole tylko wtedy, gdy ma wartość (stary kształt zostaje bez zmian).
fn put_opt(payload: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    if let Some(value) = value {
        payload.insert(key.into(), value);
    }
}

fn tool_payload(
    phase: &str,
    name: String,
    id: Option<String>,
    parent: Option<String>,
) -> Map<String, Value> {
    let mut payload = Map::new();
    payload.insert("phase".into(), json!(phase));
    payload.insert("name".into(), json!(name));
    put_opt(&mut payload, "toolUseId", id.map(Value::from));
    put_opt(&mut payload, "parentToolUseId", parent.map(Value::from));
    payload
}
