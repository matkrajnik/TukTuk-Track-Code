//! Nadzorca procesu strumieniowego: linie stdout z limitem, ogon stderr,
//! kod wyjścia i kończenie drzewa. Wątki std i mpsc, bez tokio.

use std::io::{Read, Write};
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use crate::identity::ProcessIdentity;
use crate::lines::{read_lines, LineItem};
use crate::spec::{command_with, Spec};
use crate::tree::{kill_tree_by_pid, Job, KillResult};

/// Najdłuższa linia stdout; nadmiar odrzucany do końca linii i zgłaszany.
pub const MAX_LINE_BYTES: usize = 1024 * 1024;
/// Ile ostatnich bajtów stderr zostaje w pamięci.
pub const STDERR_TAIL_BYTES: usize = 64 * 1024;
/// Jak długo czekamy na potwierdzenie końca procesu po `kill`.
pub const KILL_CONFIRM_TIMEOUT: Duration = Duration::from_secs(5);
/// Jak często nadzorca sprawdza, czy proces się zakończył.
const EXIT_POLL: Duration = Duration::from_millis(20);
/// Ile czekamy na dokończenie odczytu potoków po końcu procesu (potomek mógł je odziedziczyć).
const READER_GRACE: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub max_line_bytes: usize,
    pub stderr_tail_bytes: usize,
    /// Czyści środowisko procesu przed dodaniem `spec.env`.
    pub clear_env: bool,
    /// Dołącza proces do Job Object (Windows). `false` tylko w testach ścieżki bez jobu.
    pub attach_job: bool,
    /// Kończenie drzewa po PID (`taskkill /T /F`); `true` = sukces. Podmieniane w testach.
    pub kill_tree: fn(u32) -> bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            max_line_bytes: MAX_LINE_BYTES,
            stderr_tail_bytes: STDERR_TAIL_BYTES,
            clear_env: false,
            attach_job: true,
            kill_tree: kill_tree_by_pid,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpawnError {
    /// Program nie istnieje (daje `CliMissing`).
    NotFound(String),
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessEvent {
    /// Linia stdout.
    Line(String),
    /// Linia stdout przekroczyła limit; odczyt idzie dalej.
    LineTooLong,
    /// Proces zakończył się (kod `None`, gdy zabity sygnałem lub bez kodu).
    Exited(Option<i32>),
}

/// Ogon stderr i liczba wszystkich odebranych bajtów.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StderrTail {
    pub text: String,
    pub total_bytes: u64,
}

#[derive(Default)]
struct TailBuffer {
    bytes: Vec<u8>,
    total: u64,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

pub struct StreamingProcess;

pub struct RunningProcess {
    pid: u32,
    identity: Option<ProcessIdentity>,
    events: Receiver<ProcessEvent>,
    child: Arc<Mutex<Child>>,
    job: Option<Job>,
    /// Dlaczego proces nie trafił do Job Object (wtedy koniec drzewa potwierdza tylko `taskkill`).
    job_error: Option<String>,
    kill_tree: fn(u32) -> bool,
    exited: Arc<AtomicBool>,
    tail: Arc<Mutex<TailBuffer>>,
}

impl StreamingProcess {
    pub fn spawn(spec: &Spec, options: Options) -> Result<RunningProcess, SpawnError> {
        let mut child = command_with(spec, options.clear_env).spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                SpawnError::NotFound(e.to_string())
            } else {
                SpawnError::Failed(e.to_string())
            }
        })?;
        let pid = child.id();
        #[cfg(windows)]
        let (job, job_error) = if options.attach_job {
            use std::os::windows::io::AsRawHandle;
            match Job::attach(child.as_raw_handle().cast()) {
                Ok(job) => (Some(job), None),
                Err(e) => (None, Some(e)),
            }
        } else {
            (None, Some("Job Object wyłączony w opcjach.".to_owned()))
        };
        #[cfg(not(windows))]
        let (job, job_error) = (
            None::<Job>,
            Some("Job Object jest dostępny tylko na Windows.".to_owned()),
        );

        if let Some(mut stdin) = child.stdin.take() {
            let text = spec.stdin.clone().into_bytes();
            thread::spawn(move || {
                // Proces może zamknąć stdin bez czytania (BrokenPipe); to nie błąd kroku.
                let _ = stdin.write_all(&text);
            });
        }
        let (tx, events) = channel();
        let (done_tx, done_rx) = channel::<()>();
        let tail = Arc::new(Mutex::new(TailBuffer::default()));
        if let Some(out) = child.stdout.take() {
            spawn_stdout_reader(out, tx.clone(), done_tx.clone(), options.max_line_bytes);
        }
        if let Some(err) = child.stderr.take() {
            spawn_stderr_reader(err, Arc::clone(&tail), done_tx.clone(), options);
        }
        drop(done_tx);
        let child = Arc::new(Mutex::new(child));
        let exited = Arc::new(AtomicBool::new(false));
        spawn_waiter(Arc::clone(&child), Arc::clone(&exited), tx, done_rx);
        let identity = ProcessIdentity::capture(pid, job.is_some());
        Ok(RunningProcess {
            identity,
            pid,
            events,
            child,
            job,
            job_error,
            kill_tree: options.kill_tree,
            exited,
            tail,
        })
    }
}

