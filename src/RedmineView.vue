<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { ChevronDown, ChevronRight, RefreshCw, Search } from "lucide-vue-next";
import {
  desktop,
  read,
  redmineAccount,
  redmineConnected,
  onRedmineChanged,
  redmineGroups,
  redmineIssues,
  redmineIssueTransition,
  redmineOpen,
  redmineStart,
  redmineStatuses,
  type RedmineAccount,
  type RedmineIssue,
  type RedmineStatus,
  type RedmineStatusGroup,
  type Snapshot,
} from "./store";
import { openView } from "./navigation";
import { dayKey, dayText } from "./time";
import RedmineIssueCard from "./components/RedmineIssueCard.vue";

/**
 * Widok Redmine (PLAN_1.9.md, R3): układ „projekt → zgłoszenia”. U góry
 * śledzone projekty (wybierane w Ustawieniach → Redmine), zakres „Moje” /
 * „Wszystkie otwarte”, zgłoszenia pogrupowane według statusu, a na dole moje
 * zgłoszenia spoza śledzonych projektów. Połączenie konta jest w Ustawieniach,
 * nie w widoku. „Rozpocznij” tworzy albo wznawia lokalne zadanie i startuje
 * pomiar. Zmiany w Redmine (R4) tylko po kliknięciu w menu karty: „Przesuń
 * do…” (karta od razu przechodzi do grupy nowego statusu, przy błędzie wraca
 * z komunikatem), „Przypisz do mnie”, komentarz.
 */
const props = defineProps<{
  /** Dane zadań: czas zmierzony lokalnie, plan i trwający pomiar na kartach. */
  data?: Snapshot;
  now?: number;
}>();
const emit = defineEmits<{ updated: [data: Snapshot]; stop: [] }>();

type Scope = "mine" | "all";
const account = ref<RedmineAccount | null>(null);
const busy = ref(false),
  error = ref(""),
  loaded = ref(false),
  updatedAt = ref("");
const issues = ref<RedmineIssue[]>([]),
  groups = ref<RedmineStatusGroup[]>([]),
  warnings = ref<string[]>([]),
  outside = ref<RedmineIssue[]>([]),
  outsideWarnings = ref<string[]>([]),
  outsideError = ref("");
const query = ref(""),
  trackers = ref<string[]>([]),
  collapsed = ref<string[]>([]);

/** Zapamiętany projekt, zakres i „Tylko otwarte” (wygoda widoku). */
const prefsKey = "todo-track-redmine-view-v1";
interface Prefs {
  projectId: number | null;
  scope: Scope;
  openOnly: boolean;
}
function loadPrefs(): Prefs {
  try {
    const raw = JSON.parse(localStorage.getItem(prefsKey) ?? "{}");
    return {
      projectId: typeof raw.projectId === "number" ? raw.projectId : null,
      scope: raw.scope === "all" ? "all" : "mine",
      openOnly: raw.openOnly !== false,
    };
  } catch {
    return { projectId: null, scope: "mine", openOnly: true };
  }
}
const prefs = loadPrefs();
const projectId = ref<number | null>(prefs.projectId),
  scope = ref<Scope>(prefs.scope),
  openOnly = ref(prefs.openOnly);
watch([projectId, scope, openOnly], () => {
  try {
    localStorage.setItem(
      prefsKey,
      JSON.stringify({
        projectId: projectId.value,
        scope: scope.value,
        openOnly: openOnly.value,
      }),
    );
  } catch {
    /* tylko wygoda */
  }
});

const projects = computed(() => account.value?.config.projects ?? []);
const me = computed(() => account.value?.config.userId ?? 0);
const project = computed(
  () => projects.value.find((p) => p.id === projectId.value) ?? null,
);
/** Przy kilku projektach zakładki, przy wielu pole wyboru. */
const manyProjects = computed(() => projects.value.length > 4);
/**
 * „Wszystkie otwarte” w projekcie zawsze bez zamkniętych; „Tylko otwarte”
 * dotyczy „Moich” w projekcie i moich zgłoszeń ze wszystkich projektów (bez
 * śledzonych projektów).
 */
