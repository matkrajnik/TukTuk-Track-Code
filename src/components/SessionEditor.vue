<script setup lang="ts">
import { computed, ref, watch } from "vue";
import ConfirmDialog from "./ConfirmDialog.vue";
import {
  duration,
  editSession,
  splitSession,
  type Session,
  type Snapshot,
  type Task,
} from "../store";
import { localInput, stamp } from "../time";

export type SessionTab = "hours" | "split";
const props = withDefaults(
  defineProps<{
    session: Session;
    // Bieżące przedziały z migawki: pozwalają zauważyć zatrzymanie pomiaru
    // (tray, skrót) przy otwartym oknie, bo `session` jest kopią z chwili otwarcia.
    sessions?: Session[];
    tasks: Task[];
    now: number;
    tab?: SessionTab;
  }>(),
  { tab: "hours" },
);
const emit = defineEmits<{ updated: [data: Snapshot]; close: [] }>();
// Busy is owned by App so closeModal() (e.g. on "prompt-task") and other
// actions wait for edit_session, as before the component split.
const busy = defineModel<boolean>("busy", { default: false });
const error = ref("");
// Stan przedziału bierzemy z aktualnej migawki; kopia z otwarcia jest tylko zapasem.
const current = computed(
  () =>
    props.sessions?.find((s) => s.id === props.session.id) ?? props.session,
);
const openedRunning = props.session.endedAt === null;
const running = computed(() => current.value.endedAt === null);
// Pomiar zatrzymano z zewnątrz (tray, skrót), gdy okno było otwarte.
const stoppedMeanwhile = computed(() => openedRunning && !running.value);
const tab = ref<SessionTab>(props.tab);
const editStart = ref(localInput(props.session.startedAt));
const editEnd = ref(openedRunning ? "" : localInput(props.session.endedAt!));
watch(
  () => current.value.endedAt,
  (ended, previous) => {
    if (ended !== null && previous === null) editEnd.value = localInput(ended);
  },
);

// Domyślny moment podziału: środek przedziału (dla aktywnego do chwili otwarcia),
// zaokrąglony do pełnej sekundy, bo pole datetime-local ma krok 1 s.
const openedEnd = props.session.endedAt ?? props.now;
const defaultSplit =
  Math.floor((props.session.startedAt + openedEnd) / 2 / 1000) * 1000;
const splitAt = ref(localInput(defaultSplit));
const target = ref(props.session.taskId);
const taskTitle = (id: number) =>
  props.tasks.find((t) => t.id === id)?.title ?? "Zadanie";
// Zarchiwizowane zadanie też może być celem: podział zdejmie mu archiwum
// (dla aktywnej części tak jak Start). Aktywna druga część podlega regułom
// Startu, więc ukończone zadania odpadają tylko wtedy.
const targets = computed(() =>
  props.tasks.filter(
    (t) => t.id === props.session.taskId || !(running.value && t.done),
  ),
);
watch(targets, (list) => {
  if (!list.some((t) => t.id === target.value))
    target.value = props.session.taskId;
});
const splitTime = computed(() =>
  splitAt.value === localInput(defaultSplit)
    ? defaultSplit
    : new Date(splitAt.value).getTime(),
);
const end = computed(() => current.value.endedAt ?? props.now);
const splitValid = computed(
  () =>
    Number.isFinite(splitTime.value) &&
    current.value.startedAt < splitTime.value &&
    splitTime.value < end.value,
);

