# Wydania i automatyczne aktualizacje

Kod Todo Track jest w prywatnym repozytorium (`github.com/matkrajnik/todo-track-private`),
a wydania idą do osobnego **publicznego** repozytorium `matkrajnik/todo-track-releases`,
które zawiera tylko release'y z trzema plikami: instalator `.exe`, podpis `.sig`
i `latest.json`. Zainstalowana aplikacja pyta o `latest.json` przy starcie i raz dziennie,
pokazuje baner „Dostępna jest nowa wersja” i po kliknięciu pobiera oraz instaluje
aktualizację (plugin `tauri-plugin-updater`, instalator NSIS w trybie pasywnym).

## Jak to działa

- **Konfiguracja**: `src-tauri/tauri.conf.json` → `plugins.updater` (adres
  `https://github.com/matkrajnik/todo-track-releases/releases/latest/download/latest.json`,
  klucz publiczny `pubkey`, `windows.installMode: "passive"`) i
  `bundle.createUpdaterArtifacts: true` (build tworzy podpis `.exe.sig` obok instalatora).
  Uprawnienia `updater:default` i `process:default` są w `src-tauri/capabilities/default.json`.
  CSP ma `connect-src` z `github.com` i hostami plików release (zapytania wykonuje Rust,
  więc wpis jest tylko dla porządku).
- **Aplikacja** (`src/updater.ts`, `src/components/UpdateBanner.vue`): 2 s po starcie
  głównego okna i potem co godzinę sprawdza, czy od ostatniego pytania minęła doba;
  brak sieci jest ignorowany bez komunikatu. Baner pokazuje numer wersji, notatki
  z wydania oraz przyciski „Zainstaluj i uruchom ponownie” (pasek postępu pobierania)
  i „Później” (chowa baner do następnego sprawdzenia). Ustawienia → Uruchamianie →
  Aktualizacje: przełącznik „Sprawdzaj aktualizacje automatycznie” (ustawienie
  `autoUpdateCheck`, domyślnie włączone) i „Sprawdź teraz” z wynikiem („Masz najnowszą
  wersję (X)” / „Nie udało się połączyć z serwerem aktualizacji.” / „Dostępna jest wersja X”).
- **Instalacja**: plugin pobiera instalator (`download()` z postępem) i weryfikuje podpis
  minisign kluczem publicznym. Przed `install()` frontend wywołuje komendę `prepare_update`
  (`src-tauri/src/main.rs`), która kończy bieżący pomiar (`db::stop`) i zapisuje stan okien,
  bo `install()` w Windows zapisuje instalator w katalogu tymczasowym, uruchamia go
  z argumentami `/P /UPDATE /R /ARGS <argumenty bieżącego procesu>` i od razu kończy proces
  przez `std::process::exit(0)`, bez zdarzeń zamknięcia okien. Instalator (własny szablon
  `src-tauri/installer/template.nsi`) w trybie pasywnym pomija pytanie „Zaktualizować…?”,
  wywołuje `todo-track.exe --prepare-update` (nowa instancja zapisuje koniec pomiaru
  i kończy się), kopiuje pliki i przez `/R` uruchamia aplikację ponownie z tymi samymi
  argumentami (np. `--hidden` z autostartu). Nowa wersja przy pierwszym starcie robi
  kopię `before-<wersja>-<czas>.sqlite` jak dotąd.
- **latest.json** (format pluginu): `version`, `notes`, `pub_date` (RFC 3339) i
  `platforms."windows-x86_64"` z polami `signature` (treść pliku `.sig`) i `url` do
  instalatora w release. Updater instaluje tylko wersję wyższą od zainstalowanej.

Przykład:

```json
{
  "version": "1.7.3",
  "notes": "- Automatyczne aktualizacje",
  "pub_date": "2026-10-20T10:00:00Z",
  "platforms": {
    "windows-x86_64": {
      "signature": "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkK…",
      "url": "https://github.com/matkrajnik/todo-track-releases/releases/download/v1.7.3/TodoTrack_1.7.3_x64-setup.exe"
    }
  }
}
```

## Jednorazowa konfiguracja

### 1. Klucz podpisu

Aktualizacje są podpisane kluczem minisign. Klucz prywatny **nie trafia do repozytorium**
(`.gitignore` ignoruje `*.key`); klucz publiczny jest w `tauri.conf.json`.

```bash
npx tauri signer generate -w ~/.tauri/todotrack.key
```