const effectiveOpenOnly = computed(() =>
  scope.value === "all" && !!project.value ? true : openOnly.value,
);

const fold = (text: string) => text.toLocaleLowerCase("pl");
function matches(issue: RedmineIssue) {
  const q = fold(query.value.trim()).replace(/^#/, "");
  if (q && !fold(`${issue.id} ${issue.subject}`).includes(q)) return false;
  return !trackers.value.length || trackers.value.includes(issue.tracker);
}
/** Trackery obecne w wyniku (chipy filtra). */
const trackerChips = computed(() =>
  [...new Set(issues.value.map((i) => i.tracker).filter(Boolean))].sort(
    (a, b) => a.localeCompare(b, "pl"),
  ),
);
const byId = computed(() => new Map(issues.value.map((i) => [i.id, i])));
/** Grupy z filtrem; zamknięte statusy ukryte przy „Tylko otwarte”. */
const visibleGroups = computed(() =>
  groups.value
    .filter((g) => !(g.closed && effectiveOpenOnly.value))
    .map((g) => ({
      ...g,
      issues: g.ids
        .map((id) => byId.value.get(id))
        .filter((i): i is RedmineIssue => !!i && matches(i)),
    }))
    .filter((g) => g.issues.length),
);
const visibleCount = computed(() =>
  visibleGroups.value.reduce((n, g) => n + g.issues.length, 0),
);
const visibleOutside = computed(() => outside.value.filter(matches));
function toggleTracker(name: string) {
  trackers.value = trackers.value.includes(name)
    ? trackers.value.filter((t) => t !== name)
    : [...trackers.value, name];
}
function toggleGroup(status: string) {
  collapsed.value = collapsed.value.includes(status)
    ? collapsed.value.filter((s) => s !== status)
    : [...collapsed.value, status];
}

/** Zadanie z trwającym pomiarem (numer zgłoszenia Redmine). */
const runningId = computed(() => {
  const data = props.data;
  const active = data?.sessions.find((s) => s.endedAt === null);
  return data?.tasks.find((t) => t.id === active?.taskId)?.redmineId ?? null;
});
/** Lokalne zadania zgłoszeń: czas zmierzony i dzień planu. */
const local = computed(() => {
  const out = new Map<number, { ms: number; planned: string }>();
  const data = props.data;
  if (!data) return out;
  const now = props.now ?? Date.now();
  for (const task of data.tasks) {
    if (task.redmineId == null) continue;
    let ms = 0;
    for (const s of data.sessions)
      if (s.taskId === task.id)
        ms += Math.max(0, (s.endedAt ?? now) - s.startedAt);
    const day = task.done ? "" : (task.details?.plannedDay ?? "");
    out.set(task.redmineId, {
      ms,
      planned: day ? (day === dayKey(now) ? "dziś" : dayText(day)) : "",
    });
  }
  return out;
});

const errorText = (e: unknown) => String(e instanceof Error ? e.message : e);
async function run(fn: () => Promise<void>) {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    await fn();
  } catch (e) {
    error.value = errorText(e);
  } finally {
    busy.value = false;
  }
}
const stamp = () =>
  new Date().toLocaleTimeString("pl-PL", {
    hour: "2-digit",
    minute: "2-digit",
  });
