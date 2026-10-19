# Todo Track — audyt UX (punkt wyjścia: 1.7.0)

Audyt z 16 października 2026, gałąź `claude/hungry-pare-c0cad4` od tagu `v1.7.0` (7a63d4d). Każda uwaga opiera się na zewnętrznym standardzie albo wzorcu z podobnego narzędzia i ma link do źródła. Zmian „według gustu” nie ma. Pełna lista przeczytanych źródeł (około 100, z oznaczeniem, które strony zostały otwarte) leży w notatkach sesji (`ux-research.md`); najważniejsze są przy uwagach i w sekcji „Inspiracje”.

## Zakres i metoda

- **Widoki:** Zadania; Dashboard; Plan i historia (Rano, Dzień, Tydzień → Tablica, Podsumowanie, Przegląd, Zamknięcie dnia); Raporty dzienne (Jeden dzień, Wybrane dni, Miesiąc); Historia i kopie; Jira (Lista, Tablica); Ustawienia; mini-widget; banery bezczynności, agentów AI, skupienia i Jiry; szybkie przełączanie (Ctrl+K); szybkie dodawanie.
- **Warunki:** szerokość 760 i 1280 px, a pasek dodatkowo 1000–1440 px; motyw ciemny i jasny; tryb podglądu i zastępcze IPC Tauri (pasek tytułu, Jira, bezczynność). Dane z `tests/fixtures/seed.ts` z trwającym pomiarem i połączonym kalendarzem.
- **Pomiary skryptem Playwright** (tymczasowy, niecommitowany), w każdym widoku i motywie:
  - kontrast każdego tekstu względem rzeczywistego tła, z uwzględnieniem przezroczystości;
  - rozmiar każdego elementu interaktywnego;
  - kontrast obramowań pól;
  - przejście samym klawiszem Tab z zapisem, czy fokus jest widoczny.
- **Kontrast tokenów** policzony wzorem z WCAG.
- **Wagi:**
  - krytyczna: blokuje wykonanie zadania albo niszczy dane;
  - wysoka: częsta czynność jest wyraźnie utrudniona albo naruszone jest kryterium WCAG A/AA w głównym przepływie;
  - średnia: naruszenie standardu poza głównym przepływem albo wyraźna niespójność;
  - niska: kosmetyka i drobne udogodnienia.

## Podsumowanie

Uwag krytycznych nie znaleziono: każdą funkcję da się wykonać myszą i klawiaturą, a przeciąganie ma alternatywę („Przenieś na…”, „Przesuń do…”).

| ID | Waga | Uwaga | Status |
|---|---|---|---|
| U1 | wysoka | Mierzone zadanie niewidoczne poza widokiem Zadania | wdrożone (6401946) |
| U2 | wysoka | Przyciski Start wyglądają na wyłączone (kontrast 2.67–3.77:1) | wdrożone (acea4c4) |
| U3 | wysoka | Akcje zapisujące wyglądają jak linki tekstowe, brak hierarchii przycisków | wdrożone (a06dcd3) |
| U4 | wysoka | Banery: jednakowe niebieskie tło, brak rodzaju komunikatu, akcje jako linki | wdrożone (a06dcd3) |
| U5 | wysoka | 11 przystanków Tab przed treścią; nawigacja jako przełączniki `aria-pressed` | wdrożone (6401946) |
| U6 | wysoka | Siedem pozycji w górnym pasku, mylące nazwy, przepełnienie paska | wdrożony wariant (a), lewy panel (dbdf426); nazwy bez zmian |
| U7 | średnia | Obramowania pól 1.25–1.52:1 | wdrożone (60a6e98, acea4c4) |
| U8 | średnia | Tekst z przezroczystością: zera, ukończone karty (2.35–3.30:1) | wdrożone (acea4c4) |
| U9 | średnia | Skala typografii poza rampą Fluent, tekst 11.5 px | wdrożone (60a6e98) |
| U10 | średnia | Obszary klikania poniżej 24 px | wdrożone (acea4c4) |
| U11 | średnia | Podwójny pasek u góry okna: 34 + 64 px | wdrożone (6401946) |
| U12 | średnia | Pasek przy 760 px: nawigacja 12.5 px przyklejona do krawędzi | wdrożone (6401946) |
| U13 | średnia | Niewidoczny fokus: wyszukiwanie ustawień, pola daty | wdrożone (acea4c4) |
| U14 | średnia | Odwrócona hierarchia nagłówków w Rano i Zamknięciu dnia | wdrożone (35510c5) |
| U15 | średnia | Ustawienia: opisy jako ściana tekstu, odstępy nie wiążą opisu z opcją | wdrożone (35510c5) |
| U16 | średnia | Przeładowany dzień w „Rano” sygnalizowany tylko kolorem paska | wdrożone (35510c5) |
| U17 | średnia | Tablica tygodnia nie ogłasza przeniesienia karty | wdrożone (f268b71) |
| U18 | średnia | Kolor ostrzeżenia w jasnym motywie 4.34:1 na tle `raised` | wdrożone (60a6e98) |
| U19 | średnia | Zaokrąglenia 7/10 px zamiast 4/8 px Windows 11 | wdrożone (60a6e98) |
| U20–U27 | niska | Patrz „Uwagi niskie” | opisane, niewdrożone |