Polecenie pyta o hasło klucza (puste jest dozwolone) i zapisuje
`~/.tauri/todotrack.key` (prywatny) oraz `~/.tauri/todotrack.key.pub` (publiczny).
Treść pliku `.pub` wklej do `plugins.updater.pubkey`.

Stan na 19 października 2026: para kluczy została wygenerowana na tej maszynie
(`C:\Users\m.krajnik\.tauri\todotrack.key`) poleceniem z flagą `--ci`, czyli **bez
hasła**, a jej klucz publiczny jest już w `tauri.conf.json`. Jeśli wolisz klucz z hasłem,
wygeneruj nowy (`-f` nadpisuje plik), podmień `pubkey` w konfiguracji i zrób to przed
pierwszym wydaniem z updaterem: zainstalowane wersje przyjmują tylko podpisy pasujące
do klucza publicznego, z którym zostały zbudowane. Zmiana klucza po wydaniu oznacza
jedną ręczną instalację u każdego użytkownika.

**Zrób kopię zapasową klucza prywatnego (i hasła).** Bez niego nie da się podpisać
kolejnego wydania, a zainstalowane aplikacje nie przyjmą aktualizacji.

### 2. Zmienne środowiskowe podpisu

Build z `createUpdaterArtifacts` wymaga klucza prywatnego:

| Zmienna | Znaczenie |
| --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | ścieżka do pliku klucza **albo** jego treść |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | hasło klucza; pomiń (albo pusta), gdy klucz nie ma hasła |

