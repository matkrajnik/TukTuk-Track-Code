//! Sekret dłuższy niż jeden wpis Menedżera poświadczeń Windows.
//!
//! Wpis przyjmuje najwyżej 2560 bajtów, a `keyring` zapisuje hasło w UTF-16,
//! czyli do 1280 znaków. Refresh token Microsoftu dla kont służbowych bywa
//! dłuższy, więc taki sekret leży w częściach: wpis główny trzyma znacznik
//! z liczbą części, a części są we wpisach `<użytkownik>#1`, `#2`, ...
//! Krótki sekret leży w samym wpisie głównym, jak przed podziałem, więc
//! zapisane wcześniej tokeny czyta się bez migracji.
use keyring::{Entry, Error};

/// Znaków w jednej części: 2000 bajtów w UTF-16, poniżej limitu 2560.
const PART_CHARS: usize = 1000;
/// Górna granica liczby części (16 000 znaków): chroni przed uszkodzonym znacznikiem.
const MAX_PARTS: usize = 16;
/// Początek znacznika we wpisie głównym. Tokeny OAuth nie zawierają znaku U+0001.
const MARKER: &str = "\u{1}parts:";

/// Magazyn wpisów jednej usługi; w aplikacji Menedżer poświadczeń, w testach mapa.
pub(crate) trait SecretStore {
    fn get(&self, user: &str) -> Result<String, Error>;
    fn set(&self, user: &str, value: &str) -> Result<(), Error>;
    fn delete(&self, user: &str) -> Result<(), Error>;
}

/// Menedżer poświadczeń Windows dla usługi `service`.
pub(crate) struct Keyring<'a>(pub &'a str);

impl SecretStore for Keyring<'_> {
    fn get(&self, user: &str) -> Result<String, Error> {
        Entry::new(self.0, user)?.get_password()
    }
    fn set(&self, user: &str, value: &str) -> Result<(), Error> {
        Entry::new(self.0, user)?.set_password(value)
    }
    fn delete(&self, user: &str) -> Result<(), Error> {
        Entry::new(self.0, user)?.delete_credential()
    }
}

fn part_user(user: &str, index: usize) -> String {
    format!("{user}#{index}")
}

/// Usuwa części od `from` w górę, do pierwszej brakującej.
fn remove_parts(store: &impl SecretStore, user: &str, from: usize) {
    for index in from..=MAX_PARTS {
        if matches!(store.delete(&part_user(user, index)), Err(Error::NoEntry)) {
            break;
        }
    }
}

/// Zapisuje sekret: w jednym wpisie, gdy się mieści, inaczej w częściach.
/// Części zbędne po poprzednim, dłuższym sekrecie znikają.
pub(crate) fn save(store: &impl SecretStore, user: &str, secret: &str) -> Result<(), Error> {
    let chars: Vec<char> = secret.chars().collect();
    let parts: Vec<String> = chars
        .chunks(PART_CHARS)
        .map(|part| part.iter().collect())
        .collect();
    if parts.len() <= 1 {
        store.set(user, secret)?;
        remove_parts(store, user, 1);
        return Ok(());
    }
    if parts.len() > MAX_PARTS {
        return Err(Error::TooLong(
            "secret".into(),
            (PART_CHARS * MAX_PARTS) as u32,
        ));
    }
    for (index, part) in parts.iter().enumerate() {
        store.set(&part_user(user, index + 1), part)?;
    }
    store.set(user, &format!("{MARKER}{}", parts.len()))?;
    remove_parts(store, user, parts.len() + 1);
    Ok(())
}

/// Czyta sekret zapisany przez [`save`] (albo zwykły wpis sprzed podziału).
pub(crate) fn load(store: &impl SecretStore, user: &str) -> Result<String, Error> {
    let main = store.get(user)?;
    let Some(count) = main.strip_prefix(MARKER) else {
        return Ok(main);
    };
    let count = count
        .parse::<usize>()
        .ok()
        .filter(|n| (2..=MAX_PARTS).contains(n))
        .ok_or_else(|| Error::Invalid("secret".into(), "bad part count".into()))?;
    (1..=count)
        .map(|index| store.get(&part_user(user, index)))
        .collect()
}

