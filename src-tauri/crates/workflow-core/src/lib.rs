//! Domena modułu Workflows (docs/workflows/PLAN_WDROZENIA.md, rozdziały 5–7).
//!
//! Crate nie zależy od Tauri, SQLite ani procesów systemu. Zawiera typy grafu,
//! walidację definicji i gotowy szablon procesu. Silnik wykonania (E3) dołączy
//! tu planowanie przejść i porty dla hosta.

pub mod model;
pub mod template;
pub mod validation;

pub use model::{
    AccessProfile, AgentConfig, AgentRole, ApprovalConfig, CompareOp, ConditionConfig, Edge,
    EndConfig, EndOutcome, FieldRef, FieldType, Node, NodeKind, Port, Position, Provider,
    WorkflowGraph, SCHEMA_VERSION,
};
pub use validation::{validate, Issue, IssueCode};
