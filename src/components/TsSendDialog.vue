<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import {
  tsFilled,
  tsMergeKeep,
  tsSend,
  tsTasks,
  type Snapshot,
  type TsEntry,
  type TsFilled,
  type TsMode,
  type TsProject,
} from "../store";
import {
  findTsTask,
  tsCheckSaved,
  tsCleanText,
  tsEntries,
  tsHoursText,
  tsHundredths,
  tsPlural,
} from "../timesheet";
import { decimalHours } from "../report";
import { dayText } from "../time";
import { openView } from "../navigation";
import { useModal } from "../composables/useModal";
import { useTsProjectMap } from "../composables/useTsProjectMap";

/**
 * Podgląd i wysyłka dnia do Ideo TS (PLAN_1.8.md, T3). Wiersze z `tsEntries`
 * policzone przy otwarciu (trwający pomiar do tej chwili), istniejące wpisy
 * z `ts_filled(day)` i lista zadań z `ts_tasks(day)`. Zapis tylko po kliknięciu
 * „Zapisz w TS”; zapis zastępuje cały dzień, „Zachowaj pozostałe wpisy TS”
 * dokleja wpisy TS o zadaniach spoza raportu. Nie zamyka dnia.
 */
const props = defineProps<{ data: Snapshot; day: string; now: number }>();
const emit = defineEmits<{
  close: [];
  /** Zapis przyjęty: dzień i suma wysłanych wpisów raportu („7,50”). */
  sent: [day: string, hours: string];
}>();
const { root, trapFocus } = useModal();
const map = useTsProjectMap();

/** Chwila otwarcia: trwający pomiar liczony do niej. */
const openedAt = props.now;
const plan = computed(() =>
  tsEntries(props.day, props.data, openedAt, map.value),
);
/** Opisy edytowalne w podglądzie (nie zmieniają notatek w Todo Track). */
const descriptions = ref<Record<string, string>>({});
const description = (id: string, fallback: string) =>
  descriptions.value[id] ?? fallback;

const loading = ref(true);
const loadError = ref("");
const projects = ref<TsProject[]>([]);
const filled = ref<TsFilled[]>([]);
const mode = ref<TsMode>("keep");
const busy = ref(false);
const error = ref("");
/** Liczba wpisów raportu w ostatnim zapisie. */
const sentCount = ref(0);
const result = ref<{
  count: number;
  hours: string;
  differs: boolean;
  warning: string;
} | null>(null);

async function load() {
  loading.value = true;
  loadError.value = "";
  try {
    const [list, current] = await Promise.all([
      tsTasks(props.day),
      tsFilled(props.day),
    ]);
    projects.value = list;
    filled.value = current;
  } catch (e) {
    loadError.value = String(e instanceof Error ? e.message : e);
  } finally {
    loading.value = false;
  }
}
onMounted(() => void load());

const hours = (value: string) => tsHoursText(tsHundredths(value));
const taskName = (id: string) =>
  findTsTask(projects.value, id)?.name ?? `Zadanie TS ${id}`;
const ours = computed(() => new Set(plan.value.entries.map((e) => e.tsTaskId)));
/** Godziny zadania TS już zapisane w TS (setne), gdy raport je zastąpi. */
const replaced = (id: string) => {
  const rows = filled.value.filter((f) => f.id.trim() === id);
  return rows.length
    ? tsHoursText(rows.reduce((sum, f) => sum + tsHundredths(f.hours), 0))
    : "";
};
const filledHours = computed(() =>
  tsHoursText(filled.value.reduce((sum, f) => sum + tsHundredths(f.hours), 0)),
);
/** Zadania TS z raportu, których nie ma na liście TS tego dnia. */
const missing = computed(() =>
  loading.value || loadError.value
    ? []
    : plan.value.entries.filter((e) => !findTsTask(projects.value, e.tsTaskId)),
);
const totalHours = computed(() => decimalHours(plan.value.totalMs));
const filledCount = computed(() => {
  const text = tsPlural(filled.value.length, "wpis", "wpisy", "wpisów");
  // „jest już 1 wpis”, „są już 2 wpisy”, „jest już 5 wpisów”.
  return `${text.endsWith(" wpisy") ? "są" : "jest"} już ${text}`;
});

