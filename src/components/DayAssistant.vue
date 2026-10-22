<script setup lang="ts">
/**
 * Sekcja „Asystent” w Zamknięciu dnia (PLAN_BACKLOG.md, A5). „Przeanalizuj dzień”
 * (jedyne, co woła model; nigdy sama) zwraca propozycje, które są tylko kartami:
 * - luka → „Dodaj jako czas” (ręczny czas z wypełnionymi godzinami i zadaniem),
 *   a gdy luka leży między pomiarami tego zadania także „Dołącz do …” (`fill_gap`),
 * - czas agenta bez zadania → „Przypisz” (przypisanie folderu, `agent_bind`),
 * - notatka dnia → „Dodaj do notatki” (edytowalna przed zapisem); „Zastosuj
 *   wszystkie” tylko dla notatek,
 * - opis TS → proponowany opis w podglądzie „Wyślij do TS” (zmienia go dopiero
 *   „Użyj opisu” tam).
 * Czas zawsze zapisuje się pojedynczo, po kliknięciu.
 */
import { computed, reactive, ref, watch } from "vue";
import { Bot } from "lucide-vue-next";
import AiPreview from "./AiPreview.vue";
import ManualTime from "../ManualTime.vue";
import {
  aiDayAnalyze,
  aiDayPreview,
  ofKind,
  type DayAnalysis,
  type DayProposal,
  type TsDescriptionHint,
} from "../aiday";
import {
  agentTime,
  bindAgentFolder,
  duration,
  fillGap,
  read,
  type Snapshot,
} from "../store";

const props = defineProps<{
  data: Snapshot;
  day: string;
  /** Zapis notatek dnia z Zamknięcia dnia (zmienia także pola tekstowe); błąd to wyjątek. */
  appendNotes: (items: { taskId: number; text: string }[]) => Promise<void>;
}>();
const emit = defineEmits<{
  updated: [data: Snapshot];
  /** Propozycje opisów TS czekające na decyzję (do podglądu „Wyślij do TS”). */
  hints: [hints: TsDescriptionHint[]];
}>();

const analysis = ref<DayAnalysis | null>(null);
const loading = ref(false);
const error = ref("");
const busy = ref(false);
/** Karty już zastosowane albo odrzucone (po numerze propozycji). */
const applied = reactive<Record<number, boolean>>({});
const dismissed = reactive<Record<number, boolean>>({});
/** Treść notatek do edycji przed zapisem. */
const drafts = reactive<Record<number, string>>({});
/** Błędy zapisu przy karcie. */
const cardError = reactive<Record<number, string>>({});
/** Uwagi po zapisie karty (np. przypisanie bez czasu z analizowanego dnia). */
const cardInfo = reactive<Record<number, string>>({});
let request = 0;

function reset() {
  request++;
  analysis.value = null;
  loading.value = false;
  error.value = "";
  clearCards();
}
/** Czyści stan kart i zamyka formularz czasu (karty mają numery z jednej odpowiedzi). */
function clearCards() {
  for (const map of [applied, dismissed, drafts, cardError, cardInfo])
    for (const key of Object.keys(map)) delete map[Number(key)];
  preset.value = null;
}
watch(() => props.day, reset);

const previewLoad = computed(() => () => aiDayPreview(props.day));

async function analyze() {
  if (loading.value) return;
  const id = ++request;
  loading.value = true;
  error.value = "";
  try {
    const result = await aiDayAnalyze(props.day);
    if (id !== request) return;
    clearCards();
    for (const p of result.proposals) if (p.kind === "note") drafts[p.id] = p.text;
    analysis.value = result;
  } catch (e) {
    if (id !== request) return;
    error.value = String(e instanceof Error ? e.message : e);
  } finally {
    if (id === request) loading.value = false;
  }
}

const proposals = computed(() => analysis.value?.proposals ?? []);
/** Karty widoczne: bez odrzuconych. */
const visible = computed(() => proposals.value.filter((p) => !dismissed[p.id]));
const pendingNotes = computed(() =>
  ofKind(visible.value, "note").filter(
    (p) => !applied[p.id] && (drafts[p.id] ?? "").trim(),
  ),
);

// Propozycje opisów TS idą do podglądu „Wyślij do TS”, dopóki nie zostaną odrzucone.
const hints = computed<TsDescriptionHint[]>(() =>
  ofKind(proposals.value, "ts_description")
    .filter((p) => !dismissed[p.id])
    .map((p) => ({ taskId: p.taskId, text: p.text, reason: p.reason })),
);
watch(hints, (list) => emit("hints", list), { immediate: true, deep: true });

