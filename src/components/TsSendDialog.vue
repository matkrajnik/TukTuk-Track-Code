<script setup lang="ts">
import { computed, nextTick, onMounted, reactive, ref } from "vue";
import { Pencil, RotateCcw } from "lucide-vue-next";
import SearchSelect from "./SearchSelect.vue";
import AiPreview from "./AiPreview.vue";
import type { SearchOption } from "../searchSelect";
import {
  detailsOf,
  saveDetails,
  tsFilled,
  tsSend,
  tsTasks,
  type Snapshot,
  type Task,
  type TsEntry,
  type TsFilled,
  type TsMode,
  type TsProject,
} from "../store";
import type { TsDescriptionHint } from "../api/aiday";
import {
  aiSuggestTsPreview,
  aiSuggestTsTask,
  tsOptionsOf,
  useAssignEnabled,
  type TsSuggestion,
} from "../aiassign";
import {
  findTsTask,
  tsCheckSaved,
  tsCleanText,
  tsEntries,
  tsHoursText,
  tsHundredths,
  tsOvertimeSum,
  tsOvertimeText,
  resolveTsTask,
  tsPlural,
  type TsEntriesResult,
  type TsSendRow,
} from "../timesheet";
import { tsValidHours } from "../api/timesheet";
import { decimalHours } from "../report";
import { dayText } from "../time";
import { useSettings } from "../target";
import { openView } from "../navigation";
import { useModal } from "../composables/useModal";
import {
  setTsProjectMap,
  useTsProjectMap,
} from "../composables/useTsProjectMap";

/**
 * Podgląd i wysyłka dni do Ideo TS (PLAN_1.8.md, T3; B4). Dla każdego dnia
 * wiersze z `tsEntries` policzone przy otwarciu (trwający pomiar do tej
 * chwili) z podziałem na godziny i nadgodziny ponad normę dnia, istniejące
 * wpisy z `ts_filled(day)` i lista zadań z `ts_tasks(day)`. Godziny i
 * nadgodziny można zmienić przed zapisem, zadaniom bez zadania TS można je
 * przypisać bez wychodzenia z podglądu. Kilka dni: zakładki dni, jeden zapis
 * (dzień po dniu) i jedno podsumowanie. Zapis tylko po kliknięciu „Zapisz
 * w TS”; zapis zastępuje cały dzień, „Zachowaj pozostałe wpisy TS” dokleja
 * wpisy TS o zadaniach spoza raportu. Nie zamyka dnia.
 */
const props = defineProps<{
  data: Snapshot;
  days: string[];
  now: number;
  /**
   * Proponowane opisy wpisów z asystenta Zamknięcia dnia (A5, `ts_description`):
   * pokazywane pod polem opisu jednego dnia; opis zmienia dopiero „Użyj opisu”.
   */
  hints?: TsDescriptionHint[];
}>();
const emit = defineEmits<{
  close: [];
  /** Zapis dnia przyjęty: dzień i suma wpisów raportu z pomiarów („7,50”). */
  sent: [day: string, hours: string];
  /** Przypisanie zadania TS z podglądu zmieniło zadanie. */
  updated: [data: Snapshot];
}>();
const { root, trapFocus } = useModal();
const map = useTsProjectMap();
const settings = useSettings();

/** Godziny i nadgodziny wpisu zmienione w podglądzie (tekst z pola). */
interface Amounts {
  hours: string;
  overtime: string;
}
interface DayResult {
  count: number;
  /** Godziny razem z nadgodzinami odczytane z TS. */
  hours: string;
  /** Same nadgodziny z odczytu (setne). */
  overtime: number;
  differs: boolean;
  warning: string;
  /** Liczba i suma wysłanych wpisów raportu. */
  sentCount: number;
  sentHours: string;
}
interface DayState {
  day: string;
  loading: boolean;
  loadError: string;
  projects: TsProject[];
  filled: TsFilled[];
  mode: TsMode;
  /** Opisy edytowalne w podglądzie (nie zmieniają notatek w Todo Track). */
  descriptions: Record<string, string>;
  edits: Record<string, Amounts>;
  /** Informacja po przypisaniu zadania TS (godziny przeliczone z pomiarów). */
  notice: string;
  error: string;
  result: DayResult | null;
}

/** Chwila otwarcia: trwający pomiar liczony do niej. */
const openedAt = props.now;
/** Dni ustalone przy otwarciu (bez zmian w trakcie wysyłki). */
const states = reactive<DayState[]>(
  props.days.map((day) => ({
    day,
    loading: true,
    loadError: "",
    projects: [],
    filled: [],
    mode: "keep",
    descriptions: {},
    edits: {},
    notice: "",
    error: "",
    result: null,
  })),
);
const multi = states.length > 1;
const active = ref(0);
const cur = computed(() => states[active.value]);
const plans = computed<TsEntriesResult[]>(() =>
  states.map((s) =>
    tsEntries(s.day, props.data, openedAt, map.value, {
      settings: settings.value,
    }),
  ),
);
const plan = computed(() => plans.value[active.value]);

const busy = ref(false);
/** Edycja godzin i nadgodzin włączona (pola zamiast liczb). */
const editing = ref(false);
/** Po zapisie kilku dni: podsumowanie wysyłki. */
const summary = ref(false);

