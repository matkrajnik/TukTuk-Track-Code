import { computed, ref, toValue, type MaybeRefOrGetter, type Ref } from "vue";
import type { Parsed } from "../quickadd";
import type { Snapshot, Task } from "../store";
import { existingTask, suggestTasks } from "../taskSuggest";

/**
 * Podpowiedzi istniejących zadań pod polem „Nowe zadanie”: lista (combobox),
 * wybrana pozycja i zadanie o tej samej nazwie, które formularz kontynuuje
 * zamiast tworzyć nowe.
 */
export function useTaskSuggestions(
  data: MaybeRefOrGetter<Snapshot>,
  parsed: MaybeRefOrGetter<Parsed>,
  title: Ref<string>,
  input: Ref<HTMLInputElement | null>,
) {
  const existing = computed(() => existingTask(toValue(data), toValue(parsed)));
  const suggestions = computed(() =>
    suggestTasks(toValue(data), toValue(parsed)),
  );
  const index = ref(-1);
  const open = ref(false);
  const shown = computed(() => open.value && suggestions.value.length > 0);
  /** Wpisanie tekstu otwiera listę bez wybranej pozycji. */
  function typed() {
    open.value = true;
    index.value = -1;
  }
  function close() {
    open.value = false;
    index.value = -1;
  }
  /** Wybór z listy wpisuje nazwę zadania; Enter go potem kontynuuje. */
  function pick(task: Task) {
    title.value = task.title;
    close();
    input.value?.focus();
  }
  return { existing, suggestions, index, shown, typed, close, pick };
}
