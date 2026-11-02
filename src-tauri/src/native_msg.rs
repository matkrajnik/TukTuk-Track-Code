//! Ramka native messaging i nazwa potoku (PLAN_1.7.md, W4).
//!
//! Wspólny plik aplikacji (`mod native_msg`) i programu `tuktuk-track-bridge`
//! (`src/bin/bridge.rs`, przez `#[path]`), żeby oba końce czytały i pisały tę
//! samą ramkę: 4 bajty długości (little-endian) i tyle bajtów JSON w UTF-8.
//! Tak samo wygląda ramka między przeglądarką a hostem (stdin/stdout) i między
//! hostem a aplikacją (nazwany potok). Wiadomość większa niż [`MAX`] (1 MB) jest
//! odrzucana; jej treść jest pomijana, więc kolejna ramka czyta się poprawnie.
#![allow(dead_code)]

use std::io::{self, Read, Write};

/// Największa wiadomość w jedną i drugą stronę (limit Chrome dla odpowiedzi hosta).
pub const MAX: usize = 1024 * 1024;

/// Nazwa hosta native messaging (klucz rejestru i pole `name` manifestu hosta).
pub const HOST_NAME: &str = "pl.tuktuktrack.bridge";

/// Błąd odczytu ramki.
#[derive(Debug)]
pub enum FrameError {
    /// Długość ponad [`MAX`]; treść została pominięta.
    TooLarge(usize),
    /// Strumień urwał się w środku ramki.
    Truncated,
    Io(io::Error),
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameError::TooLarge(n) => write!(f, "wiadomość ma {n} bajtów, limit to {MAX}"),
            FrameError::Truncated => write!(f, "niepełna wiadomość"),
            FrameError::Io(e) => write!(f, "{e}"),
        }
    }
}

/// Ramka: długość little-endian i treść. Błąd dla treści ponad [`MAX`].
pub fn encode(body: &[u8]) -> Result<Vec<u8>, FrameError> {
    if body.len() > MAX {
        return Err(FrameError::TooLarge(body.len()));
    }
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(body);
    Ok(out)
}

/// Zapisuje jedną ramkę i opróżnia bufor.
pub fn write_frame<W: Write>(w: &mut W, body: &[u8]) -> Result<(), FrameError> {
    let frame = encode(body)?;
    w.write_all(&frame).map_err(FrameError::Io)?;
    w.flush().map_err(FrameError::Io)
}

/// Czyta jedną ramkę. `Ok(None)` = koniec strumienia przed pierwszym bajtem
/// (przeglądarka zamknęła połączenie).
pub fn read_frame<R: Read>(r: &mut R) -> Result<Option<Vec<u8>>, FrameError> {
    let mut len = [0u8; 4];
    let mut got = 0;
    while got < 4 {
        match r.read(&mut len[got..]) {
            Ok(0) if got == 0 => return Ok(None),
            Ok(0) => return Err(FrameError::Truncated),
            Ok(n) => got += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(FrameError::Io(e)),
        }
    }
    let size = u32::from_le_bytes(len) as usize;
    if size > MAX {
        // Pomijamy treść, żeby następna ramka zaczynała się we właściwym miejscu.
        let skipped =
            io::copy(&mut r.take(size as u64), &mut io::sink()).map_err(FrameError::Io)?;
        if (skipped as usize) < size {
            return Err(FrameError::Truncated);
        }
        return Err(FrameError::TooLarge(size));
    }
    let mut body = vec![0u8; size];
    r.read_exact(&mut body).map_err(|e| match e.kind() {
        io::ErrorKind::UnexpectedEof => FrameError::Truncated,
        _ => FrameError::Io(e),
    })?;
    Ok(Some(body))
}

/// Najdłuższe oczekiwanie klienta na odpowiedź aplikacji (zablokowana aplikacja
/// nie zawiesza `tuktuk-track-ctl`, `tuktuk-track-mcp` ani hosta przeglądarki).
pub const REPLY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Klient potoku: wysyła ramkę do aplikacji i czeka na odpowiedź najwyżej
/// [`REPLY_TIMEOUT`]. Zajęty potok (inna wiadomość w toku) próbujemy otworzyć
/// jeszcze kilka razy. Potok, którego serwer działa na innym koncie, to błąd.
pub fn exchange(pipe: &str, body: &[u8]) -> Result<Vec<u8>, FrameError> {
    exchange_with(pipe, body, REPLY_TIMEOUT)
}