## Uwagi szczegółowe

### U1. Mierzone zadanie niewidoczne poza Zadaniami (wysoka)

- **Miejsce:** wszystkie widoki poza Zadaniami, czyli Dashboard, Plan i historia, Raporty, Jira, Historia, Ustawienia.
- **Obserwacja:**
  - Pasek „Bieżący pomiar” jest tylko w widoku Zadania.
  - Na pozostałych widokach nie widać, że czas leci, ani na czym. Wyjątki: widok Jira (znacznik na karcie) i osobne okno mini-widgetu.
  - Żeby zatrzymać pomiar, trzeba wrócić do Zadań.
- **Standard:**
  - [Nielsen, heurystyka 1: widoczność stanu systemu](https://www.nngroup.com/articles/ten-usability-heuristics/).
  - Wzorzec stałego paska timera: [Toggl Track — Timer](https://support.toggl.com/en/articles/2527645-timer-mode) i [Clockify — śledzenie czasu](https://clockify.me/help/getting-started/track-your-time-as-regular-user), gdzie timer jest u góry każdej strony.
  - [Prawo Fittsa](https://lawsofux.com/fittss-law/): Stop powinien być blisko, a nie dwa kliknięcia dalej.
- **Zmiana:**
  - W pasku nawigacji, na każdym widoku poza Zadaniami, jest kropka, nazwa zadania, czas i przycisk Stop.
  - Kliknięcie nazwy wraca do Zadań.
  - Przy 1100–1439 px nazwa chowa się do podpowiedzi, żeby pasek się zmieścił; zostają kropka, czas i Stop.
  - Timer dalej zmienia się tylko po kliknięciu.

### U2. Start wygląda na wyłączony (wysoka)

- **Miejsce:** Zadania, karty niemierzonych zadań; dotyczy też zera czasu „00:00:00”.
- **Obserwacja:**
  - Kolumna z czasem i przyciskiem Start ma `opacity: 0.6`, a zero dodatkowo `opacity: 0.55`.
  - Zmierzony kontrast: Start 3.77:1 w ciemnym i 2.67:1 w jasnym motywie, zero 2.94 i 2.35:1.
  - Start to najczęstsza czynność w aplikacji, a wygląda jak przycisk wyłączony obok naprawdę wyłączonego „Dodaj zadanie”.
- **Standard:**
  - [WCAG 1.4.3 Contrast (Minimum)](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html): 4.5:1 dla tekstu. Zwolnienie dotyczy tylko nieaktywnych kontrolek.
  - [Nielsen, heurystyka 4: spójność](https://www.nngroup.com/articles/ten-usability-heuristics/): ten sam wygląd nie może znaczyć „wyłączone” i „aktywne”.
- **Zmiana:** usunięta przezroczystość. Start ma pełny kontrast przycisku drugorzędnego, a zero kolor `--muted` (6.6:1).

### U3. Brak hierarchii przycisków (wysoka)

- **Miejsce:**
  - Zamknięcie dnia: „Zapisz notatki dnia”, „Przenieś niedokończone na jutro”, „Kopiuj raport TS”, „Oznacz dzień jako sprawdzony”.
  - Tydzień → Przegląd: „Zapisz wnioski”.
  - Dzień: „Dodaj do planu”.
- **Obserwacja:**
  - Akcje zapisujące i kończące rytuał mają ten sam wygląd co linki pomocnicze: niebieski tekst 12.5 px bez tła i ramki.
  - Wyłączone wersje (np. „Zapisz wnioski” bez zmian) są prawie nieodróżnialne od zwykłego tekstu.
- **Standard:**
  - [Fluent 2 — kolor](https://fluent2.microsoft.design/color): kolor marki oszczędnie, dla głównych akcji (CTA).
  - [Windows — okna dialogowe](https://learn.microsoft.com/en-us/windows/apps/design/controls/dialogs-and-flyouts/dialogs): jedna wyróżniona akcja z konkretnym czasownikiem.
  - [Efekt von Restorff](https://lawsofux.com/von-restorff-effect/): wyróżniona ma być jedna rzecz.
  - [NN/g — nazwy poleceń](https://www.nngroup.com/articles/ui-copy/).
- **Zmiana:**
  - Akcja kończąca widok jest przyciskiem głównym z akcentem: „Oznacz dzień jako sprawdzony”, „Zapisz wnioski”.
  - Kroki pośrednie są przyciskami drugorzędnymi z ramką 3:1.
  - Linki tekstowe zostały dla nawigacji i drobnych akcji.

### U4. Banery bez rodzaju komunikatu i z akcjami jako linkami (wysoka)

- **Miejsce:** banery bezczynności, agentów AI, końca bloku skupienia i przerwy, przesunięcia zgłoszenia w Jirze.
- **Obserwacja:**
  - Wszystkie cztery wyglądają tak samo: pełne niebieskie tło z niebieską ramką.
  - Bezczynność, która dotyczy poprawności zapisanego czasu, nie różni się od propozycji agenta.
  - Akcje to równorzędne linki 12.5 px, np. „Przełącz”, „Nie teraz”, „Nie proponuj dla tego folderu”, „Zapamiętaj dla tego folderu”. Nie widać, która jest odpowiedzią główną.
- **Standard:**
  - [Windows InfoBar](https://learn.microsoft.com/en-us/windows/apps/design/controls/infobar): rodzaje Informational, Success, Warning i Error; ikona i kolor nigdy nie są jedynym sygnałem; opcjonalny przycisk akcji.
  - [Fluent 2 — kolor](https://fluent2.microsoft.design/color): marka nie na dużych powierzchniach.
  - [NN/g — wskaźniki, walidacje i powiadomienia](https://www.nngroup.com/articles/indicators-validations-notifications/): natarczywość dopasowana do pilności.
  - [Prawo Hicka](https://lawsofux.com/hicks-law/).
- **Zmiana:**
  - Neutralna powierzchnia z paskiem 4 px i ikoną 20 px. Bezczynność ma rodzaj „ostrzeżenie” (żółty, zegar), pozostałe „informacja” (akcent; ikony robota, timera, filiżanki, biletu).
  - Główna odpowiedź jest przyciskiem z akcentem: „Przełącz”, „Przesuń”, „Przerwa”, „Rozpocznij”.
  - Równorzędne odpowiedzi to przyciski drugorzędne: dwie odpowiedzi baneru bezczynności, przejścia Jiry.
  - „Nie teraz” i „Nie proponuj…” to przyciski bez tła.
  - Role ARIA i etykiety bez zmian (`alert` dla bezczynności, nazwane regiony).

### U5. Klawiatura: 11 przystanków przed treścią, nawigacja jako przełączniki (wysoka)

- **Miejsce:** każdy widok.
- **Obserwacja:**
  - Przejście klawiszem Tab: marka, 7 pozycji nawigacji, szybkie przełączanie, mini-widget i motyw, a dopiero potem treść. W aplikacji desktopowej dochodzą jeszcze 3 przyciski okna i „Schowaj do traya”.
  - Pozycje nawigacji miały `aria-pressed`, więc czytnik ogłaszał je jako przełączniki „wciśnięty / niewciśnięty”, a nie jako bieżącą stronę.
- **Standard:**
  - [WCAG 2.4.1 Bypass Blocks](https://www.w3.org/WAI/WCAG22/Understanding/bypass-blocks.html) (poziom A).
  - [Windows — dostępność klawiatury](https://learn.microsoft.com/en-us/windows/apps/design/accessibility/keyboard-accessibility).
  - Stan bieżącej strony to `aria-current="page"`; przełączniki z `aria-pressed` opisuje [APG](https://www.w3.org/WAI/ARIA/apg/patterns/tabs/).
  - [NN/g — zakładki](https://www.nngroup.com/articles/tabs-used-right/): wybrana pozycja ma co najmniej dwa sygnały.
- **Zmiana:**
  - Link „Przejdź do treści” jako pierwszy przystanek, widoczny po fokusie.
  - `aria-current="page"` w nawigacji.
  - Wybrana pozycja ma podkreślenie i pogrubienie.
  - Testy sprawdzające `aria-pressed` nawigacji przeszły na `aria-current`. Etykiety się nie zmieniły.

### U6. Nawigacja główna: siedem pozycji i mylące nazwy (wysoka)

- **Miejsce:** pasek nawigacji.
- **Obserwacja:**
  - Siedem pozycji z ikonami nie mieściło się w pasku przy 1180 px w podglądzie: przepełnienie o około 20 px, ucięta ikona motywu.
  - Poniżej 1100 px nawigacja schodziła do drugiego rzędu przyklejonego do dolnej krawędzi paska.
  - Nazwy się nakładają:
    - „Raporty dzienne” zawiera też Miesiąc i Wybrane dni;
    - „Plan i historia” i „Historia i kopie” mają to samo słowo;
    - „Dashboard” jest jedyną angielską nazwą w polskim interfejsie.
- **Standard:**
  - [Windows NavigationView](https://learn.microsoft.com/en-us/windows/apps/design/controls/navigationview): nawigacja górna dla 5 lub mniej równorzędnych kategorii, lewa dla 5–10. Lewa zwija się do ikon przy 641–1007 px.
  - [NN/g — zakładki](https://www.nngroup.com/articles/tabs-used-right/): mało pozycji, 1–2 słowa, proste nazwy.
  - [Nielsen, heurystyka 2: język użytkownika](https://www.nngroup.com/articles/ten-usability-heuristics/).
  - [Prawo Millera](https://lawsofux.com/millers-law/) i [Prawo Jakoba](https://lawsofux.com/jakobs-law/): użytkownicy Windows znają lewy panel z Ustawień, Outlooka i Teams.
- **Pierwszy krok (6401946), bez zmiany sposobu pracy:**
  - Pasek 48 px.
  - Przy 1100–1439 px etykiety bez ikon, poniżej 1100 px dwa rzędy 48 + 40 px.
- **Propozycja przedstawiona użytkownikowi:**
  - (a) Lewy panel nawigacji według NavigationView: rozwinięty od 1008 px, same ikony poniżej, Ustawienia na dole panelu jak w Windows.
  - (b) Albo zostaje pasek górny, ale z 5 pozycjami i polskimi nazwami. Przykład:
    - „Zadania”;
    - „Przegląd” zamiast „Dashboard”;
    - „Plan”;
    - „Raporty” zamiast „Raporty dzienne”;
    - „Jira”;
    - „Historia i kopie” przeniesione do Ustawień albo do menu „Więcej”;
    - „Ustawienia” w prawym rogu jako ikona z etykietą.
  - Wariant (b) zmienia widoczne nazwy używane w testach (np. `getByRole("button", { name: "Raporty dzienne" })`), więc testy trzeba by przepisać.
- **Decyzja użytkownika (19 października 2026): wariant (a). Wdrożone w dbdf426:**
  - Od 1008 px panel rozwinięty: 240 px, ikona 20 px i etykieta 14 px, pozycje 40 px.
  - Przy 641–1007 px panel z samymi ikonami (LeftCompact, 56 px). Nazwa jest w podpowiedzi i w dostępnej nazwie przycisku.
  - Poniżej 641 px panel jest schowany (LeftMinimal). Przycisk menu w pasku otwiera go nad treścią: fokus trafia na pierwszą pozycję, Esc albo wybór pozycji zamyka panel, a fokus wraca na przycisk menu ([APG — Dialog](https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/)).
  - Przycisk menu na górze panelu zwija i rozwija go ręcznie, a wybór zostaje zapamiętany.
  - Ustawienia stoją na dole panelu jak w Windows.
  - Wybrana pozycja ma pionową kreskę akcentu 3×16 px i tło. NavigationView pokazuje jeden wskaźnik wyboru.
  - Pasek 48 px nad widokiem zostaje dla mierzonego zadania i akcji (szybkie przełączanie, mini-widget, motyw, tray).
  - Pasek tytułu jest przyklejony u góry okna, a panel stoi pod nim.
  - Nazwy pozycji się nie zmieniły. Zmiana nazw (np. „Dashboard” na polską nazwę, „Raporty dzienne” na „Raporty”) jest osobną decyzją. Nie wdrożyłem jej, bo nazwy są używane w testach i w README.
  - Nowy test `tests/navigation.spec.ts` sprawdza trzy tryby, zapamiętanie i klawiaturę. Przy 480, 640, 760, 1007, 1008, 1180 i 1280 px (podgląd i aplikacja, pięć widoków) strona nie przewija się w poziomie.

### U7. Obramowania pól (średnia)

- **Obserwacja:** obramowanie pól tekstowych, list i obszarów tekstu to `--line`: 1.38:1 do tła karty, 1.52:1 do tła strony w ciemnym motywie, 1.26–1.36:1 w jasnym.
- **Standard:**
  - [WCAG 1.4.11 Non-text Contrast](https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html): granica kontrolki potrzebna do jej rozpoznania ma mieć 3:1.
  - [Radix Colors](https://www.radix-ui.com/colors/docs/palette-composition/understanding-the-scale): osobny stopień skali dla ramek kontrolek.
- **Zmiana:**
  - Nowy token `--line-strong`: #6b7386 w ciemnym (3.55:1 do karty) i #80879a w jasnym (3.59:1 do białego).
  - Używają go pola, listy, obszary tekstu, wyszukiwanie ustawień i ramka przycisków drugorzędnych.
  - Karty zostały przy delikatnym `--line`, bo są dekoracją, a nie kontrolką.

### U8. Tekst z przezroczystością (średnia)

- **Obserwacja:** zmierzony kontrast elementów z `opacity`:
  - zera w Dashboardzie i raporcie miesiąca: 3.24 i 2.58:1;
  - ukończone karty tablicy tygodnia (`opacity: 0.65`): 3.30:1 w ciemnym i 2.64:1 w jasnym;
  - napis „Przenieś na…” na nich: 2.73:1.
- **Standard:** [WCAG 1.4.3](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html). Ukończona karta nadal jest aktywna: można ją przenieść.
- **Zmiana:** zamiast przezroczystości kolor `--muted` i przekreślenie tytułu.

### U9. Typografia poza rampą Fluent (średnia)

- **Obserwacja:**
  - Rozmiary 11.5 / 12.5 / 13.5 / 14.5 / 17 / 22 px nie odpowiadają żadnej rampie.
  - 11.5 px jest poniżej minimum Windows (12 px).
  - Krój „Segoe UI” bez wariantu Variable z Windows 11.
- **Standard:** [Typografia Windows](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/typography): Caption 12, Body 14, Subtitle 20, Title 28; minimum 12 px; Segoe UI Variable trzeba nazwać wprost. Webowa rampa [Fluent 2](https://fluent2.microsoft.design/typography) ma też 16 i 24 px.
- **Zmiana:**
  - Tokeny: 12 (xs i sm), 14 (base i md), 16 (lg), 24 (xl, tytuły widoków), 28 (2xl, duże liczby).
  - Interlinia 1.43 (20/14).
  - „Segoe UI Variable Text” jako pierwszy krój.

### U10. Obszary klikania poniżej 24 px (średnia)

- **Obserwacja:** zmierzone rozmiary:

  | Element | Rozmiar |
  |---|---|
  | „Przenieś na…” na kartach tygodnia | 78×19 |
  | Pole „Ukończ” w kartach zadań | 19×19 |
  | Uchwyt przeciągania | 22×22 |
  | Dni w tabeli Przeglądu tygodnia | 16 px wysokości |
  | „Otwórz w Jirze” na kartach tablicy | 23 px wysokości |
  | Rozwijane „Niedokończone z wcześniejszych planów” | 18 px wysokości |

- **Standard:**
  - [WCAG 2.5.8 Target Size (Minimum)](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html): 24×24 px.
  - [Windows — targeting](https://learn.microsoft.com/en-us/windows/apps/design/input/guidelines-for-targeting): około 40×40 px dla częstych celów.
- **Zmiana:**
  - Linki tekstowe i elementy rozwijane mają co najmniej 24 px.
  - Uchwyt ma 24 px.
  - Pole „Ukończ” ma 20 px z obszarem klikania 26 px.
  - Przyciski tekstowe i drugorzędne mają 32 px (standardowa wysokość przycisku Fluent).
  - Natywne pola wyboru 13 px w Ustawieniach zostały: klikalna jest cała etykieta.

### U11. Podwójny pasek u góry okna (średnia)

- **Obserwacja:** w aplikacji desktopowej pasek tytułu (34 px, tylko napis „Todo Track”) i pasek nawigacji (64 px) zajmowały 98 px, czyli 12% wysokości okna 800 px.
- **Standard:** [Pasek tytułu Windows](https://learn.microsoft.com/en-us/windows/apps/design/basics/titlebar-design) ma 32 px. Górny pasek [NavigationView](https://learn.microsoft.com/en-us/windows/apps/design/controls/navigationview) ma 48 px.
- **Zmiana:** pasek tytułu 32 px, a nad treścią pasek poleceń 48 px. Od dbdf426 nawigacja jest w lewym panelu (U6), więc nad treścią zostaje 80 px.
- **Dalszy krok (niewdrożony):** wpuszczenie paska poleceń w pasek tytułu (jeden pasek 48 px). To zmienia obszary przeciągania okna w Tauri i wymaga sprawdzenia w natywnej aplikacji.

### U12. Pasek przy 760 px (średnia)

- **Obserwacja:**
  - Poniżej 1100 px nawigacja schodziła do drugiego rzędu bez własnej wysokości, przyklejona do dolnej krawędzi.
  - Etykiety zmniejszały się do 12.5 px.
- **Standard:**
  - [Fluent 2 — layout](https://fluent2.microsoft.design/layout): siatka 4 px, odstępy z rampy.
  - [Prawo bliskości](https://lawsofux.com/law-of-proximity/).
- **Zmiana:**
  - Najpierw (6401946): rząd 48 px (marka, mierzone zadanie, akcje) i rząd 40 px z nawigacją, etykiety 14 px.
  - Od dbdf426 przy 760 px panel ma same ikony (U6) i drugi rząd nie jest potrzebny.

### U13. Niewidoczny fokus (średnia)

- **Obserwacja:**
  - Pole „Szukaj ustawienia” miało `outline: none`, a fokus sygnalizowała tylko zmiana koloru ramki 1 px.
  - Przycisk kalendarza wewnątrz pól daty i godziny nie dostawał pierścienia.
- **Standard:** [WCAG 2.4.7 Focus Visible](https://www.w3.org/WAI/WCAG22/Understanding/focus-visible.html); cel [2.4.13](https://www.w3.org/WAI/WCAG22/Understanding/focus-appearance.html) to obwódka co najmniej 2 px.
- **Zmiana:** pierścień 2 px jak w reszcie aplikacji, także na `:focus-within` pól daty, godziny i wyszukiwania.

### U14. Odwrócona hierarchia nagłówków (średnia)

- **Obserwacja:** w Rano i Zamknięciu dnia nagłówki kroków (h3, około 17 px, pogrubione) były większe niż tytuł karty (h2, 14.5 px).
- **Standard:** [Windows — content basics](https://learn.microsoft.com/en-us/windows/apps/design/basics/content-basics): nagłówki sekcji Body Strong, czyli 14 px półgrube.
- **Zmiana:** tytuł karty 16 px, nagłówki kroków 14 px półgrube.

### U15. Ustawienia: ściana tekstu (średnia)

- **Obserwacja:**
  - Opisy opcji mają do pięciu wierszy na pełną szerokość karty (około 120 znaków w wierszu).
  - Opis stoi w tej samej odległości od swojej opcji co od następnej, więc nie widać, do której należy.
- **Standard:**
  - [Prawo bliskości](https://lawsofux.com/law-of-proximity/).
  - [Typografia Windows](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/typography): 50–60 znaków w wierszu.
  - [NN/g — progressive disclosure](https://www.nngroup.com/articles/progressive-disclosure/).
- **Zmiana:**
  - Opis wcięty pod etykietę, 2 px od niej, a 16 px przed następną opcją.
  - Linia do 72 znaków, pola wyboru 16 px, etykiety 14 px.
- **Niewdrożone:** zwijanie długich opisów (np. rozszerzenie przeglądarki) pod „Więcej”. Zmienia treść, a testy szukają tych zdań.

### U16. Przeładowany dzień tylko kolorem (średnia)

- **Obserwacja:**
  - W Rano → Dziś pasek pojemności robi się czerwony, a obok widać „Plan 10:00 · … zostaje 6:45”.
  - Że plan przekracza czas, trzeba wyliczyć samemu.
- **Standard:**
  - [WCAG 1.4.1 Use of Color](https://www.w3.org/WAI/WCAG22/Understanding/use-of-color.html).
  - [Sunsama — planowanie dnia](https://help.sunsama.com/docs/usage-guides/daily-planning): kontrola przewidywanego obciążenia z ostrzeżeniem.
- **Zmiana:** przy planie dłuższym niż czas na pracę pojawia się zdanie z ikoną: „Plan jest o 3:15 dłuższy niż czas na pracę. Przenieś część zadań…”.

### U17. Brak ogłoszenia po przeniesieniu karty (średnia)

- **Obserwacja:** po przeniesieniu karty na tablicy tygodnia (przeciągnięciem albo „Przenieś na…”) czytnik ekranu nic nie słyszy.
- **Standard:**
  - [Atlassian — wytyczne dostępności drag and drop](https://atlassian.design/components/pragmatic-drag-and-drop/accessibility-guidelines/): ogłoszenie w regionie na żywo, skąd i dokąd.
  - [WCAG 4.1.3 Status Messages](https://www.w3.org/WAI/WCAG22/Understanding/status-messages.html).
- **Zmiana:**
  - Ukryty komunikat `role="status"`, np. „„Code review” przeniesione z: Do zaplanowania na: czw., 15 paź.”.
  - W wąskim oknie kolumny dociągają się do krawędzi (scroll-snap).

### U18. Kolor ostrzeżenia w jasnym motywie (średnia)

- **Obserwacja:** `--warning` #9a6400 miał 4.34:1 na tle `--raised`, np. „00:30 · 200%” na kartach tygodnia.
- **Standard:** [WCAG 1.4.3](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html).
- **Zmiana:** #8a5a00 (5.15:1 na `raised`, 5.93:1 na białym).

### U19. Zaokrąglenia (średnia)

- **Obserwacja:** kontrolki miały 7 px, karty 10 px, a odznaki 4 px.
- **Standard:**
  - [Geometria Windows 11](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/geometry): kontrolki 4 px, okna, menu wysuwane i karty 8 px.
  - [Fluent 2 — kształty](https://fluent2.microsoft.design/shapes): odznaki 2 px.
  - [Prawo Jakoba](https://lawsofux.com/jakobs-law/): aplikacja ma wyglądać jak reszta systemu.
- **Zmiana:** 2 / 4 / 8 px.

## Uwagi niskie (opisane, niewdrożone)

- **U20. Formaty dat.**
  - Obserwacja: ISO „2026-10-15” (np. „Uzupełnij opis pracy z 2026-10-15”, „Największe rozjazdy … 2026-10-12”) obok „czw., 15 paź”.
  - Standard: [heurystyka 4](https://www.nngroup.com/articles/ten-usability-heuristics/).
  - Propozycja: format polski wszędzie poza raportem TS. Niewdrożone, bo teksty są w asercjach testów.
- **U21. „Dodaj zadanie” wyłączony przy pustym polu.** Zgodne z zasadą zapobiegania błędom, ale [NN/g o placeholderach](https://www.nngroup.com/articles/form-design-placeholders/) zaleca widoczną etykietę zamiast samego „Nowe zadanie…”. Pole ma `aria-label`, więc WCAG 3.3.2 jest spełnione programowo.
- **U22. Legenda składni szybkiego dodawania.**
  - Wzorzec: [Akiflow](https://product.akiflow.com/help/articles/6483573-command-bar) pokazuje legendę tokenów, a [Todoist](https://todoist.com/help/articles/use-task-quick-add-in-todoist-va4Lhpzz) pozwala kliknąć rozpoznany token, żeby go cofnąć.
  - Todo Track pokazuje rozpoznane chipy; brakuje ściągi składni.
- **U23. Skróty w stopce szybkiego przełączania.** Stopka ma już „↑↓ wybór · Enter uruchamia · Esc zamyka”. [Raycast](https://www.raycast.com/blog/a-fresh-look-and-feel) dodaje pasek akcji kontekstowych. Do rozważenia przy rozbudowie palety.
- **U24. Mini-widget.** Przy 320 px nazwa zadania ucina się po 12 znakach („Analiza imp…”). Pełna nazwa jest w podpowiedzi. [WCAG 1.4.10](https://www.w3.org/WAI/WCAG22/Understanding/reflow.html) jest spełnione, bo nic nie przewija się w dwóch osiach.
- **U25. Pola opisów w Ustawieniach** nie są połączone z opcją przez `aria-describedby` (czytnik nie odczyta opisu przy fokusie). Wymaga identyfikatorów dla około 40 opisów; do zrobienia razem z U15.
- **U26. Lista „priorytet” w karcie zadania** to lista bez ramki (1.25:1). Rozpoznaje ją strzałka, więc [1.4.11](https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html) jest spełnione. Zostawione jako „subtelna” lista Fluent.
- **U27. Skróty jednoklawiszowe.** [Toggl](https://support.toggl.com/en/articles/2527645-timer-mode) ma N i S, a [Notion Calendar](https://www.notion.com/help/notion-calendar-keyboard-shortcuts) T dla „dziś” i J/K dla okresów. Todo Track ma Ctrl+K, Ctrl+, i skróty globalne. Do rozważenia: T = Dzisiaj w Planie i Raportach oraz `?` z listą skrótów.

## Inspiracje: co przenoszę i dlaczego

| Źródło | Co konkretnie | Gdzie | Dlaczego |
|---|---|---|---|
| [Toggl Track — Timer](https://support.toggl.com/en/articles/2527645-timer-mode), [Clockify](https://clockify.me/help/getting-started/track-your-time-as-regular-user) | Stały pasek mierzonego zadania ze Stop na każdej stronie | U1, pasek nawigacji | Najczęstsza czynność trackera ma być zawsze w zasięgu (heurystyka 1, Fitts) |
| [Windows InfoBar](https://learn.microsoft.com/en-us/windows/apps/design/controls/infobar) | Pasek i ikona rodzaju, neutralne tło, jeden przycisk akcji | U4, banery | Oficjalny wzorzec Windows dla trwałych komunikatów w aplikacji |
| [Linear — „A calmer interface”](https://linear.app/now/behind-the-latest-design-refresh), [„How we redesigned the Linear UI”](https://linear.app/now/how-we-redesigned-the-linear-ui) | Mniej koloru marki w ramach, neutralne powierzchnie, akcent tylko dla wyróżnionej akcji | U3, U4 (banery bez niebieskiego tła), mniej ikon przy ciasnym pasku | Spokojniejsze otoczenie zostawia uwagę na timerze i planie dnia |
| [Typografia](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/typography) i [geometria Windows 11](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/geometry), [Fluent 2 — layout](https://fluent2.microsoft.design/layout) | Rampa 12/14/16/24/28, Segoe UI Variable, zaokrąglenia 4/8, siatka 4 px | U9, U19 | Aplikacja ma wyglądać jak program Windows (prawo Jakoba) |
| [Windows NavigationView](https://learn.microsoft.com/en-us/windows/apps/design/controls/navigationview) | Lewy panel w trzech trybach (rozwinięty od 1008 px, same ikony przy 641–1007 px, schowany poniżej), Ustawienia na dole, pasek 48 px | U6, U11, U12 | Siedem pozycji przekracza zalecenie dla nawigacji górnej |
| [Radix Colors — skala](https://www.radix-ui.com/colors/docs/palette-composition/understanding-the-scale) | Osobny stopień neutralny dla ramek kontrolek | U7 (`--line-strong`) | Rozdziela dekoracyjne linie kart od granic pól (WCAG 1.4.11) |
| [Atlassian — drag and drop](https://atlassian.design/components/pragmatic-drag-and-drop/accessibility-guidelines/) | „Move to…” w menu karty (było) i ogłoszenie przeniesienia | U17 | Wzorzec z tablicy Jiry, którą użytkownicy znają |
| [Sunsama — planowanie dnia](https://help.sunsama.com/docs/usage-guides/daily-planning) | Słowne ostrzeżenie o przeładowanym planie | U16 | Rytuał „Rano” jest wzorowany na tym samym procesie |
| [MDN — tabular-nums](https://developer.mozilla.org/en-US/docs/Web/CSS/font-variant-numeric) | Cyfry tej samej szerokości w czasie | czas w pasku (U1) | Licznik nie skacze co sekundę (reszta aplikacji już to miała) |
| [Behance — Structured Daily Planner](https://www.behance.net/gallery/180788757/Structured-Daily-Planner-CaseStudy?locale=en_US), [Mobbin — time tracker](https://mobbin.com/explore/web/screens/time-tracker), [Dribbble — time tracking](https://dribbble.com/search/time-tracking-dashboard) | Przegląd porównawczy | — | Tylko inspiracja estetyczna, nie dowód użyteczności. Potwierdza kierunek: jeden akcent i timer połączony z planem |

## Zrzuty wzorcowe

Wszystkie wzorce w `tests/visual.spec.ts-snapshots` zostały świadomie zregenerowane (`npm run test:visual:update`, Windows 11, lokalnie). Zmiany dotyczą każdego widoku: typografia, zaokrąglenia, pasek 48 px, ramki pól, a w widokach innych niż Zadania także mierzone zadanie w pasku. Wzorce widoków zregenerowałem jeszcze raz w dbdf426 dla lewego panelu nawigacji; widget się nie zmienił. Lista:

- **Widoki w motywie ciemnym i jasnym, przy 760 i 1280 px, po 4 pliki na widok (32 pliki):** `zadania`, `dashboard`, `plan-dzien`, `plan-tablica-tygodnia`, `zamkniecie-dnia`, `ustawienia`, `jira`, `jira-tablica`.
- **Mini-widget (2 pliki):** `widget-dark`, `widget-light`.

Żaden z nich nie był potwierdzony na windows-latest w CI.

## Weryfikacja

Wyniki po dbdf426 (lewy panel):

- `npm run build`: przechodzi (vue-tsc i Vite).
- `cargo fmt --check`: przechodzi.
- `cargo test --locked`: 234 z 234 testów.
- Playwright z `--workers=2`, bez zrzutów (`--grep-invert @visual`): 219 z 219, bez powtórek, plus 3 z 3 w nowym `tests/navigation.spec.ts`.
- Zrzuty wzorcowe (`tests/visual.spec.ts`) po świadomej regeneracji: 34 z 34.
- Razem 256 z 256 (253 z 1.7.0 i 3 nowe).
- Ponowny pomiar audytu po zmianach:
  - w żadnym widoku i motywie nie ma tekstu poniżej 4.5:1;
  - poza wyjątkami opisanymi w U10 i U26 nie ma obszaru klikania poniżej 24 px;
  - strona nie przewija się w poziomie przy 480, 640, 760, 1007, 1008, 1180 i 1280 px, w podglądzie i w aplikacji.

Nie sprawdzono w natywnej aplikacji (WebView2):

- pasek tytułu 32 px i przeciąganie okna;
- przyklejony pasek tytułu i panel nawigacji przy przewijaniu oraz `:has()` w WebView2;
- Segoe UI Variable przy różnych DPI;
- wygląd mierzonego zadania w pasku po zmianie skali Windows.
