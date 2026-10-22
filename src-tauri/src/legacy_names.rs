//! Nazwy z Todo Track (przed zmianą nazwy na TukTuk Track, PLAN_BACKLOG.md, N1).
//!
//! Jedyne miejsce poza `legacy.rs`, w którym stare nazwy mogą występować: używają
//! ich migracja danych (`legacy.rs`) i usuwanie starych wpisów hooków
//! (`hooks_config.rs`, plik wspólny z programem `tuktuk-track-hook`). Kontrola
//! kompletności (`legacy::tests`) dopuszcza stare nazwy tylko w tych plikach.
#![allow(dead_code)]

/// Nazwa aplikacji (wpis `Run`, klucz `Uninstall`).
pub const NAME: &str = "Todo Track";
/// Identyfikator Tauri: katalogi danych i przedrostek usług poświadczeń.
pub const IDENTIFIER: &str = "pl.todotrack.desktop";
/// Host native messaging rozszerzenia przeglądarki.
pub const HOST_NAME: &str = "pl.todotrack.bridge";
/// Plik bazy w katalogu danych.
pub const DB_FILE: &str = "todo-track.sqlite";
/// Fragment polecenia wpisu hooka agenta (`todo-track-hook.exe <agent>`).
pub const HOOK: &str = "todo-track-hook";
/// Nazwa serwera MCP w rejestracji Claude Code i Codexa.
pub const MCP_SERVER: &str = "todo-track";
/// Program serwera MCP obok aplikacji.
pub const MCP_EXE: &str = "todo-track-mcp.exe";
/// Przedrostek kluczy `localStorage` (okna aplikacji).
pub const STORAGE_PREFIX: &str = "todo-track-";