const entries = (): TsEntry[] =>
  plan.value.entries.map((e) => ({
    taskId: e.tsTaskId,
    hours: e.hours,
    overtime: e.overtime,
    description: tsCleanText(description(e.tsTaskId, e.description)),
  }));
const canSave = computed(
  () =>
    !busy.value &&
    !loading.value &&
    !loadError.value &&
    !result.value &&
    plan.value.entries.length > 0,
);

async function save() {
  if (!canSave.value) return;
  busy.value = true;
  error.value = "";
  const list = entries();
  const expected = mode.value === "keep" ? tsMergeKeep(list, filled.value) : list;
  sentCount.value = list.length;
  try {
    const saved = await tsSend(props.day, list, mode.value);
    const check = tsCheckSaved(expected, saved.filled);
    result.value = {
      ...check,
      differs: !saved.warning && check.differs,
      warning: saved.warning ?? "",
    };
    filled.value = saved.filled;
    emit("sent", props.day, totalHours.value);
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  } finally {
    busy.value = false;
  }
}
/** Zamknięcie okna; w trakcie zapisu zablokowane (wynik musi trafić do znacznika). */
function close() {
  if (!busy.value) emit("close");
}
function showTask(id: number) {
  if (busy.value) return;
  openView({ view: "tasks", task: id });
  emit("close");
}
</script>

