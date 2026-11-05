//! Klucz odblokowujący moduł Workflows (docs/workflows/PLAN_WDROZENIA.md, 4.1.1).
//!
//! Decyzja E0: jeden stały klucz dostępu, wspólny dla wszystkich instalacji.
//! W kodzie jest tylko skrót SHA-256 klucza, nie sam klucz. To lokalna bramka
//! funkcji eksperymentalnej, nie zabezpieczenie licencyjne: kto zmodyfikuje
//! aplikację, może ją ominąć.

use sha2::{Digest, Sha256};

/// SHA-256 znormalizowanego klucza (`normalize`).
const WORKFLOWS_KEY_SHA256: [u8; 32] = [
    0x20, 0x15, 0x07, 0xeb, 0x20, 0x8e, 0xa6, 0x94, 0xe0, 0x28, 0x36, 0x1c, 0xb9, 0x7a, 0xe1, 0x25,
    0x27, 0xd9, 0xa9, 0xc3, 0xd5, 0xd7, 0x64, 0x9e, 0xf6, 0xe3, 0xd5, 0x84, 0xff, 0x9f, 0x40, 0x95,
];

/// Przycina białe znaki i ujednolica wielkość liter, żeby wklejony klucz
/// ze spacją lub małymi literami też przechodził.
fn normalize(input: &str) -> String {
    input.trim().to_ascii_uppercase()
}

/// Sprawdza klucz wpisany przez użytkownika. Porównanie skrótów bez
/// wczesnego wyjścia, żeby czas nie zależał od zgodnych bajtów.
pub fn is_valid_workflows_key(input: &str) -> bool {
    let digest = Sha256::digest(normalize(input).as_bytes());
    digest
        .iter()
        .zip(WORKFLOWS_KEY_SHA256.iter())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_and_wrong_keys() {
        assert!(!is_valid_workflows_key(""));
        assert!(!is_valid_workflows_key("   "));
        assert!(!is_valid_workflows_key("TUKTUK-WF-00000-00000-00000-00000"));
    }

    /// Klucza nie ma w repozytorium. Test sprawdza go tylko, gdy podano
    /// `TUKTUK_WF_KEY` w środowisku (lokalnie, przez właściciela).
    #[test]
    fn accepts_key_from_env_when_provided() {
        let Ok(key) = std::env::var("TUKTUK_WF_KEY") else {
            return;
        };
        assert!(is_valid_workflows_key(&key));
        assert!(is_valid_workflows_key(&format!(
            "  {} ",
            key.to_lowercase()
        )));
    }

    #[test]
    fn normalize_trims_and_uppercases() {
        assert_eq!(normalize("  tuktuk-wf-abc \n"), "TUKTUK-WF-ABC");
    }
}
