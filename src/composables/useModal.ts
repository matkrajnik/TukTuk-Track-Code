import { nextTick, onBeforeUnmount, onMounted, ref } from "vue";

const focusable = "input:not(:disabled), button:not(:disabled)";

/**
 * Zachowanie okna modalnego montowanego na czas otwarcia: fokus na pierwszym
 * polu lub przycisku, pułapka Tab/Shift+Tab i przywrócenie fokusu po zamknięciu.
 */
export function useModal() {
  const root = ref<HTMLElement | null>(null);
  let previousFocus: HTMLElement | null = null;
  onMounted(async () => {
    previousFocus = document.activeElement as HTMLElement | null;
    await nextTick();
    root.value?.querySelector<HTMLElement>("input, button")?.focus();
  });
  onBeforeUnmount(() => previousFocus?.focus());
  function trapFocus(event: KeyboardEvent) {
    if (event.key !== "Tab" || !root.value) return;
    const targets = [...root.value.querySelectorAll<HTMLElement>(focusable)];
    const first = targets[0],
      last = targets[targets.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last?.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first?.focus();
    }
  }
  return { root, trapFocus };
}
