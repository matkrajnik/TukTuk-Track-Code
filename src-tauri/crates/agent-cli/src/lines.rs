use std::io::{BufRead, BufReader, Read};

/// Jedna linia odczytana ze strumienia.
#[derive(Clone, Debug, PartialEq)]
pub enum LineItem {
    Line(String),
    /// Linia przekroczyła limit; jej treść odrzucono do końca linii.
    TooLong,
}

fn finish(mut line: Vec<u8>) -> String {
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    String::from_utf8_lossy(&line).into_owned()
}

/// Czyta linie bajtowo (`\n`, ostatni `\r` obcięty) z limitem `max_bytes`.
/// Dekodowanie UTF-8 (lossy) dopiero dla całej linii, więc znak podzielony
/// między bufory nie ulega uszkodzeniu. `on_item` zwraca `false`, by przerwać.
pub fn read_lines(reader: impl Read, max_bytes: usize, mut on_item: impl FnMut(LineItem) -> bool) {
    let mut reader = BufReader::with_capacity(8192, reader);
    let mut line: Vec<u8> = Vec::new();
    let mut overflow = false;
    loop {
        let chunk = match reader.fill_buf() {
            Ok([]) => break,
            Ok(chunk) => chunk,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            // Zerwany potok kończy odczyt; kod wyjścia niesie `Exited`.
            Err(_) => break,
        };
        let newline = chunk.iter().position(|&b| b == b'\n');
        let take = newline.unwrap_or(chunk.len());
        if !overflow {
            if line.len() + take > max_bytes {
                overflow = true;
                line.clear();
            } else {
                line.extend_from_slice(&chunk[..take]);
            }
        }
        let consumed = newline.map_or(chunk.len(), |p| p + 1);
        reader.consume(consumed);
        if newline.is_some() {
            let item = if overflow {
                LineItem::TooLong
            } else {
                LineItem::Line(finish(std::mem::take(&mut line)))
            };
            line.clear();
            overflow = false;
            if !on_item(item) {
                return;
            }
        }
    }
    // Ostatnia linia bez końca linii.
    if overflow {
        on_item(LineItem::TooLong);
    } else if !line.is_empty() {
        on_item(LineItem::Line(finish(line)));
    }
}