function close() {
  if (!busy.value) emit("close");
}
function selectTab(next: SessionTab) {
  tab.value = next;
  error.value = "";
}
async function save() {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    const s = current.value;
    // Preserve milliseconds and the exact offset of unchanged timestamps (including DST folds).
    const start =
      editStart.value === localInput(s.startedAt)
        ? s.startedAt
        : new Date(editStart.value).getTime();
    const end =
      editEnd.value === localInput(s.endedAt!)
        ? s.endedAt!
        : new Date(editEnd.value).getTime();
    if (!Number.isFinite(start) || !Number.isFinite(end))
      throw new Error("Wprowadź poprawną datę i godzinę.");
    emit("updated", await editSession(s.id, start, end));
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}
async function split() {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    if (!Number.isFinite(splitTime.value))
      throw new Error("Wprowadź poprawną datę i godzinę.");
    emit(
      "updated",
      await splitSession(
        props.session.id,
        splitTime.value,
        target.value === props.session.taskId ? null : target.value,
      ),
    );
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <ConfirmDialog
    title="Edytuj przedział"
    :error="error"
    :busy="busy"
    @close="close"
  >
    <div class="tabs session-tabs" role="tablist" aria-label="Tryb edycji">
      <button
        id="session-tab-hours"
        type="button"
        role="tab"
        :aria-selected="tab === 'hours'"
        aria-controls="session-panel-hours"
        :class="{ selected: tab === 'hours' }"
        @click="selectTab('hours')"
      >
        Godziny</button
      ><button
        id="session-tab-split"
        type="button"
        role="tab"
        :aria-selected="tab === 'split'"
        aria-controls="session-panel-split"
        :class="{ selected: tab === 'split' }"
        @click="selectTab('split')"
      >
        Podziel
      </button>
    </div>
    <div
      v-if="tab === 'hours'"
      id="session-panel-hours"
      role="tabpanel"
      aria-labelledby="session-tab-hours"
    >
      <template v-if="running">
        <p class="hint">
          Pomiar nadal trwa. Zatrzymaj go, aby zmienić godziny, albo podziel
          przedział w zakładce Podziel.
        </p>
        <div class="modal-actions">
          <button type="button" :disabled="busy" @click="close">Zamknij</button>
        </div>
      </template>
      <form v-else @submit.prevent="save">
        <label for="session-start">Początek</label
        ><input
          id="session-start"
          v-model="editStart"
          type="datetime-local"
          step="1"
          required
          :disabled="busy"
        /><label for="session-end">Koniec</label
        ><input
          id="session-end"
          v-model="editEnd"
          type="datetime-local"
          step="1"
          required
          :disabled="busy"
        />
        <p class="hint">
          Czas lokalny. Zmiana przeliczy czas zadania i raporty dzienne.
        </p>
        <div class="modal-actions">
          <button type="button" :disabled="busy" @click="close">Anuluj</button
          ><button class="primary-button" :disabled="busy">
            Zapisz przedział
          </button>
        </div>
      </form>
    </div>
    <form
      v-else
      id="session-panel-split"
      role="tabpanel"
      aria-labelledby="session-tab-split"
      @submit.prevent="split"
    >
      <label for="split-at">Moment podziału</label
      ><input
        id="split-at"
        v-model="splitAt"
        type="datetime-local"
        step="1"
        required
        :disabled="busy"
      /><label for="split-target">Zadanie drugiej części</label
      ><select
        id="split-target"
        v-model="target"
        class="split-target"
        :disabled="busy"
      >
        <option v-for="task in targets" :key="task.id" :value="task.id">
          {{ task.archived ? `${task.title} (archiwum)` : task.title }}
        </option>
      </select>
      <ol class="split-preview" aria-label="Podgląd podziału">
        <li>
          <strong>{{ taskTitle(session.taskId) }}</strong>
          <span
            >{{ stamp(current.startedAt) }} →
            {{ splitValid ? stamp(splitTime) : "…" }}</span
          ><span class="mono">{{
            splitValid ? duration(splitTime - current.startedAt) : "—"
          }}</span>
        </li>
        <li>
          <strong>{{ taskTitle(target) }}</strong>
          <span
            >{{ splitValid ? stamp(splitTime) : "…" }} →
            {{ running ? "trwa" : stamp(end) }}</span
          ><span class="mono">{{
            splitValid ? duration(end - splitTime) : "—"
          }}</span>
        </li>
      </ol>
      <p v-if="stoppedMeanwhile" class="hint">
        Pomiar został już zatrzymany o {{ stamp(end) }}. Druga część nie będzie
        trwała dalej, a moment podziału musi być wcześniejszy.
      </p>
      <p v-if="!splitValid" class="hint">
        Moment podziału musi leżeć wewnątrz przedziału.
      </p>
      <p v-else-if="running" class="hint">
        Pomiar biegnie dalej pod zadaniem drugiej części.
      </p>
      <div class="modal-actions">
        <button type="button" :disabled="busy" @click="close">Anuluj</button
        ><button class="primary-button" :disabled="busy || !splitValid">
          Podziel przedział
        </button>
      </div>
    </form>
  </ConfirmDialog>
</template>
