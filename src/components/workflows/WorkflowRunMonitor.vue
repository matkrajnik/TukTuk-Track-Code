<script setup lang="ts">
/**
 * Monitor przebiegu (E5): stan, kroki, oś zdarzeń, czasy, zgoda i zatrzymanie.
 * Przy montowaniu najpierw rejestruje nasłuch `workflow-event` (zdarzenia z
 * czasu odczytu trafiają do bufora), potem pobiera szczegóły i strony zdarzeń,
 * a na końcu scala bufor; duplikaty sekwencji są pomijane, a luka powoduje
 * dociągnięcie od ostatniej znanej sekwencji. Pauzy na żądanie nie ma (tylko
 * „Zatrzymaj”); przebieg wstrzymany po restarcie aplikacji (`paused`) ma „Wznów”
 * z jednym identyfikatorem żądania na monitor (podwójne kliknięcie = jedno wznowienie).
 * „Akceptuj” jest aktywne dopiero po wyświetleniu całego diffu: zgoda dotyczy
 * tego, co użytkownik zobaczył, więc diff przycięty (ponad 2 MB) można tylko
 * odrzucić. Szczegóły przebiegu odświeżają się tylko po zdarzeniach zmieniających
 * stan (nie po komunikatach agentów), a starsza odpowiedź nie nadpisuje nowszej.
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import ConfirmDialog from "../ConfirmDialog.vue";
import WorkflowGraphView from "./WorkflowGraphView.vue";
import {
  EXECUTION_STATUS_LABELS,
  FAILURE_MESSAGES,
  INVALIDATED_PREFIX,
  MAX_APPROVAL_COMMENT_CHARS,
  MAX_LOG_LINES_RENDERED,
  MAX_LOG_LINE_CHARS,
  PAUSED_HINT,
  RUN_STATUS_LABELS,
  formatDuration,
  isTerminal,
  mergeEvents,
  newRequestId,
  cancelWorkflowRun,
  decideWorkflowRun,
  getWorkflowRun,
  listenWorkflowEvents,
  readWorkflowArtifact,
  recoverWorkflowRun,
  recoveryMessage,
  resumeWorkflowRun,
  unconfirmedNote,
  workflowRunEvents,
  type DiffArtifact,
  type RunDetail,
  type StoredEvent,
} from "../../workflowRuns";

const props = defineProps<{ runId: number }>();

const detail = ref<RunDetail | null>(null);
const events = ref<StoredEvent[]>([]);
const error = ref("");
const notice = ref("");
const showAll = ref(false);
const confirmCancel = ref(false);
const diff = ref<DiffArtifact | null>(null);
const diffError = ref("");
const diffLoading = ref(false);
const comment = ref("");
const deciding = ref(false);
const recovering = ref(false);
const resuming = ref(false);
/** Jedno id żądania wznowienia na monitor przebiegu (monitor ma klucz = przebieg). */
const resumeRequestId = newRequestId();
const confirmRecover = ref(false);
/**
 * Jeden identyfikator żądania na zgodę: podwójne kliknięcie ma jeden skutek, a
 * kolejna zgoda w tym samym przebiegu (nowe id, także po unieważnieniu) dostaje
 * nowy identyfikator, bo host odrzuca id użyte już przy innej zgodzie.
 */
const decideRequestIds = new Map<string, string>();
function requestIdFor(approvalId: string): string {
  let id = decideRequestIds.get(approvalId);
  if (!id) {
    id = newRequestId();
    decideRequestIds.set(approvalId, id);
  }
  return id;
}
let unlisten: (() => void) | null = null;
let generation = 0;
/** Numer odczytu szczegółów: odpowiedź starszego wywołania jest pomijana. */
let refreshSeq = 0;
/** Zdarzenia strumienia, które nie zmieniają stanu przebiegu ani zgody. */
const LOG_ONLY_EVENTS: ReadonlySet<string> = new Set([
  "message",
  "toolStarted",
  "toolFinished",
]);
/** Zdarzenia z czasu wczytywania snapshotu; scalane po jego zakończeniu. */
let buffered: StoredEvent[] | null = null;

