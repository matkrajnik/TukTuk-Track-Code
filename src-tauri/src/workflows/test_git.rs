//! Wspólne ograniczenie testów uruchamiających wiele poleceń Git.
//!
//! Pomiar (E5, Windows z ochroną antywirusową): `git status` trwa 0,16 s, ale 16
//! równoległych pętli takich wywołań nie daje większej przepustowości niż jedna
//! (ok. 4-6 procesów na sekundę na całą maszynę), więc pojedyncze polecenie czeka
//! 4-15 s. Przebieg to ok. 45 poleceń Git, a krok review kilkanaście, więc przy 16
//! wątkach testów przekraczał 30 s limitu kroku. W aplikacji sterownik prowadzi
//! jeden przebieg naraz; to ograniczenie odtwarza w testach warunki produkcyjne.

use std::sync::{Condvar, Mutex};

/// Najwyżej tyle testów z intensywnym użyciem Gita naraz (w całym pliku binarnym).
pub const MAX_GIT_HEAVY_TESTS: usize = 3;

static GIT_HEAVY: (Mutex<usize>, Condvar) = (Mutex::new(0), Condvar::new());

/// Zgoda na test z Gitem; zwalniana w `Drop`.
pub struct GitPermit;

impl GitPermit {
    pub fn acquire() -> Self {
        let (count, freed) = &GIT_HEAVY;
        let mut active = count.lock().unwrap_or_else(|e| e.into_inner());
        while *active >= MAX_GIT_HEAVY_TESTS {
            active = freed.wait(active).unwrap_or_else(|e| e.into_inner());
        }
        *active += 1;
        GitPermit
    }
}

impl Drop for GitPermit {
    fn drop(&mut self) {
        let (count, freed) = &GIT_HEAVY;
        *count.lock().unwrap_or_else(|e| e.into_inner()) -= 1;
        freed.notify_one();
    }
}
