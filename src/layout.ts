import {
  computed,
  onActivated,
  onDeactivated,
  onUnmounted,
  ref,
  watch,
} from "vue";

/**
 * Szeroki układ treści (`main.wide`, PLAN_BACKLOG.md B2): widoki tablicowe
 * (Jira → Tablica, Plan → Tydzień → Tablica) zdejmują limit 1120 px, listy
 * i formularze zostają w nim. Każdy widok zgłasza się przez `useWideLayout`;
 * zgłoszenie znika przy odmontowaniu i schowaniu przez `KeepAlive`.
 */
const requests = ref(new Set<symbol>());

/** Czy któryś widoczny widok potrzebuje pełnej szerokości. */
export const wideLayout = computed(() => requests.value.size > 0);

function set(id: symbol, on: boolean) {
  if (requests.value.has(id) === on) return;
  const next = new Set(requests.value);
  if (on) next.add(id);
  else next.delete(id);
  requests.value = next;
}

/** Widok prosi o pełną szerokość, dopóki `wide()` zwraca `true`. */
export function useWideLayout(wide: () => boolean) {
  const id = Symbol("wide");
  let visible = true;
  watch(wide, (on) => set(id, visible && on), { immediate: true });
  onActivated(() => {
    visible = true;
    set(id, wide());
  });
  onDeactivated(() => {
    visible = false;
    set(id, false);
  });
  onUnmounted(() => set(id, false));
}
