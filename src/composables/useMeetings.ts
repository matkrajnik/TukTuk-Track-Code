import { onUnmounted, ref, watch } from "vue";
import {
  calendarEvents,
  calendarStatus,
  onCalendarChanged,
  type CalendarEvent,
} from "../store";

/**
 * Spotkania z kalendarza Microsoft w zakresie `[from, to)` (ms), odświeżane po
 * zmianie zakresu i po zdarzeniu `calendar-changed` (pobranie, połączenie,
 * rozłączenie). `connected` = konto połączone; bez niego lista jest pusta,
 * a widoki nie pokazują toru ani sekcji spotkań.
 */
export function useMeetings(range: () => [number, number]) {
  const events = ref<CalendarEvent[]>([]);
  const connected = ref(false);
  let request = 0;
  async function load() {
    const id = ++request;
    try {
      const status = await calendarStatus();
      const [from, to] = range();
      const list = status.connected ? await calendarEvents(from, to) : [];
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
  return { events, connected, reload: load };
}
