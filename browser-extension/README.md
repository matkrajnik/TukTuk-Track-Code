# Rozszerzenie TukTuk Track dla Chrome i Edge

Rozszerzenie podpowiada w TukTuk Track zadanie ze zgłoszenia Jiry otwartego w aktywnej karcie, także wtedy, gdy tytuł okna nie ma klucza (np. tablica z otwartym zgłoszeniem `?selectedIssue=WMS-123`). Aplikacja pokazuje wtedy „Pracujesz teraz przy WMS-123?” z przyciskiem „Przełącz”. Timer nigdy nie przełącza się sam.

## Co jest wysyłane

- Adres i tytuł aktywnej karty, tylko dla domen z listy dozwolonych (opcje rozszerzenia; domyślnie `*.atlassian.net`). Dla innych stron rozszerzenie wysyła samo „clear”, bez adresu i tytułu.
- Nazwa przeglądarki (`chrome`, `edge`, `brave`, `opera` albo pusta) i przy „clear” powód: `page` (inna strona) albo `background` (przeglądarka w tle). Aplikacja trzyma stan osobno dla każdej przeglądarki, a „clear” z tła go nie gasi, więc Chrome i Edge (albo dwa profile) z rozszerzeniem nie nadpisują sobie klucza.
- Wiadomości idą przez native messaging do programu `tuktuk-track-bridge.exe` (instalowany razem z aplikacją), a on przekazuje je do działającej aplikacji nazwanym potokiem `\\.\pipe\tuktuk-track-bridge-<użytkownik>`. Gdy aplikacja nie działa, host odpowiada „niepołączone” i nic nie robi.
- Aplikacja przyjmuje tylko `*.atlassian.net` i witryny Jiry, które zna, i trzyma wyłącznie rozpoznany klucz zgłoszenia, w pamięci, przez 30 s od ostatniego odświeżenia. Nic nie trafia do bazy ani logów.

Rozpoznanie: `/browse/KEY`, parametr `selectedIssue=KEY`, `/issues/KEY`, a na końcu klucz w tytule karty.

## Instalacja

1. W TukTuk Track: **Ustawienia → Czas pracy → Rozszerzenie przeglądarki** — włącz. Aplikacja zapisze manifest hosta obok `TukTuk Track.exe` i wpisy w `HKCU\Software\Google\Chrome\NativeMessagingHosts\pl.tuktuktrack.bridge` oraz `HKCU\Software\Microsoft\Edge\NativeMessagingHosts\pl.tuktuktrack.bridge` (bez uprawnień administratora). Wyłączenie usuwa wpisy i manifest.
2. Załaduj rozszerzenie:
   - **Tryb dewelopera**: `chrome://extensions` albo `edge://extensions` → włącz „Tryb dewelopera” → „Załaduj rozpakowane” → wskaż folder `browser-extension` (w instalacji: `<folder aplikacji>\browser-extension`; przycisk „Otwórz folder rozszerzenia” w Ustawieniach).
   - **Zasady grupy w firmie**: administrator publikuje spakowane rozszerzenie (`.crx`) i dodaje je do `ExtensionInstallForcelist` (Chrome) lub `ExtensionInstallForcelist` w zasadach Edge. Identyfikator rozszerzenia jest stały: `cecpcpkmmmkgfdklnkkkigofkcibnhbl`.
3. W opcjach rozszerzenia (ikona TukTuk Track na pasku) sprawdź stan połączenia i w razie potrzeby dopisz domenę serwera Jiry, np. `jira.firma.pl`.

## Identyfikator i klucz

Stały identyfikator wynika z pola `key` w `manifest.json` (klucz publiczny RSA). Klucz prywatny, potrzebny tylko do pakowania `.crx`, jest poza repozytorium (u autora w `%USERPROFILE%\.todo-track\browser-extension-key.pem`; ten plik lokalny nie zmienia nazwy razem z aplikacją, tak jak klucz podpisu `todotrack.key`); pliki `*.pem` i `*.crx` są w `.gitignore`. Ten sam identyfikator jest w `allowed_origins` manifestu hosta (`browser::EXTENSION_ID` w `src-tauri/src/browser.rs`; test Rust sprawdza zgodność z `key`).

## Testy

Czyste funkcje (`lib.js`: filtr domen, wiadomość z karty, opis stanu) testuje `tests/browser-extension.spec.ts` w Playwright. Rozpoznanie adresów, ramka native messaging, wygasanie sygnału i rejestracja w rejestrze są w testach Rust (`browser.rs`, `foreground.rs`).
