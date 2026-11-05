//! Proces zastępczy do testów nadzorcy (bez sieci i bez prawdziwych CLI).
//! Tryb wybiera pierwszy argument.

use std::io::Write;
use std::time::Duration;

fn sleep_forever() -> ! {
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}

fn arg(n: usize) -> String {
    std::env::args().nth(n).unwrap_or_default()
}

fn main() {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    match arg(1).as_str() {
        "replay" => {
            let text = std::fs::read_to_string(arg(2)).unwrap_or_default();
            for line in text.lines() {
                writeln!(out, "{line}").ok();
            }
        }
        "hang" => {
            writeln!(out, "start").ok();
            out.flush().ok();
            sleep_forever();
        }
        "spawn-child-and-hang" => {
            let exe = std::env::current_exe().expect("exe");
            let mut child = std::process::Command::new(exe)
                .arg("hang")
                .stdout(std::process::Stdio::null())
                .spawn()
                .expect("potomek");
            std::fs::write(arg(2), child.id().to_string()).expect("pidfile");
            writeln!(out, "child").ok();
            out.flush().ok();
            // Proces zastępczy czeka na potomka (kończy się razem z nim po kill drzewa).
            child.wait().ok();
            sleep_forever();
        }
        "ignore-stdin-close" => {
            writeln!(out, "ok").ok();
        }
        "big-stderr" => {
            let total: usize = arg(2).parse().unwrap_or(0);
            let block = vec![b'e'; 8192];
            let mut written = 0;
            let mut err = std::io::stderr().lock();
            while written < total {
                let n = block.len().min(total - written);
                err.write_all(&block[..n]).ok();
                written += n;
            }
            err.write_all(b"\nKONIEC-STDERR\n").ok();
            writeln!(out, "done").ok();
        }
        "long-line" => {
            let total: usize = arg(2).parse().unwrap_or(0);
            let line = vec![b'x'; total];
            out.write_all(&line).ok();
            writeln!(out).ok();
            writeln!(out, "after").ok();
        }
        "split-utf8" => {
            let bytes = "zażółć\n".as_bytes();
            // Podział w środku znaku „ż” (dwa bajty).
            out.write_all(&bytes[..3]).ok();
            out.flush().ok();
            std::thread::sleep(Duration::from_millis(100));
            out.write_all(&bytes[3..]).ok();
        }
        "exit" => {
            writeln!(out, "bye").ok();
            out.flush().ok();
            std::process::exit(arg(2).parse().unwrap_or(0));
        }
        "no-final" => {
            writeln!(out, "{{\"type\":\"unknown\"}}").ok();
        }
        "write-file" => {
            std::fs::write(arg(2), "zapis").expect("zapis");
        }
        "print-env" => {
            let mut names: Vec<String> = std::env::vars().map(|(k, _)| k).collect();
            names.sort();
            for name in names {
                writeln!(out, "{name}").ok();
            }
        }
        "print-args" => {
            for a in std::env::args().skip(2) {
                writeln!(out, "{}", serde_json::to_string(&a).expect("json")).ok();
            }
        }
        other => {
            eprintln!("nieznany tryb: {other}");
            std::process::exit(64);
        }
    }
}