async function loadDay(s: DayState) {
  s.loading = true;
  s.loadError = "";
  try {
    const [list, current] = await Promise.all([tsTasks(s.day), tsFilled(s.day)]);
    s.projects = list;
    s.filled = current;
  } catch (e) {
    s.loadError = String(e instanceof Error ? e.message : e);
  } finally {
    s.loading = false;
  }
}
async function loadAll() {
  // Dzień po dniu: każdy odczyt to strona formularza TS.
  for (const s of states) await loadDay(s);
}
onMounted(() => void loadAll());

const hours = (value: string) => tsHoursText(tsHundredths(value));
const work = (f: { hours: string; overtime: string }) =>
  tsHundredths(f.hours) + tsHundredths(f.overtime);
const taskName = (id: string) =>
  findTsTask(cur.value.projects, id)?.name ?? `Zadanie TS ${id}`;
const ours = computed(() => new Set(plan.value.entries.map((e) => e.tsTaskId)));
/** Godziny zadania TS już zapisane w TS (setne), gdy raport je zastąpi. */
const replaced = (id: string) => {
  const rows = cur.value.filled.filter((f) => f.id.trim() === id);
  return rows.length
    ? tsHoursText(rows.reduce((sum, f) => sum + work(f), 0))
    : "";
};
const filledHours = computed(() =>
  tsHoursText(cur.value.filled.reduce((sum, f) => sum + work(f), 0)),
);
/** Zadania TS z raportu, których nie ma na liście TS tego dnia. */
const missing = computed(() =>
  cur.value.loading || cur.value.loadError
    ? []
    : plan.value.entries.filter(
        (e) => !findTsTask(cur.value.projects, e.tsTaskId),
      ),
);
const filledCount = computed(() => {
  const text = tsPlural(cur.value.filled.length, "wpis", "wpisy", "wpisów");
  // „jest już 1 wpis”, „są już 2 wpisy”, „jest już 5 wpisów”.
  return `${text.endsWith(" wpisy") ? "są" : "jest"} już ${text}`;
});

// ---------------------------------------------------------------------------
// Godziny i nadgodziny (B4): podział z pomiarów, edycja przed zapisem.

const description = (s: DayState, e: TsSendRow) =>
  s.descriptions[e.tsTaskId] ?? e.description;
/** Podpowiedzi opisu odrzucone w tym oknie („Pomiń”), po zadaniu lokalnym. */
const skippedHints = ref<Set<number>>(new Set());
/**
 * Proponowany opis wpisu (A5): tylko przy jednym dniu, przed zapisem i gdy
 * różni się od opisu w polu.
 */
function hintFor(s: DayState, e: TsSendRow): TsDescriptionHint | undefined {
  if (multi || s.result) return undefined;
  return props.hints?.find(
    (h) =>
      !skippedHints.value.has(h.taskId) &&
      e.tasks.some((t) => t.id === h.taskId) &&
      h.text.trim() !== description(s, e).trim(),
  );
}
/** „Użyj opisu”: wpisuje propozycję w pole opisu (zapis dopiero „Zapisz w TS”). */
function applyHint(s: DayState, e: TsSendRow, h: TsDescriptionHint) {
  s.descriptions[e.tsTaskId] = h.text;
}
const amounts = (s: DayState, e: TsSendRow): Amounts =>
  s.edits[e.tsTaskId] ?? { hours: e.hours, overtime: e.overtime };
const edited = (s: DayState, e: TsSendRow) => !!s.edits[e.tsTaskId];
function setAmount(e: TsSendRow, key: keyof Amounts, value: string) {
  const s = cur.value;
  s.edits[e.tsTaskId] = { ...amounts(s, e), [key]: value };
}
/** „Przywróć z pomiarów”: proponowany podział zamiast wpisanych wartości. */
function restore(e: TsSendRow) {
  delete cur.value.edits[e.tsTaskId];
}
/** Komunikat błędu pól wpisu albo pusty tekst. */
function invalid(a: Amounts): string {
  const h = a.hours.trim();
  const o = a.overtime.trim() || "0";
  if (!tsValidHours(h) || !tsValidHours(o))
    return "Podaj liczbę z najwyżej dwoma miejscami po przecinku, np. 1,25.";
  if (tsHundredths(h) + tsHundredths(o) <= 0)
    return "Wpis musi mieć godziny albo nadgodziny.";
  return "";
}
const fieldError = (s: DayState, e: TsSendRow) => invalid(amounts(s, e));
const dayInvalid = (i: number) =>
  plans.value[i].entries.some((e) => fieldError(states[i], e));
/** Sumy dnia z wartości do wysłania (setne): godziny, nadgodziny, razem. */
function sums(i: number) {
  let h = 0;
  let o = 0;
  for (const e of plans.value[i].entries) {
    const a = amounts(states[i], e);
    h += tsHundredths(a.hours);
    o += tsHundredths(a.overtime);
  }
  return { hours: h, overtime: o, total: h + o };
}
const curSums = computed(() => sums(active.value));
/**
 * Godziny (bez nadgodzin, setne) wpisów TS, które zostaną w dniu obok raportu:
 * przy „Zachowaj pozostałe wpisy TS” wpisy o zadaniach spoza raportu
 * (jak `tsMergeKeep`), przy „Zastąp wszystko” żadne.
 */