const status = computed(() =>
  detail.value ? RUN_STATUS_LABELS[detail.value.summary.status] : null,
);
const pending = computed(
  () => detail.value?.approvals.find((a) => a.status === "pending") ?? null,
);
/** Diff aktualnej zgody wczytany (może być przycięty do pokazania). */
const diffLoaded = computed(
  () => !!pending.value && diff.value?.hash === pending.value.artifactHash,
);
/** Cały diff wyświetlony: dopiero wtedy można zaakceptować. */
const diffShown = computed(() => diffLoaded.value && !diff.value?.truncated);

const logLines = computed(() =>
  events.value
    .filter((e) => e.type === "message")
    .map((e) => {
      const text = String(e.payload.text ?? JSON.stringify(e.payload));
      return {
        sequence: e.sequence,
        text:
          text.length > MAX_LOG_LINE_CHARS
            ? `${text.slice(0, MAX_LOG_LINE_CHARS)}…`
            : text,
      };
    }),
);
const hiddenLines = computed(() =>
  showAll.value ? 0 : Math.max(0, logLines.value.length - MAX_LOG_LINES_RENDERED),
);
const visibleLines = computed(() =>
  showAll.value
    ? logLines.value
    : logLines.value.slice(-MAX_LOG_LINES_RENDERED),
);

const timeline = computed(() =>
  events.value
    .filter((e) => e.type !== "message" && e.type !== "toolStarted")
    .slice(-50),
);

/** Status ostatniego wykonania każdego kroku (tekstem) dla grafu w monitorze. */
const nodeStatuses = computed(() => {
  const result: Record<string, string> = {};
  for (const e of detail.value?.executions ?? [])
    result[e.nodeId] = EXECUTION_STATUS_LABELS[e.status].label;
  return result;
});
const noProblems: ReadonlySet<string> = new Set();

function nodeLabel(nodeId: string): string {
  return detail.value?.graph.nodes.find((n) => n.id === nodeId)?.label ?? nodeId;
}

async function catchUp(after: number) {
  for (;;) {
    const page = await workflowRunEvents(props.runId, after, 500);
    if (!page.length) return;
    const merged = mergeEvents(events.value, page);
    events.value = merged.events;
    after = page[page.length - 1].sequence;
    if (page.length < 500) return;
  }
}

async function refreshDetail() {
  const current = generation;
  const mine = ++refreshSeq;
  const fresh = await getWorkflowRun(props.runId);
  if (current === generation && mine === refreshSeq) detail.value = fresh;
}

async function load() {
  const current = ++generation;
  error.value = "";
  events.value = [];
  diff.value = null;
  unlisten?.();
  unlisten = null;
  buffered = [];
  try {
    // Nasłuch przed snapshotem: zdarzenie z okna odczytu (np. koniec przebiegu)
    // nie ginie, tylko czeka w buforze.
    const stop = await listenWorkflowEvents((event) => {
      if (current !== generation) return;
      if (buffered) buffered.push(event);
      else void onEvent(event);
    });
    if (current !== generation) {
      // Odmontowanie albo zmiana przebiegu w trakcie rejestracji.
      stop();
      return;
    }
    unlisten = stop;
    await refreshDetail();
    await catchUp(0);
    if (current !== generation) return;
    const pendingEvents = buffered ?? [];
    buffered = null;
    for (const event of pendingEvents) await onEvent(event);
  } catch (e) {
    buffered = null;
    if (current === generation) error.value = String(e);
  }
}

async function onEvent(event: StoredEvent) {
  if (event.runId !== props.runId) return;
  const merged = mergeEvents(events.value, [event]);
  events.value = merged.events;
  if (merged.gapAfter !== null) {
    try {
      await catchUp(merged.gapAfter);
    } catch (e) {
      error.value = String(e);
    }
  }
  // Komunikaty agentów (tysiące na krok) nie zmieniają stanu: bez pełnego odczytu.
  if (LOG_ONLY_EVENTS.has(event.type) && merged.gapAfter === null) return;
  try {
    await refreshDetail();
  } catch (e) {
    error.value = String(e);
  }
}