PowerShell (na czas sesji):

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = "$HOME\.tauri\todotrack.key"
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""
```

Git Bash:

```bash
export TAURI_SIGNING_PRIVATE_KEY="$HOME/.tauri/todotrack.key"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
```

Gdy `TAURI_SIGNING_PRIVATE_KEY` nie jest ustawiona, `npm run release` używa
`~/.tauri/todotrack.key`, jeśli plik istnieje. Zwykły `npm run tauri build` tego nie robi:
bez zmiennej kończy się błędem o braku klucza.

### 3. Publiczne repozytorium wydań

Utwórz publiczne repozytorium `todo-track-releases` na koncie `matkrajnik` (strona GitHub
albo `gh repo create matkrajnik/todo-track-releases --public --add-readme`). Repozytorium
musi mieć co najmniej jeden commit (np. README z informacją „tu są tylko wydania Todo
Track”), bo release tworzy tag na gałęzi domyślnej. Nie oznaczaj wydań jako draft ani
pre-release: adres `releases/latest/download/latest.json` wskazuje najnowszy zwykły release.

Jeśli repozytorium ma inną nazwę albo właściciela, zmień adres w
`plugins.updater.endpoints` w `tauri.conf.json`; skrypt wydania czyta stamtąd
`<OWNER>/<REPO>`. Właściciel `matkrajnik` pochodzi z adresu zdalnego repozytorium
prywatnego na GitHubie.

### 4. GitHub CLI

```powershell
winget install GitHub.cli
gh auth login
```

Logowanie: GitHub.com, protokół HTTPS, przeglądarka. Konto musi mieć prawo zapisu do
`todo-track-releases`. Sprawdzenie: `gh auth status`. (19 października 2026 `gh` nie
było zainstalowane na tej maszynie.)

## Wydanie krok po kroku

1. **Wersja** w trzech plikach: `src-tauri/tauri.conf.json`, `package.json`,
   `src-tauri/Cargo.toml` (`[package] version`). Potem w `src-tauri` uruchom `cargo check`
   (bez `--locked`), żeby `Cargo.lock` dostał nową wersję pakietu; `npm run release`
   buduje z `--locked`. Dopisz „Nowości” w `README.md`.
2. **Sprawdzenie**: `npm run check` (build frontendu, testy Playwright, `cargo fmt --check`,
   `cargo test --locked`).
3. **Wydanie**:

   ```powershell
   npm run release -- --notes-file notes.md
   ```

   albo z jedną linią notatek: `npm run release -- --notes "Poprawki tablicy tygodnia"`.
   Wielowierszowe notatki podawaj z pliku: `npm run` w Windows ucina argument na
   końcu pierwszej linii. Skrypt (`scripts/release.mjs`):
   - sprawdza zgodność wersji w trzech plikach i adres repozytorium wydań w konfiguracji;
   - sprawdza `cargo` (dodaje `%USERPROFILE%\.cargo\bin` do PATH, jak `npm run check`),
     klucz podpisu, `gh auth status` i czy release `v<wersja>` jeszcze nie istnieje;
   - buduje `npm run tauri build -- -- --locked`;
   - kopiuje `Todo Track_<wersja>_x64-setup.exe` i `.sig` do
     `src-tauri/target/release/bundle/updater/` pod nazwą **bez spacji**
     `TodoTrack_<wersja>_x64-setup.exe` (GitHub zamienia spacje w nazwach plików release
     na kropki, więc adres w `latest.json` musi wskazywać nazwę bez spacji), składa
     `latest.json` i zapisuje notatki do `notes.md`;
   - kopiuje instalator pod oryginalną nazwą do `releases/` i usuwa stamtąd starsze
     instalatory (folder trzyma tylko najnowszy);
   - wywołuje `gh release create v<wersja> --repo matkrajnik/todo-track-releases
     --title "Todo Track <wersja>" --notes-file … <exe> <sig> latest.json`.

   Opcje: `--dry-run` robi wszystko poza publikacją (bez kopii w `releases/`) i wypisuje
   polecenie `gh`; `--skip-build` używa gotowego instalatora z `target/` (np. powtórka po
   błędzie `gh`). Skrypt **nie commituje i nie taguje** prywatnego repozytorium.
4. **Prywatne repozytorium**: commit (wersje, README, `releases/…exe`), tag `v<wersja>`
   i push z tagiem, np. `git push github.com master --tags`.
5. **Kontrola**: strona release'u ma trzy pliki, `latest.json` wskazuje właściwy adres,
   a w zainstalowanej aplikacji „Sprawdź teraz” pokazuje nową wersję (albo „Masz najnowszą
   wersję”, gdy to ona jest zainstalowana).

Instalacje sprzed wersji z updaterem (1.7.2 i starsze) nie sprawdzają aktualizacji;
pierwszą wersję z updaterem trzeba zainstalować ręcznie z `releases/`.

## Pierwsza aktualizacja przez updater: sprawdzenie natywne

Ścieżka aktualizacji nie była uruchamiana natywnie; testy Playwright pokrywają tylko
interfejs (baner, postęp, ustawienia) na zastępczym pluginie, a `cargo test` logikę
ustawień. Instalator ma własny szablon NSIS z dodatkowym potwierdzeniem wersji i krokiem
`--prepare-update`, więc **pierwsze wydanie z updaterem wymaga ręcznego sprawdzenia**
na prawdziwej instalacji (najlepiej dwie wersje testowe, np. 1.7.3 zainstalowana ręcznie
i 1.7.4 opublikowana jako release):

- baner pojawia się po starcie (do kilku sekund) i po „Sprawdź teraz”; „Później” chowa go;
- „Zainstaluj i uruchom ponownie”: pasek postępu, potem aplikacja znika, a instalator
  w **trybie pasywnym** (`/P /UPDATE /R`) pokazuje tylko pasek instalacji, **bez** pytania
  „Zaktualizować Todo Track z wersji…?” (w `.onInit` szablonu pytanie jest pomijane przy
  `$PassiveMode = 1`) i bez wyboru katalogu;
- `--prepare-update` w `Section Install`: aplikacja została już zakończona przez plugin,
  więc polecenie uruchamia nową instancję, która kończy pomiar i wychodzi; instalator nie
  może utknąć na `TodoTrackRequireClosed` („aplikacja nadal działa”);
- przy **aktywnym pomiarze** przedział kończy się w chwili aktualizacji (komenda
  `prepare_update` przed `install()` i `--prepare-update` z instalatora), nic nie ginie, powstaje kopia
  `before-<nowa wersja>-<czas>.sqlite`;
- po instalacji aplikacja startuje sama (`/R`), z tymi samymi argumentami: start
  z autostartu (`--hidden`) wraca schowany do traya, zwykły start pokazuje okno; pozycje
  i rozmiary okien są zachowane;
- wpis w „Aplikacje i funkcje” ma nową wersję, skróty działają, rozszerzenie przeglądarki
  (`todo-track-bridge.exe`) nadal jest zarejestrowane;
- błędny podpis (np. podmieniony `signature` w `latest.json`) daje w banerze komunikat
  „Nie udało się zainstalować aktualizacji…”, a aplikacja działa dalej;
- bez sieci start aplikacji jest cichy, a „Sprawdź teraz” pokazuje „Nie udało się
  połączyć z serwerem aktualizacji.”.

Przy zmianie wersji Tauri CLI porównaj szablon NSIS z upstreamem (zob. README,
„Instalacja i aktualizacje”), bo updater zależy od obsługi `/P`, `/UPDATE`, `/R` i `/ARGS`
w `.onInit` oraz `.onInstSuccess`.
