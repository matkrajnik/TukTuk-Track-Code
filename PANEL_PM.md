# Panel planowania dla PM — koncepcja

Szkic do przemyślenia, poza zakresem 1.7. Nic z tego nie jest zaimplementowane.

## Pomysł w jednym zdaniu

PM planuje mi zadania w przeglądarce, a Todo Track na moim komputerze je dostaje, ustawia w kolejności i pokazuje „co teraz”. PM widzi przy tym moje spotkania (zajętość) i stan pracy: co w toku, co skończone, co zablokowane.

## Role i uprawnienia

| Rola | Co może |
|------|---------|
| Pracownik (użytkownik Todo Track) | Widzi zadania zaplanowane dla siebie, przyjmuje je albo odrzuca z komentarzem, zmienia kolejność w obrębie dnia (PM widzi zmianę), decyduje, co z jego danych jest widoczne. |
| PM | Tworzy i planuje zadania dla osób ze swoich zespołów (dzień, kolejność, szacunek, priorytet, klucz Jiry). Widzi zajętość z kalendarza, plan i status. |
| Kierownik zespołu | Jak PM, plus zarządza składem zespołu. |
| Administrator | Konfiguracja organizacji, aplikacja w Entra ID, retencja danych. |

Jedna osoba może pracować dla kilku PM. Wtedy konflikty („dwóch PM planuje mi ten sam dzień na 120%”) muszą być widoczne dla obu.

## Przepływ

1. PM w panelu przeciąga zadanie na dzień pracownika (widok tygodnia w stylu tablicy z aplikacji: kolumny dni, pasek pojemności = cel dnia minus spotkania).
2. Serwer zapisuje zadanie jako „zaproponowane”. Aplikacja pobiera je przy następnej synchronizacji (co 1–5 min albo push przez WebSocket/SignalR).
3. W Todo Track zadanie pojawia się z oznaczeniem „od: Anna (PM)” i powiadomieniem z przyciskami „Przyjmij” / „Odrzuć…”. Przyjęte trafia do planu dnia w kolejności ustalonej przez PM.
4. Praca idzie jak dziś: timer, notatki, ukończenie. Do serwera wraca tylko **status** (nierozpoczęte / w toku / czekam na / ukończone) i opcjonalnie suma czasu na zadaniu.
5. PM widzi, co z planu jest zrobione i gdzie są rozjazdy z szacunkami.

Zasada z aplikacji obowiązuje dalej: panel tylko planuje i proponuje. Nigdy nie uruchamia ani nie przełącza timera na komputerze pracownika.

## Logowanie i konta (Microsoft Entra ID)

- **Panel web**: logowanie kontem firmowym Microsoft (OpenID Connect, MSAL). Bez osobnych haseł.
- **Aplikacja desktop**: to samo konto. W 1.7 aplikacja dostaje logowanie Microsoft (OAuth z PKCE) dla kalendarza, więc rejestracja aplikacji w Entra ID i obsługa tokenów będą już gotowe. Panel doda tylko zakres (scope) własnego API.
- **Zespoły**: z grup Entra ID (np. `TT-Zespol-WMS`) albo prowadzone ręcznie w panelu. Grupy są wygodniejsze, bo odejście pracownika z firmy odcina dostęp automatycznie.
- **Role**: role aplikacji w Entra ID (`PM`, `Lead`, `Admin`) przypisywane przez administratora. Backend sprawdza je w tokenie (`roles`).

## Spotkania

- Źródło: Microsoft Graph. Najprościej i najbezpieczniej przez **free/busy** (`getSchedule`): PM widzi tylko „zajęty 10:00–11:00”, bez tytułów. Wymaga tylko uprawnienia `Calendars.ReadBasic` w imieniu PM, bez dostępu do skrzynek innych osób.
- Opcjonalnie pracownik może w aplikacji włączyć udostępnianie tytułów spotkań swoim PM.
- Pojemność dnia w panelu = cel dnia pracownika (z jego ustawień) minus spotkania minus urlop (Graph: zdarzenia „poza biurem”).

## „Kto co robi” — prywatność i prawo

To najbardziej wrażliwy kawałek i od niego zależy, czy ludzie w ogóle zainstalują aplikację.

