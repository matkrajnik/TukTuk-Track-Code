<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { RefreshCw } from "lucide-vue-next";
import SearchSelect from "./SearchSelect.vue";
import type { SearchOption } from "../searchSelect";
import {
  detailsOf,
  onTsChanged,
  saveDetails,
  tsSettings,
  tsTodayTasks,
  type Snapshot,
  type Task,
  type TsProject,
} from "../store";
import { findTsTask, resolveTsTask } from "../timesheet";
import { openView } from "../navigation";
import { setTsProjectMap, useTsProjectMap } from "../composables/useTsProjectMap";

/**
 * „Zadanie TS” w rozwiniętym kafelku (PLAN_1.8.md, T2): jawny wybór zadania
 * Ideo TS z listy `ts_tasks(dzisiaj)` (pamięć podręczna w `store.ts`) i
 * „Ustaw jako domyślne dla projektu”. Bez połączenia pokazuje zapamiętaną nazwę.
 */
const props = defineProps<{ task: Task }>();
const emit = defineEmits<{ updated: [data: Snapshot]; error: [message: string] }>();

const map = useTsProjectMap();
const state = ref<"loading" | "ready" | "offline" | "error">("loading");
const projects = ref<TsProject[]>([]);
const loadError = ref("");
const busy = ref(false);

const explicitId = computed(() => props.task.details?.tsTaskId?.trim() ?? "");
const explicitName = computed(
  () => props.task.details?.tsTaskName?.trim() || explicitId.value,
);
const project = computed(() => props.task.details?.project?.trim() ?? "");
const resolved = computed(() => resolveTsTask(props.task, map.value));
/** Domyślne zadanie TS projektu tego zadania (bez względu na wybór w zadaniu). */
const projectDefault = computed(() =>
  project.value
    ? resolveTsTask({ details: { ...detailsOf(props.task), tsTaskId: "" } }, map.value)
    : null,
);
const listed = computed(() =>
  explicitId.value ? findTsTask(projects.value, explicitId.value) : null,
);
/** Zapamiętane zadanie nie występuje na dzisiejszej liście TS. */
const unavailable = computed(
  () => state.value === "ready" && !!explicitId.value && !listed.value,
);
/** Pole wyboru: domyślne zadanie projektu to właśnie wybrane w zadaniu. */
const isDefault = computed(
  () => !!explicitId.value && projectDefault.value?.id === explicitId.value,
);
const asDefault = ref(isDefault.value);
watch(isDefault, (value) => (asDefault.value = value));

async function load(refresh = false) {
  loadError.value = "";
  try {
    const settings = await tsSettings();
    if (!settings.connected) {
      state.value = "offline";
      return;
    }
    // Odświeżenie po zmianie TS nie chowa listy na czas pobierania.
    if (state.value !== "ready") state.value = "loading";
    projects.value = await tsTodayTasks(refresh);
    state.value = "ready";
  } catch (e) {
    loadError.value = String(e instanceof Error ? e.message : e);
    state.value = "error";
  }
}
onMounted(() => void load());
// Połączenie lub rozłączenie konta w Ustawieniach, zmiana symulowanego TS.
onBeforeUnmount(onTsChanged(() => void load()));

async function run(action: () => Promise<void>) {
  if (busy.value) return;
  busy.value = true;
  emit("error", "");
  try {
    await action();
  } catch (e) {
    emit("error", String(e instanceof Error ? e.message : e));
    asDefault.value = isDefault.value;
  } finally {
    busy.value = false;
  }
}
/**
 * Wybór z listy: zapis w zadaniu (nazwa odświeżana przy każdym wyborze).
 * Nie zmienia domyślnego zadania projektu — to osobna akcja pola wyboru,
 * żeby zmiana w jednym zadaniu nie przestawiała po cichu innych zadań projektu.
 */
function choose(id: string) {
  if (id === explicitId.value) return;
  const name = id ? (findTsTask(projects.value, id)?.name ?? "") : "";
  // Pole pokazuje `explicitId`, więc po błędzie zapisu wraca samo.
  void run(async () => {
    emit(
      "updated",
      await saveDetails(props.task.id, {
        ...detailsOf(props.task),
        tsTaskId: id,
        tsTaskName: name,
      }),
    );
  });
}
/** Pozycje pola: „— brak —”, niedostępne zapamiętane, zadania w grupach projektów TS. */
const options = computed<SearchOption<string>[]>(() => [
  { value: "", label: "— brak —" },
  ...(unavailable.value
    ? [{ value: explicitId.value, label: `(niedostępne) ${explicitName.value}` }]
    : []),
  ...projects.value.flatMap((p) =>
    p.tasks.map((t) => ({ value: t.id, label: t.name, group: p.name })),
  ),
]);
/** „Ustaw jako domyślne dla projektu”: mapowanie projektu na wybrane zadanie. */
function toggleDefault() {
  const on = asDefault.value;
  if (!project.value) return;
  if (on && !explicitId.value) return;
  void run(async () => {
    if (on) await setTsProjectMap(project.value, explicitId.value, explicitName.value);
    else if (projectDefault.value?.id === explicitId.value)
      await setTsProjectMap(project.value, "", "");
  });
}
function connect() {
  openView({ view: "settings", section: "timesheet" });
}
</script>

<template>
  <section class="ts-task" :aria-label="'Zadanie TS: ' + task.title">
    <div class="ts-task-row">
      <label v-if="state === 'ready'" class="ts-task-select"
        >Zadanie TS<SearchSelect
          :model-value="explicitId"
          :label="'Zadanie TS: ' + task.title"
          :options="options"
          :disabled="busy"
          @change="choose"
      /></label
      >
      <p v-else class="ts-task-stored">
        <span class="ts-task-label">Zadanie TS:</span>
        {{ resolved ? resolved.name : "brak" }}
        <template v-if="resolved?.source === 'project'"> (z projektu)</template>
      </p>
      <button
        v-if="state === 'ready' || state === 'error'"
        type="button"
        class="icon-button"
        aria-label="Odśwież listę zadań TS"
        title="Pobierz ponownie listę zadań z TS"
        :disabled="busy"
        @click="load(true)"
      >
        <RefreshCw :size="14" />
      </button>
    </div>
    <p v-if="state === 'offline'" class="hint">
      <button type="button" class="text-link" @click="connect">
        Połącz konto TS w Ustawieniach
      </button>
    </p>
    <p v-if="state === 'error'" class="hint ts-task-warning" role="alert">
      Nie udało się pobrać listy zadań TS: {{ loadError }}
    </p>
    <p v-if="unavailable" class="hint ts-task-warning" role="alert">
      Zadania TS „{{ explicitName }}” nie ma na dzisiejszej liście zadań TS.
      Wybierz inne albo odśwież listę.
    </p>
    <p
      v-if="!explicitId && projectDefault && state === 'ready'"
      class="hint"
    >
      Domyślnie z projektu „{{ project }}”: {{ projectDefault.name }}
    </p>
    <label v-if="project && explicitId" class="inline-check ts-task-default">
      <input
        v-model="asDefault"
        type="checkbox"
        :disabled="busy"
        @change="toggleDefault"
      />
      Ustaw jako domyślne dla projektu „{{ project }}”
    </label>
  </section>
</template>
