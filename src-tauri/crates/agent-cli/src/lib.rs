//! Wspólna warstwa uruchamiania CLI agentów (PLAN_WDROZENIA.md, E4).
//!
//! Czyste elementy (`Spec`, parsery linii strumienia, klasyfikacja błędów)
//! są wspólne dla asystenta AI i modułu Workflows. `process` dodaje nadzorcę
//! procesu strumieniowego z limitami, kodem wyjścia i kończeniem całego drzewa.

mod failure;
mod lines;
mod parse;
mod process;
mod spec;
mod tree;

pub use failure::{classify, codex_event_text, parse_version, Failure};
pub use lines::{read_lines, LineItem};
pub use parse::{parse_claude_line_raw, parse_codex_line_raw, Parsed};
pub use process::{
    Options, ProcessEvent, RunningProcess, SpawnError, StderrTail, StreamingProcess,
    KILL_CONFIRM_TIMEOUT, MAX_LINE_BYTES, STDERR_TAIL_BYTES,
};
pub use spec::{command, command_with, Spec};
pub use tree::{kill_tree_by_pid, Job, KillResult};