- Podgląd na żywo („Jan teraz mierzy WMS-123 od 47 min”) to w praktyce monitoring pracownika. Kodeks pracy (art. 22³ i 22⁴) wymaga określenia celu i zakresu monitoringu w regulaminie i poinformowania pracowników, a RODO wymaga podstawy prawnej i minimalizacji danych. Bez zgody działu prawnego/HR tego nie uruchamiać.
- Propozycja domyślna: PM widzi **status zadania** (w toku / ukończone / czekam na) i **sumę czasu na zadaniach, które sam zaplanował**. Nie widzi przedziałów, godzin pracy, aktywności agentów, okien ani zadań prywatnych.
- „Teraz pracuję nad…” jako **opcja włączana przez pracownika**, z widocznym wskaźnikiem w aplikacji, że jest udostępniane.
- Zadania lokalne (niezaplanowane przez PM) nigdy nie wychodzą z komputera, chyba że pracownik sam je udostępni.
- Retencja: dane statusów kasowane po np. 90 dniach, eksport i usunięcie danych na żądanie.

## Architektura

```
Panel web (Vue 3, ten sam zestaw komponentów co aplikacja: tablica tygodnia, pasek pojemności)
        │  OIDC (Entra ID)
        ▼
API (np. Rust axum albo .NET — do decyzji, liczy się utrzymanie w firmie)
        │  Postgres: organizacje, zespoły, zadania zaplanowane, statusy, zdarzenia synchronizacji
        │  Microsoft Graph: free/busy (w imieniu PM)
        ▼
Todo Track desktop: moduł sync (Rust)
        – pobiera zadania od PM i zapisuje je w lokalnym SQLite z polem remote_id
        – wysyła statusy przez kolejkę wychodzącą (outbox), działa offline
```

- **Offline-first**: aplikacja działa jak dziś bez sieci. Synchronizacja to dodatek: tabela `sync_outbox` z kolejnymi zmianami i `remote_id` na zadaniach.
- **Konflikty**: PM jest właścicielem pól planowania (dzień, kolejność, szacunek), pracownik pól wykonania (status, czas, notatki). Kolejność w dniu może zmienić pracownik, a PM to widzi. Prosta reguła „ostatni zapis wygrywa per pole” wystarczy na start.
- **Hosting**: Azure App Service + Azure Database for PostgreSQL w tenancie firmy (dane nie wychodzą poza firmową chmurę), albo serwer on-premise, jeśli firma tak woli.
- **Jira**: panel może tworzyć zadanie z klucza Jiry (tytuł z API firmy). Docelowo PM planuje zgłoszenia, które już są w Jirze, a Todo Track je dopasowuje, tak jak dziś robi to z kluczami.

## Etapy

| Etap | Zakres | Szacunek |
|------|--------|----------|
| 0 | Decyzje: prawo/HR (zakres widoczności), hosting, rejestracja aplikacji w Entra ID, stack backendu | poza kodem |
| 1 — MVP | Logowanie Entra, zespoły z grup, PM tworzy zadanie dla osoby na dzień, aplikacja pobiera i pokazuje „od PM”, przyjmij/odrzuć, status wraca do panelu | 2–3 tygodnie |
| 2 | Free/busy z Graph i pojemność w panelu, tablica tygodnia zespołu, kolejność ustalana przez PM | 1–2 tygodnie |
| 3 | Czas na zaplanowanych zadaniach i rozjazdy szacunków dla PM, powiadomienia o odrzuceniach i przeładowaniu, push zamiast odpytywania | 1–2 tygodnie |
| 4 | Opcjonalnie „teraz pracuję nad” (opt-in), raporty zespołu, eksport | 1 tydzień |

## Otwarte pytania

1. Czy firma ma już narzędzie do planowania (Jira Plans, Planner, Tempo)? Panel musi wnosić coś, czego one nie dają, np. dzienną kolejność plus realną pojemność z kalendarza plus zero ręcznego raportowania.
2. Kto zatwierdzi zakres widoczności danych: HR, prawnik, IOD?
3. Czy PM planuje na konkretne dni, czy tylko kolejkę („następne w kolejce”), a dzień wynika z pojemności?
4. Czy zadania od PM mają trafiać też do Jiry (dwukierunkowo), czy Jira zostaje źródłem prawdy, a panel tylko ustala kolejność i dzień?
5. Kto utrzymuje serwer po wdrożeniu?