async function fetchIssues() {
  loaded.value = false;
  warnings.value = [];
  outsideWarnings.value = [];
  outsideError.value = "";
  updatedAt.value = "";
  if (!account.value?.connected) return;
  const list = projects.value;
  if (list.length && !list.some((p) => p.id === projectId.value))
    projectId.value = list[0].id;
  if (!list.length) {
    // Bez śledzonych projektów: moje zgłoszenia ze wszystkich projektów.
    const result = await redmineIssues(null, true, effectiveOpenOnly.value);
    issues.value = result.issues;
    groups.value = result.groups;
    warnings.value = result.warnings;
    outside.value = [];
  } else {
    // Sekcja „spoza śledzonych projektów” to dodatek: jej błąd (np. 403 na
    // zapytaniu po wszystkich projektach) nie ukrywa listy projektu.
    const [main, mine] = await Promise.allSettled([
      redmineIssues(
        projectId.value,
        scope.value === "mine",
        effectiveOpenOnly.value,
      ),
      redmineIssues(null, true, true),
    ]);
    if (main.status === "rejected") {
      issues.value = [];
      groups.value = [];
      outside.value = [];
      throw main.reason;
    }
    issues.value = main.value.issues;
    groups.value = main.value.groups;
    warnings.value = main.value.warnings;
    if (mine.status === "fulfilled") {
      const tracked = mine.value.tracked;
      outside.value = mine.value.issues.filter(
        (i) => !tracked.includes(i.projectId),
      );
      outsideWarnings.value = mine.value.warnings;
    } else {
      outside.value = [];
      outsideError.value = errorText(mine.reason);
    }
  }
  trackers.value = trackers.value.filter((t) => trackerChips.value.includes(t));
  loaded.value = true;
  updatedAt.value = stamp();
}
const refresh = () => run(fetchIssues);
/**
 * Ręczne „Odśwież”: najpierw statusy prosto z Redmine (nowy albo przestawiony
 * status trafia od razu do właściwej grupy), potem lista. Wejście do widoku i
 * zmiana projektu korzystają z pamięci statusów (raz na dobę).
 */
const refreshAll = () =>
  run(async () => {
    let statusWarning = "";
    if (account.value?.connected) {
      try {
        statusOrder = await redmineStatuses(true);
      } catch (e) {
        statusWarning = `Statusy: ${errorText(e)}`;
      }
    }
    await fetchIssues();
    if (statusWarning && !warnings.value.includes(statusWarning))
      warnings.value = [...warnings.value, statusWarning];
  });
function selectProject(id: number) {
  if (projectId.value === id) return;
  projectId.value = id;
  trackers.value = [];
  void refresh();
}
function setScope(next: Scope) {
  if (scope.value === next) return;
  scope.value = next;
  void refresh();
}
async function start(id: number) {
  await run(async () => emit("updated", await redmineStart(id)));
}
async function open(url: string) {
  await run(() => redmineOpen(url));
}
/** Statusy w kolejności z Redmine do przegrupowania po zmianie statusu. */
let statusOrder: RedmineStatus[] = [];
/** Podmienia zgłoszenie na liście projektu i w sekcji spoza śledzonych. */
function replaceIssue(next: RedmineIssue) {
  const swap = (list: RedmineIssue[]) =>
    list.map((i) => (i.id === next.id ? next : i));
  issues.value = swap(issues.value);
  outside.value = swap(outside.value);
}
function regroup() {
  if (statusOrder.length)
    groups.value = redmineGroups(issues.value, statusOrder);
}
/**
 * „Przesuń do…”: karta od razu przechodzi do grupy wybranego statusu; po
 * odczycie z Redmine dostaje faktyczny status, a przy błędzie wraca na swoje
 * miejsce z komunikatem.
 */
async function move(issue: RedmineIssue, status: RedmineStatus) {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  const before = { ...issue };
  try {
    if (!statusOrder.length) statusOrder = await redmineStatuses(false);
  } catch {
    /* bez kolejności statusów grupa zmieni się po odświeżeniu */
  }
  replaceIssue({
    ...issue,
    status: status.name,
    statusId: status.id,
    closed: status.isClosed,
  });
  regroup();
  try {
    const after = await redmineIssueTransition(issue.id, status.id);
    if (after) replaceIssue(after);
    regroup();
  } catch (e) {
    replaceIssue(before);
    regroup();
    error.value = `Nie udało się przesunąć #${issue.id}: ${errorText(e)}`;
  } finally {
    busy.value = false;
  }
  // Status na kafelkach powiązanych zadań (w aplikacji także „data-changed”).
  try {
    emit("updated", await read());
  } catch {
    /* kafelki odświeżą się przy następnym odczycie */
  }
}
/** „Przypisz do mnie” z menu karty: karta pokazuje nową osobę. */
function assigned(issue: RedmineIssue | null) {
  if (issue) replaceIssue(issue);
}
function openSettings() {
  // Sekcja „Redmine” w Ustawieniach (R5); bez niej otwiera się ostatnia.
  openView({ view: "settings", section: "redmine" });
}