async function showDiff() {
  const approval = pending.value;
  if (!approval) return;
  diffLoading.value = true;
  diffError.value = "";
  try {
    const view = await readWorkflowArtifact(props.runId, approval.artifactHash);
    if (pending.value?.id === approval.id) diff.value = view;
  } catch (e) {
    if (pending.value?.id === approval.id)
      diffError.value = e instanceof Error ? e.message : String(e);
  } finally {
    diffLoading.value = false;
  }
}

watch(
  () => pending.value?.id ?? null,
  (id) => {
    diff.value = null;
    diffError.value = "";
    if (id) void showDiff();
  },
);

async function decide(decision: "approve" | "reject") {
  const approval = pending.value;
  const current = detail.value;
  if (!approval || !current || deciding.value) return;
  if (decision === "approve" && !diffShown.value) return;
  if (decision === "reject" && !comment.value.trim()) {
    error.value = "Odrzucenie wymaga komentarza.";
    return;
  }
  deciding.value = true;
  error.value = "";
  try {
    await decideWorkflowRun({
      runId: props.runId,
      approvalId: approval.id,
      decision,
      requestId: requestIdFor(approval.id),
      expectedRevision: current.revision,
      artifactHash: approval.artifactHash,
      comment: comment.value.trim() || null,
    });
    notice.value = decision === "approve" ? "Zaakceptowano." : "Odrzucono.";
    comment.value = "";
  } catch (e) {
    const text = e instanceof Error ? e.message : String(e);
    if (text.startsWith(INVALIDATED_PREFIX)) {
      notice.value = text.slice(INVALIDATED_PREFIX.length);
    } else {
      error.value = text;
    }
  } finally {
    deciding.value = false;
    try {
      await refreshDetail();
    } catch (e) {
      error.value = String(e);
    }
  }
}

async function stop() {
  confirmCancel.value = false;
  try {
    await cancelWorkflowRun(props.runId);
    notice.value = "Zatrzymywanie przebiegu…";
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  }
}

/**
 * Wznowienie przebiegu wstrzymanego po restarcie: jedno wywołanie naraz i jedno
 * id żądania (powtórka po błędzie nie wznowi drugi raz). Stan odświeża się po
 * odpowiedzi i po zdarzeniu `runStateChanged`.
 */
async function resume() {
  if (resuming.value) return;
  resuming.value = true;
  error.value = "";
  notice.value = "";
  try {
    const status = await resumeWorkflowRun(props.runId, resumeRequestId);
    if (status === "running") notice.value = "Wznowiono przebieg.";
    else if (status === "failed")
      error.value =
        "Nie wznowiono przebiegu: katalog roboczy jest niedostępny, przebieg zakończył się błędem.";
  } catch (e) {
    error.value = `Nie udało się wznowić przebiegu: ${e instanceof Error ? e.message : String(e)}`;
  } finally {
    resuming.value = false;
    try {
      await refreshDetail();
    } catch (e) {
      error.value = String(e);
    }
  }
}

/** Odzyskanie przerwanego przebiegu: nic nie wznawia, tylko zwalnia katalog po potwierdzeniu. */
const canRecover = computed(() => {
  const view = detail.value?.recovery;
  return !!view && (view.workspaceReserved || view.processesUnconfirmed > 0);
});

/**
 * Po potwierdzeniu w oknie: jedno wywołanie naraz (podwójne kliknięcie nie
 * wysyła drugiego). Niepotwierdzony koniec procesu to alert z PID, żeby
 * użytkownik mógł zakończyć proces ręcznie.
 */
async function recover() {
  confirmRecover.value = false;
  if (recovering.value) return;
  recovering.value = true;
  error.value = "";
  notice.value = "";
  try {
    const outcome = await recoverWorkflowRun(props.runId);
    if (outcome.unconfirmedPids.length > 0) error.value = unconfirmedNote(outcome.unconfirmedPids);
    else if (outcome.workspaceReleased)
      notice.value = "Przebieg odzyskany: katalog roboczy zwolniony. Przebieg nie jest wznawiany.";
    else
      error.value =
        "Nie potwierdzono końca procesu agenta; katalog roboczy pozostaje zablokowany.";
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    recovering.value = false;
    try {
      await refreshDetail();
    } catch (e) {
      error.value = String(e);
    }
  }
}

