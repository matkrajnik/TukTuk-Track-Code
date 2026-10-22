<script setup lang="ts">
/**
 * Tablica Jiry (J2): kolumny z kartami, przejścia przez przeciągnięcie albo
 * „Przesuń do…”, wybór przejścia przy kilku możliwych. Stan przesuwania
 * (`pending`, błąd tablicy, otwarte menu) jest tutaj; dane tablicy pobiera
 * `JiraView`. Nic nie dzieje się samo: tylko przeciągnięcie albo kliknięcie
 * wywołuje przejście w Jirze.
 */
import { computed, ref, watch } from "vue";
import { Ellipsis, ExternalLink, Play, Square } from "lucide-vue-next";
import JiraIssueActions from "../JiraIssueActions.vue";
import JiraFieldChips from "../JiraFieldChips.vue";
import {
  duration,
  initials,
  jiraBoardMove,
  jiraOpen,
  type JiraBoardIssue,
  type JiraBoardView,
  type JiraMoveOption,
  type JiraPerson,
  type JiraScope,
} from "../../store";

/** Czas i plan lokalnego zadania zgłoszenia (klucz → wartości na karcie). */
interface LocalIssue {
  ms: number;
  running: boolean;
  planned: string;
}
const props = defineProps<{
  board: JiraBoardView;
  scope: JiraScope;
  /** Trwa odczyt w widoku (przyciski Start/Stop nieaktywne). */
  busy: boolean;
  /** Zapisy do Jiry włączone; inaczej tablica tylko do odczytu. */
  writable: boolean;
  /** Filtr wyszukiwania (klucz lub nazwa). */
  query: string;
  updatedAt: string;
  activeUrl?: string | null;
  local: Map<string, LocalIssue>;
  /**
   * Udane przesunięcie: komunikat i odczyt danych w `JiraView`. Wywołanie
   * zwrotne zamiast zdarzenia, bo odświeżenie tablicy może odmontować ten
   * komponent w trakcie przesuwania, a zdarzenia odmontowanego Vue pomija.
   */
  moved: (notice: string) => Promise<void>;
}>();
const emit = defineEmits<{
  open: [url: string];
  start: [key: string];
  stop: [];
  notice: [text: string];
}>();

const boardError = ref("");
const openMenu = ref<string | null>(null);
const dragging = ref<string | null>(null);
const dropTarget = ref<number | null>(null);
const pending = ref<string[]>([]);
/** Kilka przejść do kolumny: wybór użytkownika (karta czeka w nowej kolumnie). */
const choice = ref<{
  card: JiraBoardIssue;
  column: number;
  from: number;
  options: JiraMoveOption[];
} | null>(null);
// Nowa tablica (odświeżenie, inna tablica albo zakres) zaczyna bez stanu.
watch(
  () => props.board,
  () => {
    boardError.value = "";
    choice.value = null;
    openMenu.value = null;
  },
);

const matches = (text: string) =>
  text
    .toLocaleLowerCase("pl")
    .includes(props.query.trim().toLocaleLowerCase("pl"));
const columnCards = computed(() =>
  props.board.columns.map((_, i) =>
    props.board.issues.filter(
      (c) => c.column === i && matches(`${c.key} ${c.summary}`),
    ),
  ),
);

function onDragStart(event: DragEvent, card: JiraBoardIssue) {
  if (!props.writable || pending.value.includes(card.key)) {
    event.preventDefault();
    return;
  }
  dragging.value = card.key;
  event.dataTransfer?.setData("text/plain", card.key);
  if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
}
function onDragOver(event: DragEvent, column: number) {
  if (!dragging.value) return;
  event.preventDefault();
  if (event.dataTransfer) event.dataTransfer.dropEffect = "move";
  dropTarget.value = column;
}
function onDrop(event: DragEvent, column: number) {
  event.preventDefault();
  const key = dragging.value || event.dataTransfer?.getData("text/plain");
  dragging.value = null;
  dropTarget.value = null;
  const card = props.board.issues.find((c) => c.key === key);
  if (card) void moveCard(card, column);
}
function onDragEnd() {
  dragging.value = null;
  dropTarget.value = null;
}
/**
 * Optymistyczne przesunięcie karty; błąd, brak przejścia albo przejście
 * wymagające pól cofa kartę z komunikatem.
 */
