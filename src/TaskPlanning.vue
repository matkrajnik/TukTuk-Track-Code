<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { Plus, Trash2 } from "lucide-vue-next";
import { detailsOf, saveDetails, type Task, type Snapshot } from "./store";
import { correctionTitle, usuallyText, type CorrectionOf } from "./estimates";
import { dayKey } from "./time";
import {
  emptyRule,
  isRecurring,
  ruleText,
  ruleUsesWeekdays,
  type RepeatKind,
  type RepeatRule,
} from "./recurrence";
const props = withDefaults(
  defineProps<{
    task: Task;
    projects: string[];
    /** Karta zadania ma własny wybór priorytetu w wierszu. */
    showPriority?: boolean;
    /** Poprawka szacunków (podpowiedź „zwykle ~…” przy szacunku). */
    correction?: CorrectionOf;
  }>(),
  { showPriority: true },
);
const emit = defineEmits<{ updated: [data: Snapshot] }>();
const draft = ref(detailsOf(props.task));
const noteDay = ref(dayKey(Date.now()));
const message = ref("");
const busy = ref(false);
const dirty = ref(false);
const newCheck = ref("");
function addCheck() {
  if (newCheck.value.trim() && draft.value.checklist.length < 100) {
    draft.value.checklist.push({ text: newCheck.value.trim(), done: false });
    newCheck.value = "";
    dirty.value = true;
  }
}
/** Podpowiedź z bieżącego szkicu: szacunek i projekt przed zapisem. */
const usual = computed(() => {
  const c = props.correction?.(draft.value.project);
  const minutes = Number(draft.value.estimateMinutes) || 0;
  return c
    ? { text: usuallyText(minutes, c), title: correctionTitle(c) }
    : { text: "", title: "" };
});
const weekdays = ["Pon", "Wt", "Śr", "Czw", "Pt", "Sob", "Niedz"];
const weekdayNames = [
  "poniedziałek",
  "wtorek",
  "środa",
  "czwartek",
  "piątek",
  "sobota",
  "niedziela",
];
/** Reguła cyklu szkicu (detailsOf zawsze ją uzupełnia). */
const rule = computed<RepeatRule>(() => draft.value.repeatRule ?? emptyRule());
/** Nowy rodzaj cyklu dostaje rozsądne wartości startowe. */
function ruleKindChanged() {
  const r = rule.value;
  const from = draft.value.repeatFrom || dayKey(Date.now());
  if (r.kind === "weeks" && !r.interval) r.interval = 2;
  if (r.kind === "monthDay" && !r.day) r.day = Number(from.slice(8, 10)) || 1;
  if (r.kind === "nthWeekday") {
    if (!r.nth) r.nth = 1;
    if (!r.weekday) r.weekday = 1;
  }
}
/**
 * Pole „Cykl” (B1): „Bez cyklu” (`none`) to reguła tygodniowa bez dni; pozostałe
 * wartości to rodzaje reguły (`""` = w wybrane dni tygodnia). Wybór „W wybrane
 * dni tygodnia” przed zaznaczeniem dni pamięta `weekdaysChosen`, żeby
 * parametry cyklu nie znikały.
 */