/** Konto i lista od nowa (wejście do widoku, zmiana konta albo projektów). */
const load = () =>
  run(async () => {
    account.value = await redmineAccount();
    // Szybkie dodawanie rozpoznaje `#1234` tylko przy połączonym Redmine.
    redmineConnected.value = account.value.connected;
    if (account.value.connected) await fetchIssues();
    else {
      issues.value = [];
      groups.value = [];
      outside.value = [];
    }
  });
/**
 * Połączenie, rozłączenie albo nowe śledzone projekty (Ustawienia, inne okno,
 * w podglądzie także zmiana symulowanego Redmine): widok wczytuje się od
 * nowa; gdy właśnie trwa inna operacja, zaraz po niej.
 */
let pending = false;
function changed() {
  if (!alive) return;
  if (busy.value) pending = true;
  else {
    pending = false;
    void load();
  }
}
let alive = true;
let stopChanges: (() => void) | undefined;
watch(busy, (now) => {
  if (!now && pending) changed();
});
onMounted(() => {
  stopChanges = onRedmineChanged(changed);
  void load();
});
onUnmounted(() => {
  alive = false;
  stopChanges?.();
});
</script>
<template>
  <section class="jira-view redmine-view">
    <div class="page-heading">
      <h1>Redmine</h1>
      <div v-if="account?.connected" class="jira-actions">
        <span class="hint account-name"
          >Połączono jako {{ account.config.displayName }}</span
        ><button class="text-link" @click="openSettings">
          Ustawienia → Redmine
        </button>
      </div>
    </div>
    <p v-if="!desktop" class="hint jira-preview-note">
      Podgląd: symulowany Redmine z przykładowymi zgłoszeniami.
    </p>
    <p v-if="error" class="error" role="alert">{{ error }}</p>
    <div v-if="account && !account.connected" class="redmine-connect">
      <p>
        Widok Redmine pokazuje Twoje zgłoszenia ze śledzonych projektów i
        pozwala rozpocząć nad nimi pracę.
      </p>
      <button class="text-link" @click="openSettings">
        Połącz konto Redmine w Ustawieniach
      </button>
    </div>
    <template v-else-if="account?.connected">
      <div v-if="projects.length" class="redmine-projects">
        <div
          v-if="!manyProjects"
          class="jira-mode redmine-project-tabs"
          role="group"
          aria-label="Projekt Redmine"
        >
          <button
            v-for="p in projects"
            :key="p.id"
            type="button"
            :aria-pressed="p.id === projectId"
            :disabled="busy"
            @click="selectProject(p.id)"
          >
            {{ p.name }}
          </button>
        </div>
        <label v-else class="check-label"
          >Projekt<select
            :value="projectId ?? projects[0]?.id"
            aria-label="Projekt Redmine"
            :disabled="busy"
            @change="
              selectProject(Number(($event.target as HTMLSelectElement).value))
            "
          >
            <option v-for="p in projects" :key="p.id" :value="p.id">
              {{ p.name }}
            </option>
          </select></label
        >
        <div class="jira-mode" role="group" aria-label="Zakres zgłoszeń">
          <button
            type="button"
            :aria-pressed="scope === 'mine'"
            :disabled="busy"
            @click="setScope('mine')"
          >
            Moje</button
          ><button
            type="button"
            :aria-pressed="scope === 'all'"
            :disabled="busy"
            @click="setScope('all')"
          >
            Wszystkie otwarte
          </button>
        </div>
      </div>
      <div v-else class="redmine-all-projects">
        <h2>Moje zgłoszenia ze wszystkich projektów</h2>
        <button class="text-link" @click="openSettings">
          Wybierz projekty w Ustawieniach
        </button>
      </div>
      <div class="jira-search-toolbar">
        <label class="search"
          ><Search :size="15" /><input
            v-model="query"
            aria-label="Szukaj zgłoszeń Redmine"
            placeholder="Numer lub temat zgłoszenia…" /></label
        ><label v-if="!project || scope === 'mine'" class="check-label"
          ><input
            v-model="openOnly"
            type="checkbox"
            :disabled="busy"
            @change="refresh"
          />Tylko otwarte</label
        ><button class="secondary-button" :disabled="busy" @click="refreshAll">
          <RefreshCw :size="15" />Odśwież
        </button>
      </div>
      <div
        v-if="trackerChips.length > 1"
        class="redmine-trackers"
        role="group"
        aria-label="Tracker"
      >
        <button
          v-for="t in trackerChips"
          :key="t"
          type="button"
          class="redmine-chip"
          :aria-pressed="trackers.includes(t)"
          @click="toggleTracker(t)"
        >
          {{ t }}
        </button>
      </div>
      <div v-if="warnings.length" class="error" role="alert">
        <div>
          <strong>Nie wszystkie zgłoszenia zostały pobrane.</strong>
          <p v-for="warning in warnings" :key="warning">{{ warning }}</p>
        </div>
      </div>
      <p class="jira-result-info">
        {{
          !project
            ? "Przypisane do mnie"
            : scope === "mine"
              ? `${project.name} · moje`
              : `${project.name} · wszystkie otwarte`
        }}
        · {{ visibleCount }} zgłoszeń<span v-if="updatedAt">
          · Odświeżono {{ updatedAt }}</span
        >
      </p>
      <p v-if="busy" class="hint" role="status">Proszę czekać…</p>
      <div class="redmine-groups" :aria-busy="busy">
        <section
          v-for="g in visibleGroups"
          :key="g.status"
          class="redmine-group"
          :aria-label="'Status ' + g.status"
        >
          <button
            type="button"
            class="redmine-group-head"
            :aria-expanded="!collapsed.includes(g.status)"
            @click="toggleGroup(g.status)"
          >
            <ChevronRight v-if="collapsed.includes(g.status)" :size="15" /><ChevronDown
              v-else
              :size="15"
            /><span class="redmine-group-name">{{ g.status }}</span
            ><span
              class="jira-column-count"
              :aria-label="'Zgłoszeń: ' + g.issues.length"
              >{{ g.issues.length }}</span
            >
          </button>
          <div v-if="!collapsed.includes(g.status)" class="redmine-cards">
            <RedmineIssueCard
              v-for="issue in g.issues"
              :key="issue.id"
              :issue="issue"
              :me="me"
              :running="runningId === issue.id"
              :busy="busy"
              :show-project="!!project && issue.projectId !== project.id"
              :local="local.get(issue.id)"
              @start="start"
              @stop="emit('stop')"
              @open="open"
              @move="move"
              @assigned="assigned"
            />
          </div>
        </section>
      </div>
      <p
        v-if="loaded && !visibleCount && !busy && !warnings.length"
        class="empty-state"
      >
        {{
          query.trim() || trackers.length
            ? "Brak zgłoszeń pasujących do wyszukiwania."
            : !project
              ? "Brak przypisanych Ci zgłoszeń."
              : scope === "mine"
                ? "Brak przypisanych Ci zgłoszeń w tym projekcie."
                : "Brak otwartych zgłoszeń w tym projekcie."
        }}
      </p>
      <section
        v-if="project && loaded"
        class="redmine-outside"
        aria-label="Przypisane do mnie spoza śledzonych projektów"
      >
        <h2>
          Przypisane do mnie spoza śledzonych projektów
          <span class="jira-column-count">{{ visibleOutside.length }}</span>
        </h2>
        <p v-for="warning in outsideWarnings" :key="warning" class="hint">
          {{ warning }}
        </p>
        <p v-if="outsideError" class="error" role="alert">
          Nie udało się pobrać zgłoszeń spoza śledzonych projektów:
          {{ outsideError }}
        </p>
        <div v-if="visibleOutside.length" class="redmine-cards">
          <RedmineIssueCard
            v-for="issue in visibleOutside"
            :key="issue.id"
            :issue="issue"
            :me="me"
            :running="runningId === issue.id"
            :busy="busy"
            :show-project="true"
            :local="local.get(issue.id)"
            @start="start"
            @stop="emit('stop')"
            @open="open"
            @move="move"
            @assigned="assigned"
          />
        </div>
        <p v-else-if="!outsideError" class="hint">
          Brak otwartych zgłoszeń przypisanych do Ciebie w innych projektach.
        </p>
      </section>
    </template>
  </section>
</template>
