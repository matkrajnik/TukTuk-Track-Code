import { nextTick, ref, type Ref } from "vue";
import { neighbour, planMove } from "../order";
import { reorderTasks, type Snapshot, type Task } from "../store";

/**
 * Wspólna obsługa ręcznej kolejności dla listy zadań i planu dnia.
 * `scope` odróżnia uchwyty różnych list przy przywracaniu fokusu.
 */
export function useReorder(options: {
  scope: string;
  all: () => Task[];
  activeTaskId: () => number | null | undefined;
  busy: Ref<boolean>;
  updated: (data: Snapshot) => void;
  error: (message: string) => void;
}) {
  /** Przeciągane zadanie i lista (grupa), z której pochodzi. */
  const dragging = ref<{ id: number; list: string } | null>(null);
  const over = ref(0);

  async function apply(visible: Task[], id: number, targetId: number) {
    const entries = planMove(options.all(), visible, id, targetId);
    if (!entries || options.busy.value) return;
    options.busy.value = true;
    options.error("");
    try {
      options.updated(await reorderTasks(entries));
    } catch (e) {
      options.error(String(e));
    } finally {
      options.busy.value = false;
    }
  }

  async function move(visible: Task[], id: number, delta: -1 | 1) {
    const target = neighbour(visible, id, delta, options.activeTaskId());
    if (!target) return;
    await apply(visible, id, target.id);
    await nextTick();
    document
      .querySelector<HTMLElement>(
        `[data-reorder-handle="${options.scope}-${id}"]`,
      )
      ?.focus();
  }

  function start(event: DragEvent, id: number, list: string) {
    dragging.value = { id, list };
    if (event.dataTransfer) {
      event.dataTransfer.effectAllowed = "move";
      event.dataTransfer.setData("text/plain", String(id));
      const card = (event.target as HTMLElement).closest<HTMLElement>(
        "[data-reorder-item]",
      );
      if (card) event.dataTransfer.setDragImage(card, 16, 16);
    }
  }
  function end() {
    dragging.value = null;
    over.value = 0;
  }

  /** Atrybuty i nasłuch dla elementu listy, na który można upuścić zadanie. */
  function dropProps(visible: Task[], task: Task, list: string) {
    const accepts = () =>
      dragging.value?.list === list && dragging.value.id !== task.id;
    return {
      "data-reorder-item": task.id,
      class: { "drop-target": over.value === task.id },
      onDragover: (event: DragEvent) => {
        if (!accepts()) return;
        event.preventDefault();
        if (event.dataTransfer) event.dataTransfer.dropEffect = "move";
        over.value = task.id;
      },
      onDragleave: () => {
        if (over.value === task.id) over.value = 0;
      },
      onDrop: (event: DragEvent) => {
        if (!accepts()) return;
        event.preventDefault();
        const id = dragging.value!.id;
        end();
        void apply(visible, id, task.id);
      },
    };
  }

  return { move, start, end, dropProps };
}