const keptHours = computed(() =>
  cur.value.mode === "keep" && !cur.value.result
    ? cur.value.filled
        .filter((f) => !ours.value.has(f.id.trim()))
        .reduce((sum, f) => sum + tsHundredths(f.hours), 0)
    : 0,
);
/** Godziny dnia w TS po zapisie: raport i zachowane wpisy TS. */
const dayHours = computed(() => curSums.value.hours + keptHours.value);
/** Norma dnia, gdy suma godzin ją przekracza (ostrzeżenie; nadwyżka należy do nadgodzin). */
const overNorm = computed(() => {
  const norm = plan.value.norm;
  return norm !== null && dayHours.value > norm ? norm : null;
});
const otText = (value: string) => tsOvertimeText(tsHundredths(value));

const entries = (i: number): TsEntry[] =>
  plans.value[i].entries.map((e) => {
    const a = amounts(states[i], e);
    return {
      taskId: e.tsTaskId,
      hours: tsHoursText(tsHundredths(a.hours)),
      overtime: tsOvertimeText(tsHundredths(a.overtime)),
      description: tsCleanText(description(states[i], e)),
    };
  });

// ---------------------------------------------------------------------------
// Przypisanie zadania TS z podglądu (B4).

/**
 * „Ustaw jako domyślne dla projektu” per zadanie (domyślnie wyłączone; po podpowiedzi
 * asystenta zaznaczone, gdy projekt nie ma jeszcze mapowania).
 */
const asDefault = ref<Record<number, boolean>>({});

// Asystent przypisań (A4): „Podpowiedz” i „Podpowiedz wszystkie” wypełniają
// sugestię z uzasadnieniem; zapis dopiero po „Użyj podpowiedzi” albo wyborze z listy.
const assistEnabled = useAssignEnabled();
const suggestions = ref<Record<number, TsSuggestion>>({});
const suggesting = ref<Record<number, boolean>>({});
const suggestingAll = ref(false);
/**
 * Trwa jakiekolwiek pytanie: przyciski „Podpowiedz” są wtedy wyłączone we
 * wszystkich wierszach (pytania o zadanie TS idą do modelu po jednym).
 */
const anySuggesting = computed(
  () => suggestingAll.value || Object.values(suggesting.value).some(Boolean),
);
/**
 * Wartość „domyślne dla projektu” sprzed podpowiedzi, gdy zaznaczył je asystent:
 * „Odrzuć” / „Zamknij” ją przywraca, żeby późniejszy wybór ręczny nie stał się
 * po cichu domyślnym zadaniem projektu.
 */
const defaultBefore = ref<Record<number, boolean>>({});
/** Projekt zadania nie ma domyślnego zadania TS (`ts_project_map`). */
const noProjectMap = (task: Task) =>
  !!projectOf(task) &&
  !resolveTsTask({ details: { ...detailsOf(task), tsTaskId: "" } }, map.value);
/** Jedno pytanie o zadanie; błąd trafia do komunikatu dnia, kolejne pytania idą dalej. */
async function suggestFor(task: Task, day: DayState): Promise<boolean> {
  if (suggesting.value[task.id]) return true;
  suggesting.value = { ...suggesting.value, [task.id]: true };
  try {
    const found = await aiSuggestTsTask(
      task.id,
      day.day,
      tsOptionsOf(day.projects),
    );
    suggestions.value = { ...suggestions.value, [task.id]: found };
    if (found.tsTaskId && noProjectMap(task) && !asDefault.value[task.id]) {
      if (!(task.id in defaultBefore.value))
        defaultBefore.value = { ...defaultBefore.value, [task.id]: false };
      asDefault.value[task.id] = true;
    }
    return true;
  } catch (e) {
    day.error = String(e instanceof Error ? e.message : e);
    return false;
  } finally {
    suggesting.value = { ...suggesting.value, [task.id]: false };
  }
}
/** „Podpowiedz wszystkie”: zadania bez zadania TS po kolei (jedno wywołanie naraz). */
async function suggestAll() {
  if (suggestingAll.value) return;
  const day = cur.value;
  suggestingAll.value = true;
  day.error = "";
  try {
    for (const u of plan.value.unmapped)
      if (!(await suggestFor(u.task, day))) break;
  } finally {
    suggestingAll.value = false;
  }
}
const suggestedName = (task: Task) => {
  const id = suggestions.value[task.id]?.tsTaskId;
  return id ? findTsTask(cur.value.projects, id)?.name : undefined;
};
/**
 * „Użyj podpowiedzi”: ta sama ścieżka co wybór z listy (`assign`). Po błędzie
 * zapisu sugestia zostaje, żeby nie trzeba było pytać modelu ponownie.
 */
