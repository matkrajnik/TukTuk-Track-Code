# Plan nowego logo Todo Track (kiwi / tukan)

Stan: wdrożony wariant A (kiwi). `app-icon.svg` to kopia `design/logo/kiwi-a.svg`, ikony w `src-tauri/icons/`
i `icon-active.ico` zregenerowane (19.10.2026). Pozostałe warianty zostają w `design/logo/` do porównania.
Pliki wariantów i podgląd: `design/logo/`.

## 1. Gdzie logo jest używane i co z tego wynika

| Miejsce | Źródło | Rozmiar na ekranie |
| --- | --- | --- |
| Ikona okna i paska zadań Windows | `src-tauri/icons/icon.ico` (`tauri.conf.json` → `bundle.icon`) | 16–32 px (pasek zadań, Alt+Tab 32–48 px) |
| Tray, stan spoczynku | ikona okna (`default_window_icon` w `src-tauri/src/tray.rs`) | 16 px (24 px przy 150% DPI) |
| Tray, trwa pomiar | `src-tauri/icons/icon-active.ico` = `icon.png` + czerwona kropka, generuje `src-tauri/installer/make-artwork.ps1` | 16 px |
| Instalator NSIS | `icons/icon.ico` (`installerIcon`); `sidebar.bmp` i `header.bmp` rysuje osobno `make-artwork.ps1` (miętowe kółko z ptaszkiem) | 32–48 px |
| Nagłówek paska bocznego i topbar (`AppNav.vue`) | `src/components/BrandIcon.vue` – sylwetka z `design/logo/kiwi-mono.svg` w kolorze akcentu | 20 px |
| Mini-widget, pasek tytułu | nie rysują własnego logo (pasek tytułu to tekst „Todo Track”), dostają ikonę okna | 16 px |
| Sklep / MSIX | `Square*Logo.png`, `StoreLogo.png` | 30–310 px |

Wnioski: liczy się czytelność sylwetki w 16 px (tray, pasek zadań), a jedna ikona
służy w obu motywach Windows, więc kafelek musi odcinać się i od jasnego, i od ciemnego paska.

## 2. Koncepcja: kiwi, nie tukan

Rekomendacja: **kiwi**.

- Sylwetka kiwi to okrągły tułów i jeden długi dziób, czyli dwa duże kształty. To przeżywa
  skalowanie do 16 px (sprawdzone na klatkach ICO z `tauri icon`); tukan potrzebuje trzech
  kolorów (tułów, pierś, dziób) i oka, żeby nie wyglądać jak pingwin.
- Kiwi nie lata, chodzi i zostawia ślady; „Track” to śledzenie czasu, krok po kroku.
  Jest skromny i nocny, co pasuje do narzędzia, które pracuje po cichu w trayu.
- Tukan jest głośniejszy kolorystycznie (pomarańcz i żółć na dużym polu), co kłóci się
  z chłodną, stonowaną paletą interfejsu (granat, błękit `--accent`).
- Dziób kiwi zostaje pomarańczowy (`--tl-2`), żeby w 16 px był widoczny jako osobny kształt;
  w naturze jest kościany, ale kremowy dziób na kremowym tułowiu zlewałby się w plamę.

Tukan jest w zestawie jako wariant C, żeby porównać na żywo.

## 3. Paleta (z `src/styles/base.css`)

| Rola | Kolor | Pochodzenie |
| --- | --- | --- |
| Kafelek (A, mono) | `#1b2540` | między `--bg` `#111318` a `--accent-bg` `#1f2a44` motywu ciemnego |
| Sylwetka | `#f4f1e6` | krem z `icon-active.ico` (pierścień kropki) |
| Dziób | `#e6a86b` | `--tl-2` (oś dnia, motyw ciemny) |
| Kafelek B | `#2f8a5f` | `--tl-0` motywu jasnego; ciągłość z obecną zieloną ikoną `#2d654b` |
| Kafelek C | `#8fb3ff` | `--accent` motywu ciemnego |
| Żółty spód dzioba (C) | `#d9c65f` | `--tl-5` |
| Kropka pomiaru | `#e5484d` z obwódką `#f4f1e6` | jak dziś w `make-artwork.ps1` |

Zaokrąglenie kafelka: `rx=116` na 512 (22,6%), jak w obecnym `app-icon.svg`.

## 4. Warianty (`design/logo/`)