function output(value: Record<string, unknown> | null): string {
  return value ? JSON.stringify(value, null, 2) : "";
}

onMounted(load);
watch(() => props.runId, load);
onBeforeUnmount(() => {
  generation += 1;
  buffered = null;
  unlisten?.();
  unlisten = null;
});
</script>

<template>
  <section class="dashboard-card wf-monitor" aria-labelledby="wf-monitor-title">
    <p v-if="error" class="error" role="alert">{{ error }}</p>
    <p v-if="notice" class="hint" role="status">{{ notice }}</p>
    <template v-if="detail && status">
      <h2 id="wf-monitor-title">
        {{ detail.summary.workflowName }} (v{{ detail.summary.version }})
      </h2>
      <p>
        <strong data-testid="run-status"
          ><span aria-hidden="true">{{ status.icon }}</span>
          {{ status.label }}</strong
        >
        <span v-if="detail.summary.errorCode" class="hint">
          — {{ FAILURE_MESSAGES[detail.summary.errorCode] }}
        </span>
      </p>
      <p class="hint">
        <template v-if="detail.summary.taskTitle"
          >Zadanie: {{ detail.summary.taskTitle }}.
        </template>
        <template v-if="detail.goal">Cel: {{ detail.goal }}. </template>
        Czas agentów: {{ formatDuration(detail.agentMs) }}; oczekiwanie na
        zgody: {{ formatDuration(detail.waitingMs) }}; tokeny:
        {{ detail.tokens ?? "brak danych" }}.
      </p>
      <p
        v-if="detail.summary.status === 'paused'"
        class="hint"
        data-testid="run-paused"
      >
        {{ PAUSED_HINT }}
      </p>
      <div class="wf-toolbar">
        <button
          v-if="detail.summary.status === 'paused'"
          type="button"
          class="primary-button"
          :disabled="resuming"
          :aria-busy="resuming"
          @click="resume"
        >
          {{ resuming ? "Wznawianie…" : "Wznów" }}
        </button>
        <button
          v-if="!isTerminal(detail.summary.status)"
          type="button"
          class="today-button"
          @click="confirmCancel = true"
        >
          Zatrzymaj
        </button>
      </div>

      <section
        v-if="detail.recovery"
        class="wf-recovery"
        aria-labelledby="wf-recovery-title"
      >
        <h3 id="wf-recovery-title">Odzyskanie przebiegu</h3>
        <p data-testid="run-recovery">{{ recoveryMessage(detail.recovery) }}</p>
        <button
          v-if="canRecover"
          type="button"
          class="today-button"
          :disabled="recovering"
          :aria-busy="recovering"
          @click="confirmRecover = true"
        >
          {{ recovering ? "Sprawdzanie procesów…" : "Odzyskaj przebieg" }}
        </button>
      </section>

      <h3>Graf przebiegu</h3>
      <WorkflowGraphView
        :graph="detail.graph"
        :selected="null"
        :problems="noProblems"
        :statuses="nodeStatuses"
        readonly
      />

      <h3>Kroki</h3>
      <ol class="wf-steps">
        <li v-for="e in detail.executions" :key="e.id">
          <strong>{{ nodeLabel(e.nodeId) }}</strong>
          (próba {{ e.attempt }}, runda {{ e.iteration }}):
          <span aria-hidden="true">{{ EXECUTION_STATUS_LABELS[e.status].icon }}</span>
          {{ EXECUTION_STATUS_LABELS[e.status].label }}
          <span v-if="e.errorCode" class="hint">
            — {{ FAILURE_MESSAGES[e.errorCode] }}
          </span>
          <details v-if="e.output">
            <summary>Wynik</summary>
            <pre>{{ output(e.output) }}</pre>
          </details>
        </li>
      </ol>

      <section
        v-if="pending"
        class="wf-approval"
        aria-labelledby="wf-approval-title"
      >
        <h3 id="wf-approval-title">Zgoda na zmiany</h3>
        <p v-if="detail.goal">Cel: {{ detail.goal }}</p>
        <p class="hint">Skrót zmian: <code>{{ pending.artifactHash }}</code></p>
        <p v-if="diffLoading" class="hint" role="status">Wczytuję zmiany…</p>
        <p v-if="diffError" class="error" role="alert">
          Nie udało się wczytać zmian: {{ diffError }}
          <button type="button" class="text-link" @click="showDiff">
            Spróbuj ponownie
          </button>
        </p>
        <p v-else-if="!diffLoaded && !diffLoading" class="hint">
          Akceptacja będzie możliwa po wyświetleniu zmian.
        </p>
        <template v-if="diffLoaded && diff">
          <p v-if="diff.truncated" class="error" role="status">
            Zmiany są za duże, żeby pokazać je w całości (pokazano
            {{ Math.round(diff.text.length / 1024) }} KB z
            {{ Math.round(diff.size / 1024) }} KB). Tych zmian nie można
            zaakceptować w aplikacji; możesz je odrzucić z komentarzem.
          </p>
          <p v-if="diff.redacted > 0" class="hint" role="status">
            Ukryte fragmenty przypominające sekrety: {{ diff.redacted }}
            (oznaczone [ukryte]). Skrót zgody dotyczy pełnych zmian.
          </p>
          <pre class="wf-diff" tabindex="0" aria-label="Zmiany do zaakceptowania">{{
            diff.text
          }}</pre>
        </template>
        <label>
          Komentarz (wymagany przy odrzuceniu)
          <textarea
            v-model="comment"
            rows="2"
            :maxlength="MAX_APPROVAL_COMMENT_CHARS"
          />
        </label>
        <div class="modal-actions">
          <button
            type="button"
            :disabled="deciding"
            @click="decide('reject')"
          >
            Odrzuć
          </button>
          <button
            type="button"
            class="primary-button"
            :disabled="deciding || !diffShown"
            @click="decide('approve')"
          >
            Akceptuj
          </button>
        </div>
      </section>
      <p v-for="a in detail.approvals.filter((x) => x.comment)" :key="a.id" class="hint">
        Komentarz do zgody: {{ a.comment }}
      </p>
      <p
        v-if="detail.approvals.some((x) => x.artifactPurged)"
        class="hint"
        data-testid="artifact-purged"
      >
        Diff zgody usunięto po okresie przechowywania (Ustawienia → Workflows);
        historia przebiegu zostaje.
      </p>
      <p
        v-if="detail.approvals.some((x) => !x.artifactAvailable && !x.artifactPurged)"
        class="hint"
        data-testid="artifact-unavailable"
      >
        Pliku diffu zgody nie ma na dysku (np. przywrócona kopia go nie
        zawierała); historia przebiegu zostaje.
      </p>

      <h3>Dziennik agentów</h3>
      <button
        v-if="hiddenLines"
        type="button"
        class="text-link"
        @click="showAll = true"
      >
        Pokaż wcześniejsze ({{ hiddenLines }})
      </button>
      <ol class="wf-log" aria-label="Dziennik agentów">
        <li v-for="l in visibleLines" :key="l.sequence">{{ l.text }}</li>
      </ol>

      <h3>Oś zdarzeń</h3>
      <ol class="wf-timeline">
        <li v-for="e in timeline" :key="e.sequence">
          #{{ e.sequence }} {{ e.type }}
        </li>
      </ol>
    </template>
    <ConfirmDialog
      v-if="confirmCancel"
      title="Zatrzymać przebieg?"
      confirm-label="Zatrzymaj"
      @close="confirmCancel = false"
      @confirm="stop"
    >
      <p>Bieżący krok zostanie przerwany, a kolejne nie ruszą.</p>
    </ConfirmDialog>
    <ConfirmDialog
      v-if="confirmRecover"
      title="Odzyskać przebieg?"
      confirm-label="Sprawdź procesy i zwolnij katalog"
      @close="confirmRecover = false"
      @confirm="recover"
    >
      <p>
        Aplikacja sprawdzi procesy agenta tego przebiegu i zakończy te, które
        nadal działają. Katalog roboczy zostanie zwolniony dopiero po
        potwierdzonym końcu procesów. Przebieg nie zostanie wznowiony.
      </p>
    </ConfirmDialog>
  </section>
</template>