fn spawn_stdout_reader(
    out: impl Read + Send + 'static,
    tx: Sender<ProcessEvent>,
    done: Sender<()>,
    max_line_bytes: usize,
) {
    thread::spawn(move || {
        read_lines(out, max_line_bytes, |item| {
            let event = match item {
                LineItem::Line(line) => ProcessEvent::Line(line),
                LineItem::TooLong => ProcessEvent::LineTooLong,
            };
            tx.send(event).is_ok()
        });
        // Odbiorca mógł zniknąć po anulowaniu; nic do zgłoszenia.
        let _ = done.send(());
    });
}

fn spawn_stderr_reader(
    mut err: impl Read + Send + 'static,
    tail: Arc<Mutex<TailBuffer>>,
    done: Sender<()>,
    options: Options,
) {
    thread::spawn(move || {
        let mut chunk = [0u8; 8192];
        loop {
            match err.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    let mut buffer = lock(&tail);
                    buffer.total += n as u64;
                    buffer.bytes.extend_from_slice(&chunk[..n]);
                    let excess = buffer.bytes.len().saturating_sub(options.stderr_tail_bytes);
                    if excess > 0 {
                        buffer.bytes.drain(..excess);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
        let _ = done.send(());
    });
}

fn spawn_waiter(
    child: Arc<Mutex<Child>>,
    exited: Arc<AtomicBool>,
    tx: Sender<ProcessEvent>,
    readers_done: Receiver<()>,
) {
    thread::spawn(move || {
        let code = loop {
            let status = lock(&child).try_wait();
            match status {
                Ok(Some(status)) => break status.code(),
                Ok(None) => thread::sleep(EXIT_POLL),
                Err(_) => break None,
            }
        };
        exited.store(true, Ordering::SeqCst);
        // Czekamy, aż oba potoki się opróżnią (każdy czytnik wysyła jedno `()`).
        let deadline = Instant::now() + READER_GRACE;
        while readers_done
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .is_ok()
        {}
        // Odbiorca mógł już zrezygnować; wtedy nikt nie czeka na kod wyjścia.
        let _ = tx.send(ProcessEvent::Exited(code));
    });
}

impl RunningProcess {
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Tożsamość procesu (PID i czas utworzenia) do rejestru procesów; `None`, gdy
    /// systemu nie dało się zapytać.
    pub fn identity(&self) -> Option<ProcessIdentity> {
        self.identity
    }

    pub fn events(&self) -> &Receiver<ProcessEvent> {
        &self.events
    }

    pub fn has_exited(&self) -> bool {
        self.exited.load(Ordering::SeqCst)
    }

    /// Ogon stderr zebrany do tej pory.
    pub fn stderr_tail(&self) -> StderrTail {
        let buffer = lock(&self.tail);
        StderrTail {
            text: String::from_utf8_lossy(&buffer.bytes).into_owned(),
            total_bytes: buffer.total,
        }
    }

    /// Powód braku Job Object; `None`, gdy proces jest w jobie.
    pub fn job_error(&self) -> Option<&str> {
        self.job_error.as_deref()
    }

    /// Kończy całe drzewo: Job Object, potem `taskkill /T /F` (łata wyścig
    /// między spawn a przypisaniem do jobu), potem potwierdzenie końca.
    ///
    /// `Confirmed` tylko wtedy, gdy bezpośredni proces się zakończył i:
    /// z jobem: job nie ma żywych procesów (`ActiveProcesses == 0`);
    /// bez jobu: `taskkill /T` zakończył się sukcesem. W każdym innym razie
    /// (także gdy nie da się zapytać jobu) wynik to `Unconfirmed`.
    pub fn kill(&self, confirm_timeout: Duration) -> KillResult {
        if let Some(job) = &self.job {
            // Wynik sprawdza niżej licznik procesów jobu, nie kod tego wywołania.
            job.terminate();
        }
        let tree_killed = (self.kill_tree)(self.pid);
        {
            let mut child = lock(&self.child);
            // Błąd oznacza zwykle, że proces już się zakończył; o wyniku decyduje potwierdzenie niżej.
            let _ = child.kill();
        }
        let deadline = Instant::now() + confirm_timeout;
        loop {
            if self.tree_ended(tree_killed) {
                return KillResult::Confirmed;
            }
            if Instant::now() >= deadline {
                return KillResult::Unconfirmed;
            }
            thread::sleep(EXIT_POLL);
        }
    }

    fn tree_ended(&self, tree_killed: bool) -> bool {
        if !self.has_exited() {
            return false;
        }
        match &self.job {
            Some(job) => job.active_processes() == Some(0),
            None => tree_killed,
        }
    }
}