async function applySuggestion(task: Task) {
  const id = suggestions.value[task.id]?.tsTaskId;
  if (!id) return;
  await assign(task, id);
}
function dropSuggestion(task: Task) {
  const { [task.id]: _dropped, ...rest } = suggestions.value;
  suggestions.value = rest;
  const { [task.id]: _before, ...left } = defaultBefore.value;
  defaultBefore.value = left;
}
/** „Odrzuć” / „Zamknij”: bez zapisu; przywraca pole „domyślne” i fokus w wierszu. */
async function dismissSuggestion(task: Task) {
  if (task.id in defaultBefore.value)
    asDefault.value[task.id] = defaultBefore.value[task.id];
  dropSuggestion(task);
  await nextTick();
  const row = root.value?.querySelector(`[data-unmapped="${task.id}"]`);
  const target =
    row?.querySelector<HTMLElement>(".ts-send-suggest:not(:disabled)") ??
    row?.querySelector<HTMLElement>('[role="combobox"]:not(:disabled)');
  if (target) target.focus();
  else await refocus();
}
/** „Pokaż, co zostanie wysłane” przy „Podpowiedz wszystkie”: pytania o każde zadanie. */
const suggestPreview = computed(() => {
  const day = cur.value;
  const tasks = plan.value.unmapped.map((u) => u.task);
  const options = tsOptionsOf(day.projects);
  return async () =>
    (
      await Promise.all(tasks.map((t) => aiSuggestTsPreview(t.id, day.day, options)))
    ).join("\n\n");
});
const projectOf = (task: Task) => task.details?.project?.trim() ?? "";
const tsOptions = computed<SearchOption<string>[]>(() =>
  cur.value.projects.flatMap((p) =>
    p.tasks.map((t) => ({ value: t.id, label: t.name, group: p.name })),
  ),
);
/** Zapis zadania TS w zadaniu; `true`, gdy się udał. */
async function assign(task: Task, id: string): Promise<boolean> {
  if (busy.value || !id) return false;
  const s = cur.value;
  const name = findTsTask(s.projects, id)?.name ?? "";
  busy.value = true;
  s.error = "";
  s.notice = "";
  let done = false;
  try {
    const project = projectOf(task);
    if (asDefault.value[task.id] && project)
      await setTsProjectMap(project, id, name);
    emit(
      "updated",
      await saveDetails(task.id, {
        ...detailsOf(task),
        tsTaskId: id,
        tsTaskName: name,
      }),
    );
    done = true;
    dropSuggestion(task);
    // Czas wpisu i podział nadgodzin dnia się zmieniają: wpisane ręcznie
    // wartości byłyby nieaktualne, więc wracają do podziału z pomiarów.
    if (Object.keys(s.edits).length) {
      s.edits = {};
      s.notice =
        "Po przypisaniu zadania TS godziny i nadgodziny przeliczono z pomiarów — poprzednie zmiany godzin odrzucono.";
    }
  } catch (e) {
    s.error = String(e instanceof Error ? e.message : e);
  } finally {
    busy.value = false;
  }
  if (done) await refocus();
  return done;
}
/**
 * Po przypisaniu wiersz z polem wyboru znika z listy „Bez zadania TS”, a z nim
 * fokus. Fokus wraca do okna: następne pole przypisania, przycisk zapisu albo
 * nagłówek okna.
 */
async function refocus() {
  await nextTick();
  const box = root.value;
  if (!box || box.contains(document.activeElement)) return;
  const target =
    box.querySelector<HTMLElement>(
      '.ts-send-unmapped [role="combobox"]:not(:disabled)',
    ) ??
    box.querySelector<HTMLElement>(".modal-actions .primary-button:not(:disabled)") ??
    box.querySelector<HTMLElement>("h2");
  target?.focus();
}

// ---------------------------------------------------------------------------
// Zapis.

/** Dni do zapisu: z wpisami, jeszcze niezapisane. */
const pending = computed(() =>
  states
    .map((s, i) => ({ s, i }))
    .filter(({ s, i }) => !s.result && plans.value[i].entries.length > 0),
);
/** Dzień nie może być zapisany: wczytywanie, błąd odczytu albo błędne pola. */
const blocked = (i: number) =>
  states[i].loading || !!states[i].loadError || dayInvalid(i);
/**
 * Dni gotowe do zapisu. Przy kilku dniach dzień z błędem odczytu albo
 * z błędnymi godzinami jest pomijany (podsumowanie podaje powód), a nie
 * blokuje pozostałych; zapis czeka tylko na zakończenie wczytywania.
 */
const ready = computed(() => pending.value.filter(({ i }) => !blocked(i)));
const canSave = computed(
  () =>
    !busy.value &&
    ready.value.length > 0 &&
    !pending.value.some(({ s }) => s.loading) &&
    (multi || ready.value.length === pending.value.length),
);
/** Powód pominięcia dnia przy zapisie kilku dni. */
const skipReason = (i: number) =>
  states[i].loadError
    ? "błąd odczytu dnia z TS"
    : dayInvalid(i)
      ? "błędne godziny lub nadgodziny"
      : "";
const skipped = computed(() => pending.value.filter(({ i }) => skipReason(i)));

