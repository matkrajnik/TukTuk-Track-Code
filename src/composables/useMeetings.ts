import { computed, onUnmounted, ref, watch } from "vue";
import {
  calendarEvents,
  calendarStatus,
  onCalendarChanged,
  type CalendarEvent,
} from "../store";

/**
 * Spotkania w zakresie `[from, to)` (ms): z kalendarza Microsoft przy
 * połączonym koncie i ręczne zawsze (`calendar_events`, B12). Odświeżane po
 * zmianie zakresu i po zdarzeniu `calendar-changed` (pobranie, połączenie,
 * rozłączenie, zmiana ręcznego spotkania). `connected` = konto połączone;
 * `shown` = tor albo sekcja spotkań ma sens (konto połączone albo jest choć
 * jedno spotkanie, np. ręczne bez kalendarza).
 */
export function useMeetings(range: () => [number, number]) {
  const events = ref<CalendarEvent[]>([]);
  const connected = ref(false);
  const shown = computed(() => connected.value || events.value.length > 0);
  let request = 0;
  async function load() {
    const id = ++request;
    try {
      const [from, to] = range();
      const [status, list] = await Promise.all([
        calendarStatus(),
        calendarEvents(from, to),
      ]);
      if (id !== request) return;
      connected.value = status.connected;
      events.value = list;
    } catch {
      if (id !== request) return;
      connected.value = false;
      events.value = [];
    }
  }
  // Porównanie po wartościach: nowa tablica z tym samym zakresem nie pobiera ponownie.
  watch(() => range().join(","), load, { immediate: true });
  let stop: (() => void) | undefined;
  let alive = true;
  void onCalendarChanged(() => void load())
    .then((s) => (alive ? (stop = s) : s()))
    .catch(() => undefined);
  onUnmounted(() => {
    alive = false;
    stop?.();
  });
  return { events, connected, shown, reload: load };
}