| Plik | Opis | Ocena w 16 px |
| --- | --- | --- |
| `kiwi-a.svg` | **A · Kiwi**: granatowy kafelek, kremowa sylwetka, pomarańczowy dziób, oko jako kropka tła | tułów i dziób czytelne, nogi ledwo, oko znika (zamierzone) |
| `kiwi-b.svg` | **B · Kiwi ze znacznikiem**: zielony kafelek, w tułowiu wycięty ptaszek ✓ | ptaszek widać jako nacięcie; w 16 px dominuje „✓ na plamie”, kiwi czytelne od 32 px |
| `tukan-c.svg` | **C · Tukan**: jasnoniebieski kafelek, granatowy tułów, kremowa pierś, dwukolorowy dziób | ciemna plama z pomarańczowym dziobem, oko znika; od 32 px czytelny tukan |
| `kiwi-mono.svg` | sylwetka A w jednym kolorze (`currentColor`), oko jako wycięcie | do traya w stylu ikon systemowych Windows 11 |
| `kiwi-mono-active.svg` | jak wyżej plus czerwona kropka pomiaru | odpowiednik `icon-active.ico` |

Wersje jasna i ciemna: kafelek pełnokolorowy jest jeden dla obu motywów (ikony aplikacji
w Windows nie zależą od motywu). Warianty mono zmieniają kolor przez `currentColor`:
czarny na jasnym pasku, biały na ciemnym (podgląd pokazuje oba).

Podgląd: `design/logo/preview.html` (16, 32, 64, 256 px na jasnym i ciemnym tle oraz rząd
traya z wariantami mono). Działa z pliku i przez dowolny serwer statyczny.

Wierny obraz małych rozmiarów: `design/logo/preview-ico.ps1` wycina klatki 16 i 32 px
z `icon.ico` wygenerowanego przez `tauri icon` i składa je powiększone 8× na jasnym i ciemnym pasku:

```powershell
npx tauri icon design/logo/kiwi-a.svg -o $env:TEMP\icons-kiwi-a
powershell -File design/logo/preview-ico.ps1 -Dirs "$env:TEMP\icons-kiwi-a" -Out $env:TEMP\ico-sheet.png
```

## 5. Ograniczenia projektowe

- Dwa kształty główne (tułów, dziób), bez piór, faktury, konturów ani cieni; każdy detal
  cieńszy niż ~30/512 (1 px w 16 px) znika przy skalowaniu i tak ma być (oko, przerwa między nogami).
- Dziób u podstawy ma ~60/512 (2 px w 16 px), nogi 34/512; cieńsze rozmywały się w 16 px.
- Sylwetka ma ≥ 32/512 marginesu od krawędzi kafelka (czubek dzioba), tułów ≥ 90/512, żeby zaokrąglone
  rogi jej nie ścinały. Środek optyczny: tułów stoi lekko na lewo od środka (ok. 237/512), bo długi dziób
  po prawej równoważy masę; dziób ma ok. 0,8 długości pierwszego szkicu, żeby nie dotykał krawędzi.
- Kontrast sylwetki do kafelka ≥ 4,5:1 (krem na granacie ≈ 13:1, krem na zieleni ≈ 4,6:1,
  granat na błękicie ≈ 7:1). Kafelek odcina się i od jasnego (`#eef0f4`), i od ciemnego (`#202228`) paska.
- Czyste SVG bez osadzonych rastrów, bez filtrów i gradientów (resvg w `tauri icon` je obsługuje,
  ale drobne gradienty w 16 px dają tylko szum).
- Czerwona kropka pomiaru zajmuje prawy dolny róg, więc żaden element sylwetki nie niesie tam informacji.

## 6. Wdrożenie wybranego wariantu

1. Skopiować wybrany plik na `app-icon.svg` w katalogu głównym (np. `design/logo/kiwi-a.svg`).
2. Wygenerować ikony: `npm run tauri icon app-icon.svg` (zapisuje do `src-tauri/icons/`, w tym
   `icon.ico`, `icon.png`, `icon.icns`, `Square*Logo.png`, `android/`, `ios/`).
3. Odtworzyć ikonę traya z kropką: `powershell -File src-tauri/installer/make-artwork.ps1`
   (buduje `icon-active.ico` z nowego `icon.png`; przy okazji przerysowuje `sidebar.bmp`
   i `header.bmp` instalatora).
4. Sprawdzić wynik skryptem `design/logo/preview-ico.ps1` na `src-tauri/icons` i w aplikacji:
   pasek zadań, tray w spoczynku i podczas pomiaru, Alt+Tab, instalator NSIS.
5. Zaktualizować README (sekcja o trayu wspomina ikonę z kropką) i wpis w `CHANGELOG`/wersji.

Do rozważenia osobno:
- `sidebar.bmp` instalatora nadal rysuje miętowe kółko z ptaszkiem w GDI+; można zastąpić je
  rastrem kiwi z `icon.png` (zmiana w `make-artwork.ps1`).
- Tray w stylu systemowym: Windows 11 używa ikon monochromatycznych w trayu; `kiwi-mono.svg`
  można wyeksportować do ICO (białą dla ciemnego paska, czarną dla jasnego) i przełączać
  w `tray.rs` według motywu. Dziś tray dostaje pełnokolorową ikonę okna i tak może zostać.