/**
 * Wykonuje zapis karty: błąd zostaje przy karcie, a karta czeka dalej. Wynik
 * zapisu, który skończył się po nowej analizie, nie oznacza kart nowej odpowiedzi.
 */
async function guard(p: DayProposal, action: () => Promise<void>) {
  if (busy.value) return;
  busy.value = true;
  const id = request;
  delete cardError[p.id];
  try {
    await action();
    if (id === request) applied[p.id] = true;
  } catch (e) {
    if (id === request) cardError[p.id] = String(e instanceof Error ? e.message : e);
  } finally {
    busy.value = false;
  }
}

// --- luka ------------------------------------------------------------------

const preset = ref<{
  start: number;
  end: number;
  key: string;
  taskId: number | null;
  card: number;
  /** Analiza, z której pochodzi karta. */
  request: number;
} | null>(null);
/** „Dodaj jako czas”: otwiera ręczny czas z godzinami luki i zadaniem z propozycji. */
function openTime(p: DayProposal) {
  if (p.fromMs === null || p.toMs === null) return;
  delete cardError[p.id];
  preset.value = {
    start: p.fromMs,
    end: p.toMs,
    key: p.id + ":" + Date.now(),
    taskId: p.taskId,
    card: p.id,
    request,
  };
}
function timeSaved(data: Snapshot) {
  if (preset.value && preset.value.request === request) applied[preset.value.card] = true;
  preset.value = null;
  emit("updated", data);
}
const fillLabel = (p: DayProposal) =>
  p.fill?.mode === "previous"
    ? "Dołącz do poprzedniego pomiaru"
    : "Dołącz do następnego pomiaru";
/** `fill_gap` (B3): cała luka do pomiaru sąsiedniego zadania. */
function fill(p: DayProposal) {
  const f = p.fill;
  if (!f) return;
  return guard(p, async () => emit("updated", await fillGap(f.beforeId, f.afterId, f.mode)));
}

// --- czas agenta -----------------------------------------------------------

const folderName = (path: string | null) =>
  (path ?? "").replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? "";
const sameFolder = (a: string, b: string) =>
  a.trim().replace(/[\\/]+$/, "").toLowerCase() ===
  b.trim().replace(/[\\/]+$/, "").toLowerCase();
function assign(p: DayProposal) {
  const folder = p.folder;
  if (!folder) return;
  const id = request;
  return guard(p, async () => {
    await bindAgentFolder(folder, p.taskId);
    emit("updated", await read());
    // Przypisanie uzupełnia tylko okresy z ostatnich 7 dni (`agent_time::backfill`):
    // przy starszym dniu czas agenta z tej analizy zostaje bez zadania.
    if (p.fromMs === null || p.toMs === null) return;
    const left = (await agentTime(p.fromMs, p.toMs)).periods.some(
      (x) => x.taskId === null && sameFolder(x.folder, folder),
    );
    if (left && id === request)
      cardInfo[p.id] =
        "Folder przypisano, ale czas agenta z tego dnia nadal jest bez zadania: przypisanie uzupełnia tylko okresy z ostatnich 7 dni.";
  });
}

// --- notatki ---------------------------------------------------------------

function addNote(p: DayProposal) {
  const text = (drafts[p.id] ?? "").trim();
  if (!text) return;
  return guard(p, () => props.appendNotes([{ taskId: p.taskId, text }]));
}
/** „Zastosuj wszystkie”: tylko notatki, jednym zapisem. */
async function addAllNotes() {
  const list = pendingNotes.value;
  if (!list.length || busy.value) return;
  busy.value = true;
  for (const p of list) delete cardError[p.id];
  try {
    await props.appendNotes(
      list.map((p) => ({ taskId: p.taskId, text: drafts[p.id].trim() })),
    );
    for (const p of list) applied[p.id] = true;
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  } finally {
    busy.value = false;
  }
}

const range = (p: DayProposal) =>
  p.fromMs !== null && p.toMs !== null
    ? `${p.from}–${p.to} · ${duration(p.toMs - p.fromMs)}`
    : "";
const tasks = computed(() => props.data.tasks.filter((t) => !t.archived));
</script>

