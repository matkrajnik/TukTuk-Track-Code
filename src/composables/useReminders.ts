import { computed, readonly, ref } from "vue";
import { onDataChanged } from "../store";
import {
  effectiveAt,
  onRemindersChanged,
  reminderList,
  type Reminder,
} from "../remindersapi";

const list = ref<Reminder[]>([]);
let started = false;
async function load() {
  try {
    list.value = await reminderList();
  } catch {
    // Bez listy kafelki po prostu nie mają znacznika 🔔.
  }
}
/** Najbliższe włączone przypomnienie każdego zadania (znacznik 🔔 na kafelku). */
const byTask = computed(() => {
  const map = new Map<number, Reminder>();
  for (const r of list.value) {
    if (!r.enabled || r.taskId === null) continue;
    const have = map.get(r.taskId);
    if (!have || effectiveAt(r) < effectiveAt(have)) map.set(r.taskId, r);
  }
  return map;
});

/**
 * Wspólna, reaktywna lista przypomnień (C3): wczytywana raz, potem odświeżana
 * po zdarzeniu `reminders-changed` i po zmianie danych (usunięcie zadania
 * zdejmuje je z przypomnienia).
 */
export function useReminders() {
  if (!started) {
    started = true;
    void load();
    void onRemindersChanged(() => void load()).catch(() => undefined);
    void onDataChanged(() => void load()).catch(() => undefined);
    // Podgląd: zmiany zadań w tym samym oknie.
    window.addEventListener("preview-data-changed", () => void load());
  }
  return { reminders: readonly(list), byTask, reload: load };
}