/// Usuwa wpis główny i wszystkie części. Wynik dotyczy wpisu głównego
/// (`NoEntry`, gdy sekretu nie było).
pub(crate) fn delete(store: &impl SecretStore, user: &str) -> Result<(), Error> {
    let result = store.delete(user);
    remove_parts(store, user, 1);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    /// Magazyn w pamięci z limitem 2560 bajtów UTF-16 na wpis, jak Windows.
    #[derive(Default)]
    struct Memory(RefCell<BTreeMap<String, String>>);

    impl SecretStore for Memory {
        fn get(&self, user: &str) -> Result<String, Error> {
            self.0.borrow().get(user).cloned().ok_or(Error::NoEntry)
        }
        fn set(&self, user: &str, value: &str) -> Result<(), Error> {
            if value.encode_utf16().count() * 2 > 2560 {
                return Err(Error::TooLong("password".into(), 2560));
            }
            self.0.borrow_mut().insert(user.into(), value.into());
            Ok(())
        }
        fn delete(&self, user: &str) -> Result<(), Error> {
            self.0
                .borrow_mut()
                .remove(user)
                .map(|_| ())
                .ok_or(Error::NoEntry)
        }
    }

    fn token(len: usize) -> String {
        (0..len)
            .map(|i| char::from(b'a' + (i % 26) as u8))
            .collect()
    }

    #[test]
    fn short_secret_stays_in_one_entry() {
        let store = Memory::default();
        save(&store, "u", "abc").unwrap();
        assert_eq!(load(&store, "u").unwrap(), "abc");
        assert_eq!(store.0.borrow().len(), 1);
    }

    #[test]
    fn long_microsoft_token_is_split_and_read_back() {
        let store = Memory::default();
        let refresh = token(2400);
        save(&store, "u", &refresh).unwrap();
        assert_eq!(load(&store, "u").unwrap(), refresh);
        assert_eq!(store.0.borrow().len(), 4, "znacznik i trzy części");
    }

    #[test]
    fn shorter_secret_removes_leftover_parts() {
        let store = Memory::default();
        save(&store, "u", &token(3500)).unwrap();
        save(&store, "u", &token(1500)).unwrap();
        assert_eq!(load(&store, "u").unwrap(), token(1500));
        assert_eq!(store.0.borrow().len(), 3);
        save(&store, "u", "short").unwrap();
        assert_eq!(load(&store, "u").unwrap(), "short");
        assert_eq!(store.0.borrow().len(), 1);
    }

    #[test]
    fn delete_removes_main_entry_and_parts() {
        let store = Memory::default();
        save(&store, "u", &token(2400)).unwrap();
        delete(&store, "u").unwrap();
        assert!(store.0.borrow().is_empty());
        assert!(matches!(load(&store, "u"), Err(Error::NoEntry)));
        assert!(matches!(delete(&store, "u"), Err(Error::NoEntry)));
    }

    #[test]
    fn missing_part_or_bad_marker_is_an_error() {
        let store = Memory::default();
        save(&store, "u", &token(2400)).unwrap();
        store.delete("u#2").unwrap();
        assert!(load(&store, "u").is_err());
        store.set("u", &format!("{MARKER}999")).unwrap();
        assert!(matches!(load(&store, "u"), Err(Error::Invalid(..))));
    }

    /// Prawdziwy Menedżer poświadczeń: `cargo test -- --ignored secret_parts`.
    #[test]
    #[ignore = "zapisuje do Menedżera poświadczeń Windows"]
    fn windows_credential_manager_keeps_long_token() {
        let store = Keyring("tuktuk-track.test.secret-parts");
        let refresh = token(2400);
        assert!(matches!(
            store.set("whole", &refresh),
            Err(Error::TooLong(..))
        ));
        save(&store, "u", &refresh).unwrap();
        let read = load(&store, "u");
        delete(&store, "u").unwrap();
        assert_eq!(read.unwrap(), refresh);
        assert!(matches!(load(&store, "u"), Err(Error::NoEntry)));
    }

    #[test]
    fn too_long_secret_is_rejected() {
        let store = Memory::default();
        let result = save(&store, "u", &token(PART_CHARS * MAX_PARTS + 1));
        assert!(matches!(result, Err(Error::TooLong(..))));
        assert!(store.0.borrow().is_empty());
    }
}