async function saveDay(s: DayState, i: number) {
  s.error = "";
  const list = entries(i);
  try {
    const saved = await tsSend(s.day, list, s.mode);
    // Porównanie z listą faktycznie wysłaną (B4), nie z dniem z otwarcia.
    const check = tsCheckSaved(saved.sent, saved.filled);
    s.result = {
      count: check.count,
      hours: check.hours,
      overtime: tsOvertimeSum(saved.filled),
      differs: !saved.warning && saved.differs,
      warning: saved.warning ?? "",
      sentCount: list.length,
      sentHours: tsHoursText(
        list.reduce((sum, e) => sum + work(e), 0),
      ),
    };
    s.filled = saved.filled;
    emit("sent", s.day, plans.value[i].totalHours);
  } catch (e) {
    s.error = String(e instanceof Error ? e.message : e);
  }
}
async function save() {
  if (!canSave.value) return;
  busy.value = true;
  editing.value = false;
  try {
    for (const s of states) s.notice = "";
    // Lista ustalona przed zapisem: zapisane dni wypadają z `pending`.
    for (const { s, i } of [...ready.value]) await saveDay(s, i);
    if (multi) summary.value = true;
    // Pierwszy dzień z błędem na wierzchu.
    const failed = states.findIndex((s) => s.error);
    if (failed >= 0) active.value = failed;
  } finally {
    busy.value = false;
  }
}
const savedDays = computed(() => states.filter((s) => s.result));
const savedTotal = computed(() =>
  tsHoursText(
    savedDays.value.reduce(
      (sum, s) =>
        // Bez odczytu po zapisie: suma wysłanych wpisów (odczyt jest pusty).
        sum +
        tsHundredths(s.result!.warning ? s.result!.sentHours : s.result!.hours),
      0,
    ),
  ),
);
const dayStatus = (s: DayState, i: number) =>
  s.result
    ? "wysłano"
    : s.error || s.loadError
      ? "błąd"
      : plans.value[i].entries.length
        ? ""
        : "brak wpisów";

/** Zamknięcie okna; w trakcie zapisu zablokowane (wynik musi trafić do znacznika). */
function close() {
  if (!busy.value) emit("close");
}
function showTask(id: number) {
  if (busy.value) return;
  openView({ view: "tasks", task: id });
  emit("close");
}
const saveLabel = computed(() => {
  if (busy.value) return "Zapisywanie…";
  if (multi)
    return `Zapisz w TS (${tsPlural(
      pending.value.length - skipped.value.length,
      "dzień",
      "dni",
      "dni",
    )})`;
  return cur.value.mode === "replace" && cur.value.filled.length
    ? "Zastąp i zapisz"
    : "Zapisz w TS";
});
</script>