pub fn exchange_with(
    pipe: &str,
    body: &[u8],
    timeout: std::time::Duration,
) -> Result<Vec<u8>, FrameError> {
    const PIPE_BUSY: i32 = 231;
    let mut attempt = 0;
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true);
    // Serwer potoku może tylko sprawdzić tożsamość klienta, nie działać w jego
    // imieniu (obcy proces, który zajął nazwę potoku, nie dostaje tokenu).
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const SECURITY_IDENTIFICATION: u32 = 1 << 16;
        options.security_qos_flags(SECURITY_IDENTIFICATION);
    }
    let mut file = loop {
        match options.open(pipe) {
            Ok(f) => break f,
            Err(e) if e.raw_os_error() == Some(PIPE_BUSY) && attempt < 20 => {
                attempt += 1;
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => return Err(FrameError::Io(e)),
        }
    };
    #[cfg(windows)]
    win::check_server(&file).map_err(FrameError::Io)?;
    write_frame(&mut file, body)?;
    #[cfg(windows)]
    let reply = read_frame(&mut win::TimedReader::new(
        &mut file,
        std::time::Instant::now() + timeout,
    ));
    #[cfg(not(windows))]
    let reply = {
        let _ = timeout;
        read_frame(&mut file)
    };
    reply?.ok_or(FrameError::Truncated)
}

/// Zabezpieczenia nazwanych potoków aplikacji (Win32 bez dodatkowych zależności):
/// DACL serwera tylko dla bieżącego użytkownika i SYSTEM, sprawdzenie po stronie
/// klienta, czyje jest konto procesu serwera, i odczyt z limitem czasu.
#[cfg(windows)]
pub mod win {
    use std::ffi::c_void;
    use std::fs::File;
    use std::io::{self, Read};
    use std::os::windows::io::AsRawHandle;
    use std::time::{Duration, Instant};

