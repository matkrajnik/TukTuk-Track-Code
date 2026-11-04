//! Okno innego programu na wierzch (np. Orki po `terminal switch`, który
//! przełącza kartę, ale nie wyciąga okna).
//!
//! Windows pozwala ustawić na pierwszym planie cudze okno tylko procesowi, który
//! ma teraz fokus; tak jest zaraz po kliknięciu w TukTuk Track. Szukamy okien
//! najwyższego poziomu (od góry stosu okien), które należą do procesu o podanej
//! nazwie pliku, są widoczne, mają tytuł i nie należą do innego okna (bez okien
//! podręcznych i dymków); pierwsze takie okno przywracamy, gdy jest
//! zminimalizowane, i ustawiamy na wierzchu.

/// Okno najwyższego poziomu widziane przez `EnumWindows`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub handle: isize,
    /// Nazwa pliku procesu (np. `Orca.exe`).
    pub exe: String,
    pub visible: bool,
    /// Okno należy do innego okna (podręczne, dialog).
    pub owned: bool,
    pub titled: bool,
}

/// Główne okno programu: pierwsze (najwyżej w stosie) widoczne, nienależące do
/// innego okna, z tytułem, którego proces ma nazwę `exe` (bez wielkości liter).
pub fn pick(candidates: &[Candidate], exe: &str) -> Option<isize> {
    candidates
        .iter()
        .find(|c| c.visible && !c.owned && c.titled && c.exe.eq_ignore_ascii_case(exe))
        .map(|c| c.handle)
}

/// Wyciąga główne okno programu `exe` na wierzch; `false`, gdy go nie ma albo
/// Windows odmówił (wtedy wołający ma inną drogę, np. `orca open`).
#[cfg(windows)]
pub fn bring_to_front(exe: &str) -> bool {
    let candidates = sys::windows();
    pick(&candidates, exe).is_some_and(sys::raise)
}

#[cfg(not(windows))]
pub fn bring_to_front(_exe: &str) -> bool {
    false
}

#[cfg(windows)]
mod sys {
    use super::Candidate;
    use std::ffi::c_void;

    type Handle = *mut c_void;
    const GW_OWNER: u32 = 4;
    const SW_RESTORE: i32 = 9;
    const QUERY_LIMITED_INFORMATION: u32 = 0x1000;

    #[link(name = "user32")]
    extern "system" {
        fn EnumWindows(callback: extern "system" fn(Handle, isize) -> i32, param: isize) -> i32;
        fn IsWindowVisible(hwnd: Handle) -> i32;
        fn GetWindow(hwnd: Handle, cmd: u32) -> Handle;
        fn GetWindowTextLengthW(hwnd: Handle) -> i32;
        fn GetWindowThreadProcessId(hwnd: Handle, pid: *mut u32) -> u32;
        fn IsIconic(hwnd: Handle) -> i32;
        fn ShowWindow(hwnd: Handle, cmd: i32) -> i32;
        fn SetForegroundWindow(hwnd: Handle) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
        fn QueryFullProcessImageNameW(
            process: Handle,
            flags: u32,
            name: *mut u16,
            size: *mut u32,
        ) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
    }

    /// Nazwa pliku procesu (bez ścieżki); pusta, gdy nie da się jej odczytać.
    fn exe_of(pid: u32) -> String {
        // SAFETY: uchwyt procesu z OpenProcess sprawdzamy i zamykamy; bufor ma
        // podany rozmiar, a odczytana długość jest przycinana do bufora.
        unsafe {
            let process = OpenProcess(QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return String::new();
            }
            let mut buf = [0u16; 1024];
            let mut size = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(process, 0, buf.as_mut_ptr(), &mut size) != 0;
            CloseHandle(process);
            if !ok {
                return String::new();
            }
            let path = String::from_utf16_lossy(&buf[..(size as usize).min(buf.len())]);
            path.rsplit('\\').next().unwrap_or_default().to_string()
        }
    }

    extern "system" fn collect(hwnd: Handle, param: isize) -> i32 {
        // SAFETY: `param` to wskaźnik na `Vec<Candidate>` z `windows`, ważny przez
        // cały czas `EnumWindows` (wywołanie synchroniczne); `hwnd` pochodzi od systemu.
        unsafe {
            let list = &mut *(param as *mut Vec<Candidate>);
            let visible = IsWindowVisible(hwnd) != 0;
            // Tylko widoczne okna: nazwę procesu czytamy dla nich, nie dla setek ukrytych.
            if visible {
                let mut pid = 0u32;
                GetWindowThreadProcessId(hwnd, &mut pid);
                list.push(Candidate {
                    handle: hwnd as isize,
                    exe: exe_of(pid),
                    visible,
                    owned: !GetWindow(hwnd, GW_OWNER).is_null(),
                    titled: GetWindowTextLengthW(hwnd) > 0,
                });
            }
        }
        1
    }

    /// Okna najwyższego poziomu od góry stosu.
    pub fn windows() -> Vec<Candidate> {
        let mut list: Vec<Candidate> = Vec::new();
        // SAFETY: wskaźnik na `list` żyje dłużej niż synchroniczne `EnumWindows`.
        unsafe {
            EnumWindows(collect, &mut list as *mut Vec<Candidate> as isize);
        }
        list
    }

    /// Przywraca zminimalizowane okno i ustawia je na wierzchu.
    pub fn raise(handle: isize) -> bool {
        let hwnd = handle as Handle;
        // SAFETY: uchwyt okna z `EnumWindows`; nieaktualny uchwyt daje tylko błąd wywołania.
        unsafe {
            if IsIconic(hwnd) != 0 {
                ShowWindow(hwnd, SW_RESTORE);
            }
            SetForegroundWindow(hwnd) != 0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(handle: isize, exe: &str) -> Candidate {
        Candidate {
            handle,
            exe: exe.into(),
            visible: true,
            owned: false,
            titled: true,
        }
    }

    /// Prawdziwe `EnumWindows` na pulpicie użytkownika (bez wyciągania okien):
    /// `cargo test -- --ignored window_focus`.
    #[cfg(windows)]
    #[test]
    #[ignore = "wymaga pulpitu Windows"]
    fn lists_visible_windows_with_process_names() {
        let list = sys::windows();
        assert!(!list.is_empty());
        assert!(list
            .iter()
            .any(|c| c.exe.to_ascii_lowercase().ends_with(".exe")));
        let main: Vec<String> = list
            .iter()
            .filter(|c| !c.owned && c.titled)
            .map(|c| c.exe.clone())
            .collect();
        println!("okna główne: {main:?}");
        println!("Orca: {:?}", pick(&list, "Orca.exe"));
    }

    #[test]
    fn picks_the_topmost_main_window_of_the_program() {
        let list = vec![
            window(1, "Code.exe"),
            // Dymek i okno podręczne Orki nie są jej głównym oknem.
            Candidate {
                owned: true,
                ..window(2, "Orca.exe")
            },
            Candidate {
                titled: false,
                ..window(3, "Orca.exe")
            },
            window(4, "orca.EXE"),
            window(5, "Orca.exe"),
        ];
        assert_eq!(pick(&list, "Orca.exe"), Some(4));
        assert_eq!(pick(&list, "Codex.exe"), None);
        let hidden = vec![Candidate {
            visible: false,
            ..window(6, "Orca.exe")
        }];
        assert_eq!(pick(&hidden, "Orca.exe"), None);
    }
}
