import { onUnmounted, ref, watch } from "vue";
import { agentTime, desktop, previewAgentsEvent, type Session } from "../store";
import { emptyAgentTime, type AgentTime } from "../agenttime";

/** Co ile aplikacja odświeża czas agentów (pętla agentów zapisuje co 5 s). */
const REFRESH = 60_000;

/** Podpis przedziałów pomiaru: zmiana przelicza część „Równolegle”. */
export const sessionsSignature = (sessions: readonly Session[]) =>
  sessions.map((s) => `${s.id}:${s.taskId}:${s.startedAt}:${s.endedAt ?? ""}`).join(",");

/**
 * Czas agentów w zakresie `[from, to)` (ms): okresy i sumy według zadania
 * (`agent_time`). Odświeżany po zmianie zakresu albo przedziałów pomiaru
 * (`version`), co minutę w aplikacji i po zmianie przypisań folderów w podglądzie.
 * Błąd daje pusty wynik: tor i kolumny agentów się nie pokazują.
 */
export function useAgentTime(range: () => [number, number], version?: () => string) {
  const time = ref<AgentTime>(emptyAgentTime());
  let request = 0;
  async function load() {
    const id = ++request;
    try {
      const [from, to] = range();
      const next = await agentTime(from, to);
      if (id === request) time.value = next;
    } catch {
      if (id === request) time.value = emptyAgentTime();
    }
  }
  watch(() => `${range().join(",")}|${version?.() ?? ""}`, load, {
    immediate: true,
  });
  const timer = desktop ? window.setInterval(load, REFRESH) : undefined;
  const reload = () => void load();
  if (!desktop) window.addEventListener(previewAgentsEvent, reload);
  onUnmounted(() => {
    if (timer !== undefined) window.clearInterval(timer);
    if (!desktop) window.removeEventListener(previewAgentsEvent, reload);
  });
  return { time, reload: load };
}
