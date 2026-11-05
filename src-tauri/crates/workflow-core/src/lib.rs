//! Domena modułu Workflows (docs/workflows/PLAN_WDROZENIA.md, rozdziały 5–7).
//!
//! Crate nie zależy od Tauri, SQLite ani procesów systemu. Zawiera typy grafu,
//! walidację definicji, gotowy szablon procesu oraz silnik wykonania (E3):
//! czystą maszynę stanów (`engine`), porty dla hosta (`ports`) i sterownik na
//! wątkach std (`driver`). Stan przebiegu odtwarza się wyłącznie ze zdarzeń.

pub mod driver;
pub mod engine;
pub mod model;
pub mod output;
pub mod ports;
pub mod run;
pub mod samples;
mod support;
pub mod template;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
pub mod validation;

pub use model::{
    AccessProfile, AgentConfig, AgentRole, ApprovalConfig, CompareOp, ConditionConfig, Edge,
    EndConfig, EndOutcome, FieldRef, FieldType, Node, NodeKind, Port, Position, Provider,
    WorkflowGraph, SCHEMA_VERSION,
};
pub use run::{consistent, ApprovalStatus, Decision, ExecutionStatus, FailureCode, RunStatus};
pub use validation::{validate, Issue, IssueCode};
