//! Wolne miejsce na dysku (kontrola przed startem przebiegu Workflows).

use std::path::Path;

/// Liczba bajtów dostępnych dla bieżącego użytkownika na dysku ze ścieżką `path`.
/// `None`, gdy nie da się jej ustalić (wtedy wołający nie blokuje pracy).
#[cfg(windows)]
pub fn available_bytes(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut available = 0_u64;
    // SAFETY: `wide` jest zakończony zerem, a wskaźnik wyjściowy wskazuje na żywą zmienną;
    // pozostałe wskaźniki mogą być puste.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    (ok != 0).then_some(available)
}

#[cfg(not(windows))]
pub fn available_bytes(_path: &Path) -> Option<u64> {
    None
}