const weekdaysChosen = ref(false);
const cycle = computed<RepeatKind | "none">({
  get: () =>
    rule.value.kind === "" &&
    !draft.value.repeatDays.length &&
    !weekdaysChosen.value
      ? "none"
      : rule.value.kind,
  set: (value) => {
    if (!draft.value.repeatRule) draft.value.repeatRule = emptyRule();
    const r = draft.value.repeatRule;
    weekdaysChosen.value = value === "";
    if (value === "none") {
      r.kind = "";
      draft.value.repeatDays = [];
      return;
    }
    r.kind = value;
    ruleKindChanged();
  },
});
watch(
  () => props.task.details?.priority,
  (value) => {
    draft.value.priority = value ?? 0;
  },
);
// Kolejność zmienia się przeciąganiem (reorder_tasks); zapis formularza nie może
// przywrócić starej wartości, więc szkic zawsze ją przejmuje.
watch(
  () => props.task.details?.order,
  (value) => {
    draft.value.order = value ?? 0;
  },
);
// Tak samo dzień planu przeniesiony na tablicy tygodnia (plan_task).
watch(
  () => props.task.details?.plannedDay,
  (value) => {
    draft.value.plannedDay = value ?? "";
  },
);
// Pola blokady zmienia też set_waiting („Odblokuj”, propozycje z Jiry,
// „Przypomnij jutro”); zapis starszego szkicu nie może ich cofnąć.
watch(
  () => {
    const d = props.task.details;
    return JSON.stringify([
      d?.waiting ?? "",
      d?.waitingSince ?? 0,
      d?.waitingStatus ?? "",
      d?.waitingDismissed ?? "",
      d?.waitingRemindDay ?? "",
    ]);
  },
  () => {
    const d = detailsOf(props.task);
    draft.value.waiting = d.waiting;
    draft.value.waitingSince = d.waitingSince;
    draft.value.waitingStatus = d.waitingStatus;
    draft.value.waitingDismissed = d.waitingDismissed;
    draft.value.waitingRemindDay = d.waitingRemindDay;
  },
);
// Zadanie TS zmienia się w polu „Zadanie TS” kafelka; zapis starszego szkicu
// nie może cofnąć wyboru.
watch(
  () => [props.task.details?.tsTaskId, props.task.details?.tsTaskName],
  () => {
    const d = props.task.details;
    if (d?.tsTaskId) {
      draft.value.tsTaskId = d.tsTaskId;
      draft.value.tsTaskName = d.tsTaskName ?? "";
    } else {
      delete draft.value.tsTaskId;
      delete draft.value.tsTaskName;
    }
  },
);
watch(
  () => props.task.details,
  () => {
    if (!dirty.value) {
      draft.value = structuredClone(detailsOf(props.task));
      weekdaysChosen.value = false;
    }
  },
);
async function save() {
  busy.value = true;
  message.value = "";
  try {
    const result = await saveDetails(props.task.id, draft.value);
    dirty.value = false;
    weekdaysChosen.value = false;
    draft.value = structuredClone(
      detailsOf(result.tasks.find((t) => t.id === props.task.id)!),
    );
    emit("updated", result);
    message.value = "Zapisano plan i notatki.";
  } catch (e) {
    message.value = String(e);
  } finally {
    busy.value = false;
  }
}
</script>
<template>
  <form
    class="planning-form"
    @submit.prevent="save"
    @input="
      dirty = true;
      message = '';
    "
    @change="dirty = true"
  >
    <h3>Plan i notatki</h3>
    <div class="planning-fields">
      <label v-if="showPriority"
        >Priorytet<select
          v-model.number="draft.priority"
          aria-label="Priorytet"
        >
          <option :value="1">Wysoki</option>
          <option :value="0">Normalny</option>
          <option :value="-1">Niski</option>
        </select></label
      >
      <label
        >Planowany dzień<input v-model="draft.plannedDay" type="date"
      /></label>
      <label>Termin<input v-model="draft.dueDay" type="date" /></label>
      <label
        >Projekt / obszar<input
          v-model="draft.project"
          :list="'projects-' + task.id"
          maxlength="80"
          placeholder="np. WMS"
      /></label>
    </div>
    <datalist :id="'projects-' + task.id">
      <option v-for="project in projects" :key="project" :value="project" />
    </datalist>
    <div class="planning-fields">
      <label
        >Szacowany czas (minuty)<input
          v-model.number="draft.estimateMinutes"
          type="number"
          min="0"
          max="600000"
          step="1"
          :aria-describedby="usual.text ? 'estimate-usual-' + task.id : undefined"
      /></label>
      <label
        >Godzina startu w planie<input
          v-model="draft.plannedStart"
          type="time"
          step="60"
          title="Opcjonalnie. Bez godziny blok planu układa się za poprzednim."
      /></label>
    </div>
    <p
      v-if="usual.text"
      :id="'estimate-usual-' + task.id"
      class="estimate-hint"
      :title="usual.title"
    >
      Szacunek {{ Number(draft.estimateMinutes) }} min · {{ usual.text }}
    </p>
    <div class="planning-fields repeat-rule">
      <label
        >Cykl<select v-model="cycle" aria-label="Rodzaj cyklu">
          <option value="none">Bez cyklu</option>
          <option value="">W wybrane dni tygodnia</option>
          <option value="weeks">Co N tygodni</option>
          <option value="monthDay">Miesięcznie w dniu</option>
          <option value="lastWorkday">Ostatni dzień roboczy miesiąca</option>
          <option value="nthWeekday">N-ty dzień tygodnia w miesiącu</option>
        </select></label
      >
      <label v-if="cycle !== 'none'"
        >Początek cyklu<input v-model="draft.repeatFrom" type="date"
      /></label>
      <label v-if="rule.kind === 'weeks'"
        >Co ile tygodni<input
          v-model.number="rule.interval"
          type="number"
          min="1"
          max="8"
          step="1"
      /></label>
      <label v-if="rule.kind === 'monthDay'"
        >Dzień miesiąca<input
          v-model.number="rule.day"
          type="number"
          min="1"
          max="31"
          step="1"
          title="W krótszym miesiącu zadanie wypada w jego ostatnim dniu."
      /></label>
      <template v-if="rule.kind === 'nthWeekday'">
        <label
          >Który<select v-model.number="rule.nth" aria-label="Który w miesiącu">
            <option :value="1">1.</option>
            <option :value="2">2.</option>
            <option :value="3">3.</option>
            <option :value="4">4.</option>
            <option :value="-1">Ostatni</option>
          </select></label
        >
        <label
          >Dzień tygodnia<select
            v-model.number="rule.weekday"
            aria-label="Dzień tygodnia cyklu"
          >
            <option
              v-for="(name, index) in weekdayNames"
              :key="name"
              :value="index + 1"
            >
              {{ name }}
            </option>
          </select></label
        >
      </template>
    </div>
    <fieldset
      v-if="cycle !== 'none' && ruleUsesWeekdays(rule.kind)"
      class="repeat-options"
    >
      <legend>Powtarzaj w wybrane dni</legend>
      <label v-for="(name, index) in weekdays" :key="name" class="inline-check"
        ><input
          v-model="draft.repeatDays"
          type="checkbox"
          :value="index + 1"
        />{{ name }}</label
      >
    </fieldset>
    <p v-if="isRecurring(draft)" class="hint">
      Każda data tworzy osobne zadanie ({{ ruleText(draft) }}). Kopie mają
      wyzerowaną checklistę. Po przerwie uzupełniane są maksymalnie ostatnie 366
      dni. Aby wyłączyć cykl, wybierz „Bez cyklu”; archiwizacja też go
      wstrzymuje.
    </p>
    <div class="task-checklist">
      <h3>Lista kroków</h3>
      <div
        v-for="(item, index) in draft.checklist"
        :key="index"
        class="checklist-row"
        :class="{ done: item.done }"
      >
        <input
          v-model="item.done"
          type="checkbox"
          :aria-label="'Gotowe: ' + item.text"
        /><input
          v-model="item.text"
          :aria-label="'Krok ' + (index + 1)"
          maxlength="240"
          required
        /><button
          type="button"
          class="today-button"
          :aria-label="'Usuń krok ' + (index + 1)"
          @click="
            draft.checklist.splice(index, 1);
            dirty = true;
          "
        >
          <Trash2 :size="14" />Usuń
        </button>
      </div>
      <div class="checklist-add">
        <input
          v-model="newCheck"
          aria-label="Nowy krok"
          maxlength="240"
          placeholder="Dodaj krok…"
          @keydown.enter.prevent="addCheck"
        /><button
          type="button"
          class="today-button"
          :disabled="!newCheck.trim() || draft.checklist.length >= 100"
          @click="addCheck"
        >
          <Plus :size="14" />Dodaj krok
        </button>
      </div>
    </div>
    <label
      >Czekam na…<input
        v-model="draft.waiting"
        maxlength="300"
        placeholder="Puste pole oznacza brak blokady"
    /></label>
    <p v-if="draft.waiting" class="hint">
      Zapisanie blokady zatrzyma aktywny pomiar tego zadania. Start usunie
      blokadę.
    </p>
    <div class="note-heading">
      <label
        >Dzień notatki<input v-model="noteDay" type="date" required
      /></label>
    </div>
    <label
      >Co zrobiłem<textarea
        v-model="draft.notes[noteDay]"
        maxlength="4000"
        rows="3"
        placeholder="Krótki opis wykonanej pracy w wybranym dniu"
      />
    </label>
    <div class="planning-actions">
      <button class="primary-button" :disabled="busy">
        Zapisz plan i notatki</button
      ><span role="status">{{ message }}</span>
    </div>
  </form>
</template>
