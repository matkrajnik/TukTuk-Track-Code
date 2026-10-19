import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { desktop } from "./store";
import { onSettingsChanged, readSettings } from "./settings";

/**
 * Aktualizacje aplikacji (RELEASE.md): plugin updater pyta publiczne
 * repozytorium wydań o `latest.json`, a gdy jest nowsza wersja, główne okno
 * pokazuje baner (`UpdateBanner.vue`). Pobranie i instalacja zaczynają się
 * dopiero po kliknięciu; brak sieci przy automatycznym sprawdzeniu jest
 * ignorowany, tylko „Sprawdź teraz” w Ustawieniach pokazuje wynik.
 */
export interface AvailableUpdate {
  version: string;
  /** Notatki z wydania (`notes` w latest.json). */
  notes: string;
  /** Data publikacji (`pub_date`), pusta gdy brak. */
  date: string;
}
export type CheckStatus = "available" | "latest" | "error" | "unavailable";
export interface CheckResult {
  status: CheckStatus;
  /** Komunikat dla „Sprawdź teraz”. */
  message: string;
}
export interface InstallProgress {
  downloaded: number;
  /** Rozmiar pliku z nagłówka odpowiedzi; null, gdy serwer go nie podał. */
  total: number | null;
  /** Pobieranie zakończone, trwa instalacja. */
  finished: boolean;
}

export const connectionError = "Nie udało się połączyć z serwerem aktualizacji.";
export const latestMessage = "Masz najnowszą wersję.";

/** Znaleziona aktualizacja; null, gdy ostatnie sprawdzenie nie znalazło nowszej wersji. */
export const available = ref<AvailableUpdate | null>(null);
/** „Później”: baner schowany do następnego sprawdzenia, które znajdzie aktualizację. */
export const postponed = ref(false);
export const installing = ref<InstallProgress | null>(null);
export const installError = ref("");
export const checking = ref(false);

let update: Update | null = null;
let lastCheckAt = 0;
const DAY = 24 * 3_600_000;
const HOUR = 3_600_000;
/** Pierwsze sprawdzenie chwilę po starcie, żeby nie konkurować z wczytaniem danych. */
const STARTUP_DELAY = 2_000;

async function drop() {
  const old = update;
  update = null;
  await old?.close().catch(() => {});
}

/** Jedno zapytanie do serwera wydań; wynik trafia do `available` i do banera. */
export async function checkForUpdate(): Promise<CheckResult> {
  if (!desktop)
    return {
      status: "unavailable",
      message: "Sprawdzanie aktualizacji działa tylko w aplikacji desktopowej.",
    };
  checking.value = true;
  try {
    const found = await check({ timeout: 30_000 });
    lastCheckAt = Date.now();
    if (!found) {
      await drop();
      available.value = null;
      return { status: "latest", message: latestMessage };
    }
    if (update && update.rid !== found.rid) await drop();
    update = found;
    available.value = {
      version: found.version,
      notes: (found.body ?? "").trim(),
      date: found.date ?? "",
    };
    postponed.value = false;
    return {
      status: "available",
      message: `Dostępna jest wersja ${found.version}: zainstalujesz ją z banera u góry.`,
    };
  } catch (e) {
    // Brak sieci albo serwera: przy automatycznym sprawdzeniu bez komunikatu.
    lastCheckAt = Date.now();
    console.warn("Sprawdzanie aktualizacji nie powiodło się:", e);
    return { status: "error", message: connectionError };
  } finally {
    checking.value = false;
  }
}

/** „Później”: chowa baner do następnego sprawdzenia z nową wersją. */
export function postpone() {
  postponed.value = true;
}

/**
 * Pobiera i instaluje znalezioną aktualizację. Między pobraniem a instalacją
 * komenda `prepare_update` kończy pomiar i zapisuje stan okien, bo w Windows
 * plugin po uruchomieniu instalatora kończy proces od razu; instalator (tryb
 * pasywny, `/R`) uruchamia aplikację ponownie, a `relaunch()` jest dla
 * przypadku, gdy sterowanie jednak wróci.
 */
export async function installUpdate(): Promise<void> {
  if (!update || installing.value) return;
  installing.value = { downloaded: 0, total: null, finished: false };
  installError.value = "";
  try {
    await update.download((event) => {
      const current = installing.value;
      if (!current) return;
      if (event.event === "Started")
        installing.value = {
          downloaded: 0,
          total: event.data.contentLength ?? null,
          finished: false,
        };
      else if (event.event === "Progress")
        installing.value = {
          ...current,
          downloaded: current.downloaded + event.data.chunkLength,
        };
      else if (event.event === "Finished")
        installing.value = {
          ...current,
          downloaded: current.total ?? current.downloaded,
          finished: true,
        };
    });
    await invoke("prepare_update");
    await update.install();
    await relaunch();
  } catch (e) {
    installError.value = `Nie udało się zainstalować aktualizacji: ${String(
      e instanceof Error ? e.message : e,
    )}`;
    installing.value = null;
  }
}

/**
 * Automatyczne sprawdzanie w głównym oknie: chwilę po starcie i co godzinę
 * test, czy od ostatniego sprawdzenia minęła doba. Wyłącznik to ustawienie
 * `autoUpdateCheck`; zwraca funkcję zatrzymującą.
 */
export async function startAutoChecks(): Promise<() => void> {
  if (!desktop) return () => {};
  let enabled = true;
  try {
    enabled = (await readSettings()).autoUpdateCheck;
  } catch {
    /* domyślnie włączone */
  }
  const stopSettings = await onSettingsChanged((s) => {
    enabled = s.autoUpdateCheck;
  }).catch(() => () => {});
  const run = () => {
    if (enabled && !installing.value && Date.now() - lastCheckAt >= DAY)
      void checkForUpdate();
  };
  const first = setTimeout(run, STARTUP_DELAY);
  const timer = setInterval(run, HOUR);
  return () => {
    clearTimeout(first);
    clearInterval(timer);
    stopSettings();
  };
}