<template>
  <div
    class="modal-overlay"
    @click.self="close"
    @keydown.esc="close"
  >
    <section
      ref="root"
      class="modal ts-send-modal"
      role="dialog"
      aria-modal="true"
      aria-label="Wyślij do TS"
      @keydown="trapFocus"
    >
      <h2>Wyślij do TS · {{ dayText(day) }}</h2>
      <p v-if="error" class="error" role="alert">{{ error }}</p>
      <div v-if="plan.entries.length" class="ts-send-table-wrap">
        <table class="ts-send-table" aria-label="Wpisy do wysłania">
          <thead>
            <tr>
              <th>Zadania Todo Track</th>
              <th>Zadanie TS</th>
              <th class="ts-send-hours">Godziny</th>
              <th>Opis</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="e in plan.entries" :key="e.tsTaskId">
              <td>{{ e.tasks.map((t) => t.title).join(", ") }}</td>
              <td>
                {{ e.tsTaskName
                }}<span
                  v-if="mode === 'keep' && replaced(e.tsTaskId)"
                  class="ts-send-tag"
                  >zastąpi {{ replaced(e.tsTaskId) }} h</span
                >
              </td>
              <td class="mono ts-send-hours">{{ e.hours }}</td>
              <td>
                <input
                  :value="description(e.tsTaskId, e.description)"
                  :aria-label="'Opis wpisu TS: ' + e.tsTaskName"
                  maxlength="2000"
                  :disabled="busy || !!result"
                  @input="
                    descriptions[e.tsTaskId] = (
                      $event.target as HTMLInputElement
                    ).value
                  "
                />
              </td>
            </tr>
          </tbody>
          <tfoot>
            <tr>
              <th colspan="2">Razem</th>
              <th class="mono ts-send-hours">{{ totalHours }}</th>
              <th />
            </tr>
          </tfoot>
        </table>
      </div>
      <p v-else class="report-note">
        Brak czasu z przypisanym zadaniem TS — nie ma czego wysłać.
      </p>
      <div
        v-if="plan.unmapped.length || plan.running || missing.length"
        class="ts-send-warnings"
        role="note"
        aria-label="Ostrzeżenia wysyłki"
      >
        <div v-if="plan.unmapped.length">
          <p>
            Bez zadania TS:
            {{ tsPlural(plan.unmapped.length, "zadanie", "zadania", "zadań") }}
            ({{ decimalHours(plan.unmappedMs) }} h) — ten czas nie zostanie
            wysłany.
          </p>
          <ul>
            <li v-for="u in plan.unmapped" :key="u.task.id">
              <button
                type="button"
                class="text-link"
                :aria-label="'Pokaż zadanie: ' + u.task.title"
                @click="showTask(u.task.id)"
              >
                {{ u.task.title }}</button
              ><span class="mono"> · {{ decimalHours(u.ms) }} h</span>
            </li>
          </ul>
        </div>
        <p v-if="plan.running">
          Trwający pomiar liczony do teraz (do chwili otwarcia podglądu).
        </p>
        <p v-for="e in missing" :key="e.tsTaskId">
          Zadanie TS „{{ e.tsTaskName }}” nie występuje na liście tego dnia.
        </p>
      </div>
      <section class="ts-send-existing" aria-label="Wpisy w TS">
        <p v-if="loading">Wczytywanie dnia z TS…</p>
        <template v-else-if="loadError">
          <p class="error" role="alert">
            Nie udało się odczytać dnia z TS: {{ loadError }}
          </p>
          <button type="button" class="today-button" @click="load">
            Spróbuj ponownie
          </button>
        </template>
        <template v-else-if="!result">
          <template v-if="filled.length">
            <h3>
              W TS na ten dzień {{ filledCount }} (razem {{ filledHours }} h)
            </h3>
            <ul class="ts-send-filled">
              <li v-for="(f, i) in filled" :key="i">
                <strong>{{ taskName(f.id) }}</strong>
                <span class="mono">{{ hours(f.hours) }} h</span>
                <span v-if="f.description" class="ts-send-desc">{{
                  f.description
                }}</span>
                <span
                  v-if="mode === 'replace'"
                  class="ts-send-tag"
                  >zniknie</span
                ><span
                  v-else-if="ours.has(f.id.trim())"
                  class="ts-send-tag"
                  >zostanie zastąpiony</span
                >
              </li>
            </ul>
            <div class="ts-send-mode" role="radiogroup" aria-label="Wpisy już zapisane w TS">
              <label class="inline-check"
                ><input v-model="mode" type="radio" value="keep" />Zachowaj
                pozostałe wpisy TS</label
              >
              <label class="inline-check"
                ><input v-model="mode" type="radio" value="replace" />Zastąp
                wszystko</label
              >
            </div>
            <p class="hint">
              {{
                mode === "keep"
                  ? "Wpisy o zadaniach spoza raportu zostaną dopisane bez zmian, wpisy o tych samych zadaniach zastąpione."
                  : "Znikną wszystkie dotychczasowe wpisy tego dnia w TS; zostaną tylko wpisy z raportu."
              }}
            </p>
          </template>
          <p v-else>Dzień w TS jest pusty.</p>
        </template>
      </section>
      <div v-if="result" class="ts-send-result">
        <p v-if="result.warning" role="status">
          Wysłano do TS {{ tsPlural(sentCount, "wpis", "wpisy", "wpisów") }},
          razem {{ totalHours }} h z raportu.
        </p>
        <p v-else role="status">
          W TS zapisano
          {{ tsPlural(result.count, "wpis", "wpisy", "wpisów") }}, razem
          {{ result.hours }} h.
        </p>
        <p v-if="result.warning" class="ts-send-warning" role="alert">
          {{ result.warning }}
        </p>
        <p v-if="result.differs" class="ts-send-warning" role="alert">
          TS zapisał inne wartości niż wysłane — sprawdź w przeglądarce.
        </p>
      </div>
      <p v-else class="hint">
        Zapis w TS zastępuje cały dzień. Wysyłka nie zamyka dnia w Todo Track.
      </p>
      <div class="modal-actions">
        <button :disabled="busy" @click="close">
          {{ result ? "Zamknij" : "Anuluj" }}</button
        ><button
          v-if="!result"
          class="primary-button"
          :disabled="!canSave"
          @click="save"
        >
          {{
            busy
              ? "Zapisywanie…"
              : mode === "replace" && filled.length
                ? "Zastąp i zapisz"
                : "Zapisz w TS"
          }}
        </button>
      </div>
    </section>
  </div>
</template>