<template>
  <section class="close-assist" aria-label="Asystent">
    <h3>Asystent</h3>
    <p class="hint">
      Asystent czyta dane tego dnia (oś dnia, czas agentów, spotkania, commity
      z przypisanych folderów, zmiany statusów, notatki) i tylko proponuje.
      Niczego nie zapisze bez Twojego kliknięcia.
    </p>
    <div class="close-assist-actions">
      <button
        type="button"
        class="secondary-button"
        :disabled="loading || busy"
        @click="analyze"
      >
        <Bot :size="14" />{{ loading ? "Analizuję dzień…" : "Przeanalizuj dzień" }}
      </button>
    </div>
    <AiPreview :load="previewLoad" />
    <p v-if="error" class="error" role="alert">{{ error }}</p>
    <p v-if="analysis && !visible.length" role="status">
      {{ proposals.length ? "Wszystkie propozycje odrzucono." : analysis.message }}
    </p>
    <template v-if="visible.length">
      <p role="status">
        Propozycje asystenta: {{ visible.length }}. Zapisujesz je pojedynczo
        przyciskami na kartach.
      </p>
      <div v-if="pendingNotes.length > 1" class="close-assist-actions">
        <button
          type="button"
          class="secondary-button"
          :disabled="busy"
          @click="addAllNotes"
        >
          Zastosuj wszystkie notatki ({{ pendingNotes.length }})
        </button>
      </div>
      <ul class="assist-cards" aria-label="Propozycje asystenta">
        <li
          v-for="p in visible"
          :key="p.id"
          class="assist-card"
          :data-kind="p.kind"
        >
          <header>
            <strong v-if="p.kind === 'gap'">Luka {{ range(p) }}</strong>
            <strong v-else-if="p.kind === 'unassigned'"
              >Praca agenta w „{{ folderName(p.folder) }}” {{ range(p) }}</strong
            >
            <strong v-else-if="p.kind === 'note'">Notatka dnia</strong>
            <strong v-else>Opis wpisu TS</strong>
            <span class="assist-card-kind">→ {{ p.taskTitle }}</span>
          </header>
          <p v-if="p.kind === 'ts_description'">„{{ p.text }}”</p>
          <label v-if="p.kind === 'note'">
            Notatka do dopisania
            <textarea
              v-model="drafts[p.id]"
              :aria-label="'Notatka do dopisania: ' + p.taskTitle"
              maxlength="1000"
              :disabled="applied[p.id]"
            />
          </label>
          <p v-if="p.reason" class="assist-card-reason">{{ p.reason }}</p>
          <p v-if="p.kind === 'ts_description'" class="assist-card-reason">
            Pojawi się w podglądzie „Wyślij do TS” jako proponowany opis; wpisu
            nie zmienia, dopóki go tam nie użyjesz.
          </p>
          <p v-if="cardError[p.id]" class="error" role="alert">
            {{ cardError[p.id] }}
          </p>
          <p v-if="cardInfo[p.id]" class="assist-card-reason" role="note">
            {{ cardInfo[p.id] }}
          </p>
          <p v-if="applied[p.id]" class="assist-card-done" role="status">
            Zastosowano.
          </p>
          <div v-else class="assist-card-actions">
            <template v-if="p.kind === 'gap'">
              <button
                type="button"
                class="secondary-button"
                :disabled="busy"
                :aria-label="'Dodaj jako czas: luka ' + p.from + '–' + p.to"
                @click="openTime(p)"
              >
                Dodaj jako czas
              </button>
              <button
                v-if="p.fill"
                type="button"
                class="secondary-button"
                :disabled="busy"
                :aria-label="fillLabel(p) + ': luka ' + p.from + '–' + p.to"
                @click="fill(p)"
              >
                {{ fillLabel(p) }}
              </button>
            </template>
            <button
              v-else-if="p.kind === 'unassigned'"
              type="button"
              class="secondary-button"
              :disabled="busy"
              :aria-label="'Przypisz: ' + folderName(p.folder)"
              @click="assign(p)"
            >
              Przypisz
            </button>
            <button
              v-else-if="p.kind === 'note'"
              type="button"
              class="secondary-button"
              :disabled="busy || !(drafts[p.id] ?? '').trim()"
              :aria-label="'Dodaj do notatki: ' + p.taskTitle"
              @click="addNote(p)"
            >
              Dodaj do notatki
            </button>
            <button
              type="button"
              class="today-button"
              :disabled="busy"
              :aria-label="'Odrzuć propozycję: ' + (p.kind === 'gap' ? p.from + '–' + p.to : p.taskTitle)"
              @click="dismissed[p.id] = true"
            >
              Odrzuć
            </button>
          </div>
        </li>
      </ul>
    </template>
    <ManualTime
      v-if="preset"
      :tasks="tasks"
      :day="day"
      :sessions="data.sessions"
      :preset="preset"
      remember-key="tuktuk-track-assistant-task"
      @updated="timeSaved"
    />
  </section>
</template>