    type Handle = *mut c_void;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> Handle;
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
        fn CloseHandle(handle: Handle) -> i32;
        fn LocalFree(mem: *mut c_void) -> *mut c_void;
        fn GetNamedPipeServerProcessId(pipe: Handle, pid: *mut u32) -> i32;
        fn PeekNamedPipe(
            pipe: Handle,
            buffer: *mut c_void,
            size: u32,
            read: *mut u32,
            available: *mut u32,
            left: *mut u32,
        ) -> i32;
    }
    #[link(name = "advapi32")]
    extern "system" {
        fn OpenProcessToken(process: Handle, access: u32, token: *mut Handle) -> i32;
        fn GetTokenInformation(
            token: Handle,
            class: u32,
            info: *mut c_void,
            len: u32,
            needed: *mut u32,
        ) -> i32;
        fn GetLengthSid(sid: *const c_void) -> u32;
        fn ConvertSidToStringSidW(sid: *const c_void, out: *mut *mut u16) -> i32;
        fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl: *const u16,
            revision: u32,
            descriptor: *mut *mut c_void,
            size: *mut u32,
        ) -> i32;
        fn GetSecurityInfo(
            handle: Handle,
            object_type: u32,
            info: u32,
            owner: *mut *mut c_void,
            group: *mut *mut c_void,
            dacl: *mut *mut c_void,
            sacl: *mut *mut c_void,
            descriptor: *mut *mut c_void,
        ) -> u32;
    }
    const SE_KERNEL_OBJECT: u32 = 6;
    const OWNER_SECURITY_INFORMATION: u32 = 1;
    const ERROR_ACCESS_DENIED: i32 = 5;
    const TOKEN_QUERY: u32 = 0x0008;
    const TOKEN_USER_CLASS: u32 = 1;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const SDDL_REVISION_1: u32 = 1;
    const ERROR_BROKEN_PIPE: i32 = 109;

    /// Uchwyt zamykany przy wyjściu z zakresu.
    struct Owned(Handle);
    impl Drop for Owned {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }

    /// SID (binarnie) konta, na którym działa proces. Binarne SID-y są
    /// kanoniczne, więc równość bajtów to równość kont.
    fn process_user(process: Handle) -> io::Result<Vec<u8>> {
        let mut token = std::ptr::null_mut();
        if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let token = Owned(token);
        let mut needed = 0u32;
        unsafe {
            GetTokenInformation(
                token.0,
                TOKEN_USER_CLASS,
                std::ptr::null_mut(),
                0,
                &mut needed,
            )
        };
        if needed == 0 {
            return Err(io::Error::last_os_error());
        }
        // TOKEN_USER zaczyna się wskaźnikiem na SID, więc bufor wyrównany do 8.
        let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
        let ok = unsafe {
            GetTokenInformation(
                token.0,
                TOKEN_USER_CLASS,
                buf.as_mut_ptr().cast(),
                (buf.len() * 8) as u32,
                &mut needed,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        let sid = unsafe { *(buf.as_ptr() as *const *const u8) };
        let len = unsafe { GetLengthSid(sid.cast()) } as usize;
        Ok(unsafe { std::slice::from_raw_parts(sid, len) }.to_vec())
    }

    /// SID bieżącego użytkownika.
    pub fn current_user() -> io::Result<Vec<u8>> {
        process_user(unsafe { GetCurrentProcess() })
    }

    /// SID w postaci tekstowej (`S-1-5-21-…`).
    fn sid_string(sid: &[u8]) -> io::Result<String> {
        let mut out: *mut u16 = std::ptr::null_mut();
        if unsafe { ConvertSidToStringSidW(sid.as_ptr().cast(), &mut out) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let len = (0..).take_while(|&i| unsafe { *out.add(i) } != 0).count();
        let text = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(out, len) });
        unsafe { LocalFree(out.cast()) };
        Ok(text)
    }

    /// Opis zabezpieczeń potoku: pełny dostęp tylko dla bieżącego użytkownika
    /// i SYSTEM, bez dziedziczenia. Inne konta nie połączą się z potokiem ani
    /// nie utworzą kolejnej jego instancji.
    pub struct UserOnly(*mut c_void);

    // Opis zabezpieczeń jest tylko czytany przez `CreateNamedPipeW`.
    unsafe impl Send for UserOnly {}

    impl UserOnly {
        pub fn new() -> io::Result<Self> {
            // Właściciel jawnie = użytkownik (proces z podwyższonymi uprawnieniami
            // domyślnie nadałby grupę Administratorzy); klient sprawdza go w
            // `check_server`, a cudzego SID-u jako właściciela inne konto nie ustawi.
            let sid = sid_string(&current_user()?)?;
            let sddl = format!("O:{sid}D:P(A;;GA;;;{sid})(A;;GA;;;SY)");
            let wide: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
            let mut descriptor = std::ptr::null_mut();
            let ok = unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    wide.as_ptr(),
                    SDDL_REVISION_1,
                    &mut descriptor,
                    std::ptr::null_mut(),
                )
            };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(Self(descriptor))
        }

        /// `SECURITY_ATTRIBUTES` dla `CreateNamedPipeW`; ważne, póki żyje `self`.
        pub fn attributes(&self) -> SecurityAttributes {
            SecurityAttributes {
                length: std::mem::size_of::<SecurityAttributes>() as u32,
                descriptor: self.0,
                inherit: 0,
            }
        }
    }

    impl Drop for UserOnly {
        fn drop(&mut self) {
            unsafe { LocalFree(self.0) };
        }
    }

    #[repr(C)]
    pub struct SecurityAttributes {
        length: u32,
        descriptor: *mut c_void,
        inherit: i32,
    }

    /// DACL obiektu (np. potoku) w zapisie SDDL, do testów.
    #[cfg(test)]
    pub fn dacl_sddl(file: &File) -> io::Result<String> {
        #[link(name = "advapi32")]
        extern "system" {
            fn ConvertSecurityDescriptorToStringSecurityDescriptorW(
                descriptor: *mut c_void,
                revision: u32,
                info: u32,
                out: *mut *mut u16,
                len: *mut u32,
            ) -> i32;
        }
        const DACL_SECURITY_INFORMATION: u32 = 4;
        let mut descriptor = std::ptr::null_mut();
        let status = unsafe {
            GetSecurityInfo(
                file.as_raw_handle() as Handle,
                SE_KERNEL_OBJECT,
                DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut descriptor,
            )
        };
        if status != 0 {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        let mut out: *mut u16 = std::ptr::null_mut();
        let ok = unsafe {
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                descriptor,
                SDDL_REVISION_1,
                DACL_SECURITY_INFORMATION,
                &mut out,
                std::ptr::null_mut(),
            )
        };
        unsafe { LocalFree(descriptor) };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        let len = (0..).take_while(|&i| unsafe { *out.add(i) } != 0).count();
        let text = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(out, len) });
        unsafe { LocalFree(out.cast()) };
        Ok(text)
    }

    /// SID bieżącego użytkownika jako tekst, do testów.
    #[cfg(test)]
    pub fn current_user_string() -> io::Result<String> {
        sid_string(&current_user()?)
    }

    /// Serwer potoku musi działać na koncie bieżącego użytkownika. Inaczej nazwę
    /// zajął obcy proces (np. inne konto na tej samej maszynie) i nie wysyłamy
    /// mu niczego.
    pub fn check_server(pipe: &File) -> io::Result<()> {
        let mut pid = 0u32;
        if unsafe { GetNamedPipeServerProcessId(pipe.as_raw_handle() as Handle, &mut pid) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process.is_null() {
            return Err(io::Error::last_os_error());
        }
        let process = Owned(process);
        let owner = match process_user(process.0) {
            Ok(user) => user,
            // Tokenu aplikacji uruchomionej jako administrator nie da się odczytać
            // bez podwyższonych uprawnień; wtedy decyduje właściciel potoku.
            Err(e) if e.raw_os_error() == Some(ERROR_ACCESS_DENIED) => pipe_owner(pipe)?,
            Err(e) => return Err(e),
        };
        if owner != current_user()? {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "potok aplikacji należy do innego użytkownika",
            ));
        }
        Ok(())
    }

    /// SID właściciela obiektu potoku.
    fn pipe_owner(pipe: &File) -> io::Result<Vec<u8>> {
        let mut owner = std::ptr::null_mut();
        let mut descriptor = std::ptr::null_mut();
        let status = unsafe {
            GetSecurityInfo(
                pipe.as_raw_handle() as Handle,
                SE_KERNEL_OBJECT,
                OWNER_SECURITY_INFORMATION,
                &mut owner,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut descriptor,
            )
        };
        if status != 0 {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        // `owner` wskazuje do wnętrza `descriptor`, więc kopia przed zwolnieniem.
        let sid = if owner.is_null() {
            Vec::new()
        } else {
            let len = unsafe { GetLengthSid(owner) } as usize;
            unsafe { std::slice::from_raw_parts(owner as *const u8, len) }.to_vec()
        };
        unsafe { LocalFree(descriptor) };
        Ok(sid)
    }

    /// Właściciel potoku do testów (ścieżka zapasowa `check_server`).
    #[cfg(test)]
    pub fn pipe_owner_is_current_user(pipe: &File) -> io::Result<bool> {
        Ok(pipe_owner(pipe)? == current_user()?)
    }

    /// Odczyt z potoku, który nie blokuje dłużej niż do `deadline`: czyta tylko
    /// to, co już jest w potoku (`PeekNamedPipe`). Po terminie `TimedOut`,
    /// zamknięty przez drugą stronę potok to koniec strumienia.
    pub struct TimedReader<'a> {
        file: &'a mut File,
        deadline: Instant,
    }

    impl<'a> TimedReader<'a> {
        pub fn new(file: &'a mut File, deadline: Instant) -> Self {
            Self { file, deadline }
        }
    }

    impl Read for TimedReader<'_> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if buf.is_empty() {
                return Ok(0);
            }
            let h = self.file.as_raw_handle() as Handle;
            loop {
                let mut avail = 0u32;
                let ok = unsafe {
                    PeekNamedPipe(
                        h,
                        std::ptr::null_mut(),
                        0,
                        std::ptr::null_mut(),
                        &mut avail,
                        std::ptr::null_mut(),
                    )
                };
                if ok == 0 {
                    let e = io::Error::last_os_error();
                    return match e.raw_os_error() {
                        Some(ERROR_BROKEN_PIPE) => Ok(0),
                        _ => Err(e),
                    };
                }
                if avail > 0 {
                    let want = buf.len().min(avail as usize);
                    return self.file.read(&mut buf[..want]);
                }
                if Instant::now() >= self.deadline {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "aplikacja nie odpowiedziała na czas",
                    ));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

/// Nazwa użytkownika Windows w nazwie potoku: tylko litery, cyfry, `.`, `_`, `-`.
pub fn sanitize_user(user: &str) -> String {
    let clean: String = user
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    if clean.is_empty() {
        "user".into()
    } else {
        clean
    }
}

/// `\\.\pipe\tuktuk-track-bridge-<użytkownik>`.
pub fn pipe_name_for(user: &str) -> String {
    format!(r"\\.\pipe\tuktuk-track-bridge-{}", sanitize_user(user))
}

/// Potok bieżącego użytkownika (zmienna `USERNAME`).
pub fn pipe_name() -> String {
    pipe_name_for(&std::env::var("USERNAME").unwrap_or_default())
}