<template>
  <div class="modal-overlay" @click.self="close" @keydown.esc="close">
    <section
      ref="root"
      class="modal ts-send-modal"
      role="dialog"
      aria-modal="true"
      aria-label="Wyślij do TS"
      @keydown="trapFocus"
    >
      <h2 v-if="multi" tabindex="-1">
        Wyślij do TS · {{ tsPlural(states.length, "dzień", "dni", "dni") }}
      </h2>
      <h2 v-else tabindex="-1">Wyślij do TS · {{ dayText(states[0].day) }}</h2>
      <div
        v-if="multi"
        class="tabs ts-send-days"
        role="tablist"
        aria-label="Dni wysyłki"
      >
        <button
          v-for="(s, i) in states"
          :id="'ts-send-tab-' + s.day"
          :key="s.day"
          type="button"
          role="tab"
          :aria-selected="i === active"
          aria-controls="ts-send-day-panel"
          :class="{ selected: i === active, 'ts-send-day-blocked': !s.result && blocked(i) && plans[i].entries.length > 0 }"
          @click="active = i"
        >
          {{ dayText(s.day) }}
          <span class="mono">{{ plans[i].totalHours }}</span
          ><span v-if="dayStatus(s, i)" class="ts-send-day-status">
            · {{ dayStatus(s, i) }}</span
          >
        </button>
      </div>
      <div
        :id="multi ? 'ts-send-day-panel' : undefined"
        :role="multi ? 'tabpanel' : undefined"
        :aria-labelledby="multi ? 'ts-send-tab-' + cur.day : undefined"
      >
        <p v-if="cur.error" class="error" role="alert">{{ cur.error }}</p>
        <p v-if="cur.notice" class="hint ts-send-notice" aria-live="polite">
          {{ cur.notice }}
        </p>
        <template v-if="plan.entries.length">
          <div class="ts-send-tools">
            <button
              type="button"
              class="today-button"
              :aria-pressed="editing"
              :disabled="busy || !!cur.result"
              @click="editing = !editing"
            >
              <Pencil :size="14" />{{
                editing ? "Zakończ edycję godzin" : "Edytuj godziny"
              }}
            </button>
          </div>
          <div class="ts-send-table-wrap">
            <table class="ts-send-table" aria-label="Wpisy do wysłania">
              <thead>
                <tr>
                  <th>Zadania Todo Track</th>
                  <th>Zadanie TS</th>
                  <th class="ts-send-hours">Godziny + nadgodziny</th>
                  <th>Opis</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="e in plan.entries" :key="e.tsTaskId">
                  <td>{{ e.tasks.map((t) => t.title).join(", ") }}</td>
                  <td>
                    {{ e.tsTaskName
                    }}<span
                      v-if="cur.mode === 'keep' && replaced(e.tsTaskId)"
                      class="ts-send-tag"
                      >zastąpi {{ replaced(e.tsTaskId) }} h</span
                    >
                  </td>
                  <td
                    v-if="editing && !cur.result"
                    class="ts-send-hours ts-send-edit"
                  >
                    <span class="ts-send-amounts">
                      <input
                        :value="amounts(cur, e).hours"
                        :aria-label="'Godziny: ' + e.tsTaskName"
                        :aria-invalid="!!fieldError(cur, e)"
                        inputmode="decimal"
                        maxlength="9"
                        class="mono"
                        @input="
                          setAmount(
                            e,
                            'hours',
                            ($event.target as HTMLInputElement).value,
                          )
                        "
                      /><span aria-hidden="true">+</span
                      ><input
                        :value="amounts(cur, e).overtime"
                        :aria-label="'Nadgodziny: ' + e.tsTaskName"
                        :aria-invalid="!!fieldError(cur, e)"
                        inputmode="decimal"
                        maxlength="9"
                        class="mono"
                        @input="
                          setAmount(
                            e,
                            'overtime',
                            ($event.target as HTMLInputElement).value,
                          )
                        "
                      />
                    </span>
                    <button
                      v-if="edited(cur, e)"
                      type="button"
                      class="today-button ts-send-restore"
                      :aria-label="'Przywróć z pomiarów: ' + e.tsTaskName"
                      title="Przywróć godziny i nadgodziny z pomiarów"
                      @click="restore(e)"
                    >
                      <RotateCcw :size="14" />Przywróć z pomiarów
                    </button>
                    <span v-if="fieldError(cur, e)" class="ts-send-field-error">{{
                      fieldError(cur, e)
                    }}</span>
                  </td>
                  <td v-else class="mono ts-send-hours">
                    {{ hours(amounts(cur, e).hours)
                    }}<span
                      v-if="tsHundredths(amounts(cur, e).overtime) > 0"
                      class="ts-send-ot"
                    >
                      + {{ otText(amounts(cur, e).overtime) }} nadg.</span
                    ><span v-if="edited(cur, e)" class="ts-send-edited">
                      zmienione</span
                    >
                  </td>
                  <td>
                    <input
                      :value="description(cur, e)"
                      :aria-label="'Opis wpisu TS: ' + e.tsTaskName"
                      maxlength="2000"
                      :disabled="busy || !!cur.result"
                      @input="
                        cur.descriptions[e.tsTaskId] = (
                          $event.target as HTMLInputElement
                        ).value
                      "
                    />
                    <div
                      v-if="hintFor(cur, e)"
                      class="ts-send-hint"
                      role="status"
                      :aria-label="'Proponowany opis: ' + e.tsTaskName"
                    >
                      <span
                        >Asystent proponuje opis: „{{ hintFor(cur, e)!.text }}”<template
                          v-if="hintFor(cur, e)!.reason"
                        >
                          — {{ hintFor(cur, e)!.reason }}</template
                        ></span
                      >
                      <span class="ts-send-hint-actions">
                        <button
                          type="button"
                          class="secondary-button"
                          :disabled="busy"
                          :aria-label="'Użyj opisu: ' + e.tsTaskName"
                          @click="applyHint(cur, e, hintFor(cur, e)!)"
                        >
                          Użyj opisu</button
                        ><button
                          type="button"
                          class="today-button"
                          :aria-label="'Pomiń opis: ' + e.tsTaskName"
                          @click="skippedHints = new Set([...skippedHints, hintFor(cur, e)!.taskId])"
                        >
                          Pomiń
                        </button>
                      </span>
                    </div>
                  </td>
                </tr>
              </tbody>
              <tfoot>
                <tr>
                  <th colspan="2">Razem</th>
                  <th class="mono ts-send-hours">
                    {{ tsHoursText(curSums.total) }}
                  </th>
                  <th />
                </tr>
                <tr class="ts-send-split">
                  <th colspan="2">Godziny</th>
                  <th class="mono ts-send-hours" aria-label="Suma godzin">
                    {{ tsHoursText(curSums.hours) }}
                  </th>
                  <th class="ts-send-norm">
                    <template v-if="plan.norm === 0"
                      >dzień wolny — cała praca w nadgodzinach</template
                    ><template v-else-if="plan.norm !== null"
                      >norma dnia {{ tsHoursText(plan.norm) }} h</template
                    >
                  </th>
                </tr>
                <tr class="ts-send-split">
                  <th colspan="2">Nadgodziny</th>
                  <th class="mono ts-send-hours" aria-label="Suma nadgodzin">
                    {{ tsHoursText(curSums.overtime) }}
                  </th>
                  <th />
                </tr>
              </tfoot>
            </table>
          </div>
        </template>
        <p v-else class="report-note">
          Brak czasu z przypisanym zadaniem TS — nie ma czego wysłać.
        </p>
        <div
          v-if="
            plan.unmapped.length ||
            plan.tooShort.length ||
            plan.running ||
            missing.length ||
            overNorm !== null
          "
          class="ts-send-warnings"
          role="note"
          aria-label="Ostrzeżenia wysyłki"
        >
          <p v-if="overNorm !== null && keptHours > 0">
            Suma godzin dnia w TS {{ tsHoursText(dayHours) }} h (raport
            {{ tsHoursText(curSums.hours) }} h + pozostałe wpisy TS
            {{ tsHoursText(keptHours) }} h) przekracza normę dnia
            {{ tsHoursText(overNorm) }} h — nadwyżkę wpisz w nadgodzinach.
          </p>
          <p v-else-if="overNorm !== null">
            Suma godzin {{ tsHoursText(curSums.hours) }} h przekracza normę
            dnia {{ tsHoursText(overNorm) }} h — nadwyżkę wpisz w nadgodzinach.
          </p>
          <div v-if="plan.unmapped.length">
            <p>
              Bez zadania TS:
              {{
                tsPlural(plan.unmapped.length, "zadanie", "zadania", "zadań")
              }}
              ({{ decimalHours(plan.unmappedMs) }} h) — ten czas nie zostanie
              wysłany.
            </p>
            <p v-if="assistEnabled && !cur.loading && !cur.loadError && !cur.result">
              <button
                type="button"
                class="secondary-button"
                :disabled="busy || anySuggesting"
                title="Zapytaj asystenta AI o zadanie TS dla każdego z tych zadań (nic nie zapisze bez Twojego wyboru)"
                @click="suggestAll"
              >
                {{ suggestingAll ? "Pytam asystenta…" : "Podpowiedz wszystkie" }}
              </button>
            </p>
            <AiPreview
              v-if="assistEnabled && !cur.loading && !cur.loadError && !cur.result"
              :load="suggestPreview"
            />
            <ul class="ts-send-unmapped">
              <li v-for="u in plan.unmapped" :key="u.task.id" :data-unmapped="u.task.id">
                <span>
                  <button
                    type="button"
                    class="text-link"
                    :aria-label="'Pokaż zadanie: ' + u.task.title"
                    @click="showTask(u.task.id)"
                  >
                    {{ u.task.title }}</button
                  ><span class="mono"> · {{ decimalHours(u.ms) }} h</span>
                </span>
                <span
                  v-if="!cur.loading && !cur.loadError && !cur.result"
                  class="ts-send-assign"
                >
                  <SearchSelect
                    :model-value="''"
                    :label="'Przypisz zadanie TS: ' + u.task.title"
                    placeholder="Przypisz zadanie TS…"
                    :options="tsOptions"
                    :disabled="busy"
                    @change="assign(u.task, $event)"
                  />
                  <label v-if="projectOf(u.task)" class="inline-check">
                    <input
                      v-model="asDefault[u.task.id]"
                      type="checkbox"
                      :disabled="busy"
                    />domyślne dla projektu „{{ projectOf(u.task) }}”
                  </label>
                  <button
                    v-if="assistEnabled"
                    type="button"
                    class="today-button ts-send-suggest"
                    :aria-label="'Podpowiedz zadanie TS: ' + u.task.title"
                    :disabled="busy || anySuggesting"
                    @click="suggestFor(u.task, cur)"
                  >
                    {{ suggesting[u.task.id] ? "Pytam…" : "Podpowiedz" }}
                  </button>
                </span>
                <span
                  v-if="suggestions[u.task.id]"
                  class="ts-send-suggestion"
                  role="status"
                  :aria-label="'Podpowiedź zadania TS: ' + u.task.title"
                >
                  <template v-if="suggestedName(u.task)">
                    <strong>Asystent proponuje: {{ suggestedName(u.task) }}</strong
                    ><template v-if="suggestions[u.task.id].reason">
                      — {{ suggestions[u.task.id].reason }}</template
                    >
                    <button
                      type="button"
                      class="secondary-button"
                      :disabled="busy"
                      :aria-label="'Użyj podpowiedzi: ' + u.task.title"
                      @click="applySuggestion(u.task)"
                    >
                      Użyj podpowiedzi</button
                    ><button
                      type="button"
                      class="today-button"
                      :aria-label="'Odrzuć podpowiedź: ' + u.task.title"
                      @click="dismissSuggestion(u.task)"
                    >
                      Odrzuć
                    </button>
                  </template>
                  <template v-else>
                    {{
                      suggestions[u.task.id].tsTaskId
                        ? "Zadania TS z podpowiedzi nie ma na liście tego dnia."
                        : suggestions[u.task.id].reason
                    }}
                    <button
                      type="button"
                      class="today-button"
                      :aria-label="'Zamknij podpowiedź: ' + u.task.title"
                      @click="dismissSuggestion(u.task)"
                    >
                      Zamknij
                    </button>
                  </template>
                </span>
              </li>
            </ul>
          </div>
          <p v-for="e in plan.tooShort" :key="'short-' + e.tsTaskId">
            Zadanie TS „{{ e.tsTaskName }}” ma mniej niż 0,01 h — ten wpis nie
            zostanie wysłany, a wpis w TS o tym zadaniu zostanie bez zmian.
          </p>
          <p v-if="plan.running">
            Trwający pomiar liczony do teraz (do chwili otwarcia podglądu).
          </p>
          <p v-for="e in missing" :key="e.tsTaskId">
            Zadanie TS „{{ e.tsTaskName }}” nie występuje na liście tego dnia.
          </p>
        </div>
        <section class="ts-send-existing" aria-label="Wpisy w TS">
          <p v-if="cur.loading">Wczytywanie dnia z TS…</p>
          <template v-else-if="cur.loadError">
            <p class="error" role="alert">
              Nie udało się odczytać dnia z TS: {{ cur.loadError }}
            </p>
            <button type="button" class="today-button" @click="loadDay(cur)">
              Spróbuj ponownie
            </button>
          </template>
          <template v-else-if="!cur.result">
            <template v-if="cur.filled.length">
              <h3>
                W TS na ten dzień {{ filledCount }} (razem {{ filledHours }} h)
              </h3>
              <ul class="ts-send-filled">
                <li v-for="(f, i) in cur.filled" :key="i">
                  <strong>{{ taskName(f.id) }}</strong>
                  <span class="mono"
                    >{{ hours(f.hours) }} h<template
                      v-if="tsHundredths(f.overtime) > 0"
                    >
                      + {{ otText(f.overtime) }} nadg.</template
                    ></span
                  >
                  <span v-if="f.description" class="ts-send-desc">{{
                    f.description
                  }}</span>
                  <span v-if="cur.mode === 'replace'" class="ts-send-tag"
                    >zniknie</span
                  ><span v-else-if="ours.has(f.id.trim())" class="ts-send-tag"
                    >zostanie zastąpiony</span
                  >
                </li>
              </ul>
              <div
                class="ts-send-mode"
                role="radiogroup"
                aria-label="Wpisy już zapisane w TS"
              >
                <label class="inline-check"
                  ><input
                    v-model="cur.mode"
                    type="radio"
                    value="keep"
                  />Zachowaj pozostałe wpisy TS</label
                >
                <label class="inline-check"
                  ><input
                    v-model="cur.mode"
                    type="radio"
                    value="replace"
                  />Zastąp wszystko</label
                >
              </div>
              <p class="hint">
                {{
                  cur.mode === "keep"
                    ? "Wpisy o zadaniach spoza raportu zostaną dopisane bez zmian, wpisy o tych samych zadaniach zastąpione."
                    : "Znikną wszystkie dotychczasowe wpisy tego dnia w TS; zostaną tylko wpisy z raportu."
                }}
              </p>
            </template>
            <p v-else>Dzień w TS jest pusty.</p>
          </template>
        </section>
        <div v-if="cur.result" class="ts-send-result">
          <p v-if="cur.result.warning" :role="multi ? undefined : 'status'">
            Wysłano do TS
            {{ tsPlural(cur.result.sentCount, "wpis", "wpisy", "wpisów") }},
            razem {{ cur.result.sentHours }} h z raportu.
          </p>
          <p v-else :role="multi ? undefined : 'status'">
            W TS zapisano
            {{ tsPlural(cur.result.count, "wpis", "wpisy", "wpisów") }}, razem
            {{ cur.result.hours }} h<template v-if="cur.result.overtime > 0">
              (w tym nadgodziny
              {{ tsHoursText(cur.result.overtime) }} h)</template
            >.
          </p>
          <p
            v-if="cur.result.warning"
            class="ts-send-warning"
            :role="multi ? undefined : 'alert'"
          >
            {{ cur.result.warning }}
          </p>
          <p
            v-if="cur.result.differs"
            class="ts-send-warning"
            :role="multi ? undefined : 'alert'"
          >
            TS zapisał inne wartości niż wysłane — sprawdź w przeglądarce.
          </p>
        </div>
      </div>
      <section
        v-if="summary"
        class="ts-send-summary"
        aria-label="Podsumowanie wysyłki"
      >
        <p role="status">
          Zapisano w TS
          {{ savedDays.length }} z
          {{
            tsPlural(
              states.filter((s, i) => s.result || plans[i].entries.length)
                .length,
              "dnia",
              "dni",
              "dni",
            )
          }}, razem {{ savedTotal }} h.
        </p>
        <ul>
          <li v-for="(s, i) in states" :key="s.day">
            <strong>{{ dayText(s.day) }}:</strong>{{ " " }}
            <template v-if="s.result">
              {{
                s.result.warning
                  ? `wysłano ${tsPlural(s.result.sentCount, "wpis", "wpisy", "wpisów")} (${s.result.sentHours} h), bez odczytu po zapisie`
                  : `${tsPlural(s.result.count, "wpis", "wpisy", "wpisów")} w TS, razem ${s.result.hours} h`
              }}<template v-if="s.result.overtime > 0">
                (w tym nadgodziny {{ tsHoursText(s.result.overtime) }} h)</template
              ><span v-if="s.result.differs" class="ts-send-warning">
                — TS zapisał inne wartości niż wysłane</span
              >
            </template>
            <span v-else-if="s.error" class="ts-send-warning"
              >błąd: {{ s.error }}</span
            >
            <template v-else-if="!plans[i].entries.length"
              >brak wpisów do wysłania</template
            >
            <span v-else-if="skipReason(i)" class="ts-send-warning"
              >nie wysłano: {{ skipReason(i) }}</span
            >
            <template v-else>nie wysłano</template>
          </li>
        </ul>
      </section>
      <p v-else-if="!cur.result" class="hint">
        Zapis w TS zastępuje cały dzień. Wysyłka nie zamyka dnia w Todo Track.
      </p>
      <p
        v-if="multi && skipped.length && !busy"
        class="hint ts-send-skipped"
      >
        Pominięte przy zapisie:
        {{
          skipped
            .map(({ s, i }) => `${dayText(s.day)} (${skipReason(i)})`)
            .join(", ")
        }}.
      </p>
      <div class="modal-actions">
        <button :disabled="busy" @click="close">
          {{ savedDays.length ? "Zamknij" : "Anuluj" }}</button
        ><button
          v-if="multi ? pending.length : !cur.result"
          class="primary-button"
          :disabled="!canSave"
          @click="save"
        >
          {{ saveLabel }}
        </button>
      </div>
    </section>
  </div>
</template>
