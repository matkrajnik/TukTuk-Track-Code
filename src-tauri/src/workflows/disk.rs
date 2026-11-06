//! Wolne miejsce na dysku danych aplikacji (PLAN_WDROZENIA.md, 7.2; E6).
//!
//! Przebieg zapisuje worktree, diffy i wyniki kroków w katalogu danych aplikacji;
//! przy prawie pełnym dysku start jest odrzucany z jasnym komunikatem, zamiast
//! kończyć krok w połowie zapisu.

/// Najmniej wolnego miejsca potrzebnego do startu przebiegu (512 MiB).
pub const MIN_FREE_BYTES: u64 = 512 * 1024 * 1024;

/// Komunikat odmowy startu przy braku miejsca.
pub fn low_space_message(free: u64) -> String {
    format!(
        "Za mało miejsca na dysku danych aplikacji ({} MiB wolnego, potrzeba co najmniej {} MiB). Zwolnij miejsce i uruchom przebieg ponownie.",
        free / (1024 * 1024),
        MIN_FREE_BYTES / (1024 * 1024)
    )
}

/// `Err`, gdy wiadomo, że miejsca jest za mało; nieznana ilość (`None`) nie blokuje.
pub fn ensure_free_space(free: Option<u64>) -> Result<(), String> {
    match free {
        Some(free) if free < MIN_FREE_BYTES => Err(low_space_message(free)),
        _ => Ok(()),
    }
}

/// Błąd zapisu oznaczający pełny dysk (Windows: 112 `ERROR_DISK_FULL`,
/// 39 `ERROR_HANDLE_DISK_FULL`; inne systemy: `ENOSPC`).
pub fn is_disk_full(error: &std::io::Error) -> bool {
    match error.raw_os_error() {
        Some(112 | 39) if cfg!(windows) => true,
        Some(28) if cfg!(not(windows)) => true,
        _ => error.kind() == std::io::ErrorKind::StorageFull,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_space_blocks_and_unknown_space_does_not() {
        assert!(ensure_free_space(Some(MIN_FREE_BYTES - 1)).is_err());
        assert!(ensure_free_space(Some(MIN_FREE_BYTES)).is_ok());
        assert!(ensure_free_space(None).is_ok());
        assert!(ensure_free_space(Some(10 * 1024 * 1024))
            .unwrap_err()
            .contains("10 MiB wolnego"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_disk_full_codes_are_recognised() {
        assert!(is_disk_full(&std::io::Error::from_raw_os_error(112)));
        assert!(is_disk_full(&std::io::Error::from_raw_os_error(39)));
        assert!(!is_disk_full(&std::io::Error::from_raw_os_error(5)));
    }
}