async function moveCard(
  card: JiraBoardIssue,
  column: number,
  transitionId: string | null = null,
  from = card.column,
) {
  if (!props.writable) return;
  if (pending.value.includes(card.key)) return;
  // Otwarty wybór przejścia: karta stoi w kolumnie docelowej tylko
  // optymistycznie. Prawdziwa kolumna to choice.from.
  if (choice.value?.card === card) {
    card.column = choice.value.from;
    from = choice.value.from;
    choice.value = null;
  }
  if (card.column === column && transitionId === null) return;
  const id = props.board.boardId;
  card.column = column;
  openMenu.value = null;
  choice.value = null;
  emit("notice", "");
  boardError.value = "";
  pending.value = [...pending.value, card.key];
  const columnName = props.board.columns[column]?.name ?? "";
  try {
    const out = await jiraBoardMove(id, card.key, column, transitionId);
    if (out.result === "moved") {
      Object.assign(card, {
        status: out.status,
        statusId: out.statusId,
        category: out.category,
        column: out.column,
      });
      await props.moved(`${card.key} → ${out.status}.`);
    } else if (out.result === "choose") {
      choice.value = { card, column, from, options: out.options };
    } else {
      card.column = from;
      boardError.value = out.message;
      await jiraOpen(out.url);
    }
  } catch (e) {
    card.column = from;
    boardError.value = `Nie przesunięto ${card.key} do „${columnName}”. ${String(
      e instanceof Error ? e.message : e,
    )}`;
  } finally {
    pending.value = pending.value.filter((k) => k !== card.key);
  }
}
function cancelChoice() {
  if (choice.value) choice.value.card.column = choice.value.from;
  choice.value = null;
}
function assigned(card: JiraBoardIssue, person: JiraPerson) {
  card.assignee = { ...person };
}
</script>

