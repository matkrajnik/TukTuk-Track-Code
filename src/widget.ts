/**
 * Układ okna malutkiego mini-widgetu (C2): komendy Tauri z odpowiednikami
 * w podglądzie. Osobny moduł, bo `store.ts` ma limit długości.
 *
 * W podglądzie okna widgetu nie ma: rozmiar strony ustawia przeglądarka (testy),
 * a stan malutki/powiększony trzyma sam komponent, więc obie funkcje to no-op.
 */
import { invoke } from "@tauri-apps/api/core";
import { desktop } from "./store";

/**
 * `widget_set_layout`: rozmiar i położenie okna dla widoku malutkiego,
 * powiększonego albo pełnego na stałe (`fullHeight` w pikselach logicznych).
 * Zwraca `true`, gdy Rust odmówił zwinięcia (otwarte okno ⇄) i okno zostało
 * powiększone. W podglądzie zawsze `false`.
 */
export async function widgetSetLayout(
  compact: boolean,
  expanded: boolean,
  fullHeight: number,
): Promise<boolean> {
  if (desktop)
    return invoke<boolean>("widget_set_layout", {
      compact,
      expanded,
      fullHeight,
    });
  // Podgląd: brak okna do zmiany, widok przełącza komponent.
  return false;
}

/**
 * `widget_can_collapse`: czy powiększony widget może się zwinąć. W aplikacji
 * `false` przy otwartym oknie szybkiego przełączania (⇄), które jest osobnym
 * oknem. W podglądzie `true`: okno ⇄ jest na stronie i sprawdza je komponent.
 */
export async function widgetCanCollapse(): Promise<boolean> {
  if (desktop) return invoke<boolean>("widget_can_collapse");
  return true;
}
