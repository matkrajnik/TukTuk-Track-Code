import {
  computed,
  onActivated,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
} from "vue";
import { tsSent, tsSettings, type TsSent, type TsSettings } from "../store";
import { useSettings } from "../target";

/**
 * Stan przycisku „Wyślij do TS” i znacznika wysyłki dnia (PLAN_1.8.md, T3):
 * przełącznik „Wysyłaj raport TS”, połączone konto (`ts_settings`) i ostatnia
 * wysyłka dnia (`ts_sent`). Niczego nie wysyła.
 *
 * Znacznik porównuje bieżącą sumę wpisów raportu (`tsEntries().totalHours`,
 * suma godzin per wpis) z sumą wysłanych wpisów raportu zapamiętaną lokalnie
 * (`sentReportKey`), liczoną tak samo. `ts_sent` zapisuje sumę odczytaną z TS
 * (przy „Zachowaj pozostałe wpisy TS” z wpisami spoza raportu), więc nie
 * nadaje się do porównania: bez lokalnego wpisu (inne urządzenie, wyczyszczona
 * pamięć) znacznik pokazuje samą wysyłkę, bez uwagi o zmianie raportu.
 */
export const sentReportKey = "todo-track-ts-sent-report-v1";

function readReport(): Record<string, string> {
  try {
    const value = JSON.parse(localStorage.getItem(sentReportKey) ?? "{}");
    return value && typeof value === "object" ? value : {};
  } catch {
    return {};
  }
}

/** Zapamiętuje sumę wysłanych wpisów raportu dnia (ostatnie 60 dni). */
export function rememberSentReport(day: string, hours: string) {
  try {
    const all = { ...readReport(), [day]: hours };
    const days = Object.keys(all).sort().slice(-60);
    localStorage.setItem(
      sentReportKey,
      JSON.stringify(Object.fromEntries(days.map((d) => [d, all[d]]))),
    );
  } catch {
    // Brak pamięci lokalnej: znacznik bez uwagi o zmianie raportu.
  }
}

const clock = (at: number) => {
  const d = new Date(at);
  const two = (n: number) => String(n).padStart(2, "0");
  return `${two(d.getDate())}.${two(d.getMonth() + 1)} ${two(d.getHours())}:${two(d.getMinutes())}`;
};

export function useTsSend(day: () => string, reportHours: () => string) {
  const settings = useSettings();
  const account = ref<TsSettings | null>(null);
  const sent = ref<TsSent | null>(null);
  const sentReport = ref("");
  let generation = 0;

  async function reload() {
    const current = ++generation;
    const target = day();
    const [nextAccount, nextSent] = await Promise.all([
      tsSettings().catch(() => null),
      tsSent(target).catch(() => null),
    ]);
    if (current !== generation) return;
    account.value = nextAccount;
    sent.value = nextSent;
    sentReport.value = readReport()[target] ?? "";
  }
  watch(day, () => void reload(), { immediate: true });
  // Zmiana konta lub przełącznika w innym widoku (Ustawienia, inne okno).
  const onFocus = () => void reload();
  onMounted(() => window.addEventListener("focus", onFocus));
  // Widok w KeepAlive (Raporty dzienne) po powrocie z Ustawień → Timesheet.
  onActivated(onFocus);
  onBeforeUnmount(() => window.removeEventListener("focus", onFocus));

  /** Przycisk widoczny: włączony przełącznik i połączone konto. */
  const enabled = computed(
    () => settings.value.tsSend !== false && account.value?.sendEnabled !== false,
  );
  const connected = computed(() => account.value?.connected === true);
  /** „Wysłano do TS 19.10 17:02 (7,50 h)”. */
  const marker = computed(() =>
    sent.value
      ? `Wysłano do TS ${clock(sent.value.sentAt)} (${sent.value.hours} h)`
      : "",
  );
  /** Suma wpisów raportu różni się od wysłanej (znanej tylko lokalnie). */
  const changed = computed(
    () =>
      !!sent.value && !!sentReport.value && sentReport.value !== reportHours(),
  );
  /** Po zapisie w TS: znacznik i zapamiętana suma wysłanych wpisów. */
  async function markSent(sentDay: string, hours: string) {
    rememberSentReport(sentDay, hours);
    await reload();
  }
  return { account, enabled, connected, sent, marker, changed, reload, markSent };
}