<template>
  <p class="jira-result-info">
    {{ scope === "mine" ? "Moje" : "Cała tablica" }} ·
    {{ board.issues.length }} zgłoszeń<span v-if="updatedAt">
      · Odświeżono {{ updatedAt }}</span
    >
  </p>
  <p v-if="board.truncated" class="hint jira-truncated" role="status">
    Tablica ma więcej niż {{ board.limit }} otwartych zgłoszeń — pokazano
    pierwsze {{ board.limit }}. Zawęź zakres do „Moje” albo szukaj po kluczu.
  </p>
  <p v-if="!writable" class="hint jira-read-only">
    Tablica tylko do odczytu: zmiany w Jirze są wyłączone (Ustawienia → Jira).
  </p>
  <p v-if="boardError" class="jira-move-error" role="alert">
    {{ boardError }}
  </p>
  <div
    v-if="choice"
    class="jira-move-list"
    role="group"
    :aria-label="'Wybierz przejście: ' + choice.card.key"
  >
    <span
      >{{ choice.card.key }} do „{{ board.columns[choice.column]?.name }}”:</span
    >
    <button
      v-for="o in choice.options"
      :key="o.id"
      class="action-button"
      :aria-label="`Przesuń ${choice.card.key} do: ${o.toStatus}`"
      @click="moveCard(choice.card, choice.column, o.id, choice.from)"
    >
      {{ o.name && o.name !== o.toStatus ? `${o.name} → ${o.toStatus}` : o.toStatus }}
    </button>
    <button class="action-button" @click="cancelChoice">Anuluj</button>
  </div>
  <div class="jira-board" :aria-busy="busy || !!pending.length">
    <section
      v-for="(column, index) in board.columns"
      :key="index"
      class="jira-column"
      :class="{ dropping: dropTarget === index }"
      :aria-label="'Kolumna ' + column.name"
      @dragover="onDragOver($event, index)"
      @dragleave="dropTarget === index && (dropTarget = null)"
      @drop="onDrop($event, index)"
    >
      <header>
        <h2>{{ column.name }}</h2>
        <span
          class="jira-column-count"
          :aria-label="'Zgłoszeń w kolumnie: ' + columnCards[index].length"
          >{{ columnCards[index].length }}</span
        >
      </header>
      <article
        v-for="card in columnCards[index]"
        :key="card.key"
        class="jira-card"
        :class="{
          isRunning: activeUrl === card.url,
          isPending: pending.includes(card.key),
        }"
        :data-key="card.key"
        :draggable="writable && !pending.includes(card.key)"
        @dragstart="onDragStart($event, card)"
        @dragend="onDragEnd"
      >
        <div class="jira-card-meta">
          <button
            class="text-link mono"
            :aria-label="'Otwórz w Jirze: ' + card.key"
            title="Otwórz w Jirze"
            @click="emit('open', card.url)"
          >
            {{ card.key }}<ExternalLink :size="11" /></button
          ><span v-if="card.issueType" class="jira-tag">{{
            card.issueType
          }}</span
          ><span
            v-if="card.priority"
            class="jira-tag jira-priority"
            :class="card.priority.toLowerCase()"
            :title="'Priorytet: ' + card.priority"
            >{{ card.priority }}</span
          >
          <span
            class="jira-avatar"
            :title="card.assignee?.displayName ?? 'Nieprzypisane'"
            :aria-label="
              card.assignee
                ? 'Przypisane: ' + card.assignee.displayName
                : 'Nieprzypisane'
            "
            >{{ card.assignee ? initials(card.assignee.displayName) : "–" }}</span
          >
        </div>
        <h3>{{ card.summary }}</h3>
        <div v-if="card.fields?.length" class="jira-card-fields">
          <JiraFieldChips :fields="card.fields" />
        </div>
        <div class="jira-card-local">
          <span v-if="activeUrl === card.url" class="task-status task-live"
            >mierzone teraz</span
          ><span
            v-if="local.get(card.key)?.ms"
            class="mono"
            :title="'Czas zmierzony w TukTuk Track'"
            >{{ duration(local.get(card.key)!.ms) }}</span
          ><span v-if="local.get(card.key)?.planned" class="task-chip task-planned"
            >zaplanowane na {{ local.get(card.key)!.planned }}</span
          ><span
            v-if="card.statusId === '' || !column.statusIds.includes(card.statusId)"
            class="jira-status"
            >{{ card.status }}</span
          >
        </div>
        <div class="jira-card-actions">
          <button
            v-if="activeUrl === card.url"
            class="row-timer stop"
            :disabled="busy"
            :aria-label="'Zatrzymaj: ' + card.key"
            @click="emit('stop')"
          >
            <Square :size="12" />Stop</button
          ><button
            v-else
            class="row-timer"
            :disabled="busy"
            :aria-label="'Rozpocznij: ' + card.key"
            @click="emit('start', card.key)"
          >
            <Play :size="12" />Start</button
          ><button
            class="icon-button"
            :aria-label="'Akcje: ' + card.key"
            :aria-expanded="openMenu === card.key"
            title="Przesuń do…, przypisz, komentarz, gałąź"
            @click="openMenu = openMenu === card.key ? null : card.key"
          >
            <Ellipsis :size="16" />
          </button>
        </div>
        <div
          v-if="openMenu === card.key"
          class="jira-card-menu"
          @keydown.esc.stop="openMenu = null"
        >
          <div
            v-if="writable"
            class="jira-card-move"
            role="group"
            :aria-label="'Przesuń do: ' + card.key"
          >
            <span class="hint">Przesuń do…</span>
            <button
              v-for="(target, ti) in board.columns"
              v-show="ti !== card.column"
              :key="ti"
              class="action-button"
              :disabled="pending.includes(card.key)"
              :aria-label="`Przesuń ${card.key} do kolumny: ${target.name}`"
              @click="moveCard(card, ti)"
            >
              {{ target.name }}
            </button>
          </div>
          <JiraIssueActions
            :issue-key="card.key"
            :summary="card.summary"
            @assigned="assigned(card, $event)"
          />
        </div>
      </article>
      <p v-if="!columnCards[index].length" class="hint jira-column-empty">
        Brak zgłoszeń
      </p>
    </section>
  </div>
</template>
