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

/// Klient potoku: wysyła ramkę do aplikacji i czeka na odpowiedź. Zajęty potok
/// (inna wiadomość w toku) próbujemy otworzyć jeszcze kilka razy.
pub fn exchange(pipe: &str, body: &[u8]) -> Result<Vec<u8>, FrameError> {
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
    write_frame(&mut file, body)?;
    read_frame(&mut file)?.ok_or(FrameError::Truncated)
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
