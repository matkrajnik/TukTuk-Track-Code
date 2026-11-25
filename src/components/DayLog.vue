<script setup lang="ts">
/**
 * „Oś dnia” w Dzienniku: przedziały wybranego dnia z lukami, akcje przedziału
 * (Edytuj, Podziel, Usuń) i akcje luki (B3). Widok „Grupuj po zadaniu” zbiera
 * przedziały zadań o tej samej nazwie w jeden wiersz z sumą (pod raport TS).
 */
import { computed, ref } from "vue";
import {
  ArrowDownToLine,
  ArrowUpToLine,
  ChevronDown,
  FileText,
  FoldVertical,
} from "lucide-vue-next";
import ConfirmDialog from "./ConfirmDialog.vue";
import SessionActions from "./SessionActions.vue";
import ManualTime from "../ManualTime.vue";
import {
  deleteSession,
  duration,
  fillGap,
  MIN_GAP_MS,
  TRASH_IN_GAP,
  type GapMode,
  type Session,
  type Snapshot,
} from "../store";
import { clock, dayBounds, inDay } from "../time";
import { groupByTitle, useGroupedPreference } from "../dayGroups";

const props = defineProps<{ data: Snapshot; day: string; now: number }>();
const emit = defineEmits<{
  updated: [data: Snapshot];
  edit: [session: Session];
  split: [session: Session];
  report: [day: string];
}>();

const busy = ref(false);
const error = ref("");
const grouped = useGroupedPreference("tuktuk-track-day-log-grouped");

const taskMap = computed(() => new Map(props.data.tasks.map((t) => [t.id, t])));
const titleOf = (s: Session) => taskMap.value.get(s.taskId)?.title ?? "";

const intervals = computed(() => {
  const [start, end] = dayBounds(props.day);
  return props.data.sessions
    .filter((s) => inDay(s, props.day, props.now) > 0)
    .map((s) => ({
      session: s,
      start: Math.max(start, s.startedAt),
      end: Math.min(end, s.endedAt ?? props.now),
    }))
    .sort((a, b) => a.start - b.start);
});
const total = computed(() =>
  intervals.value.reduce((sum, i) => sum + i.end - i.start, 0),
);

interface TimelineItem {
  key: string;
  start: number;
  end: number;
  session?: Session;
  /** Luka: pomiary przed nią (najpóźniej kończący się) i po niej. */
  before?: Session;
  after?: Session;
  /** W luce leży przedział zadania w koszu: `fill_gap` jej nie zapełni. */
  trashed?: boolean;
}
const timeline = computed(() => {
  const result: TimelineItem[] = [];
  const trash = (props.data.trashSessions ?? []).filter(
    (s) => s.endedAt === null || s.endedAt > s.startedAt,
  );
  let previous = 0;
  let last: Session | undefined;
  for (const interval of intervals.value) {
    // Luki krótsze niż sekunda (np. po edycji 14:58:39 → 14:58:39) pomijamy.
    if (previous && interval.start - previous >= MIN_GAP_MS)
      result.push({
        key: `gap-${interval.session.id}`,
        start: previous,
        end: interval.start,
        before: last,
        after: interval.session,
        trashed: trash.some(
          (s) =>
            s.startedAt < interval.start &&
            (s.endedAt ?? props.now) > previous,
        ),
      });
    result.push({ key: `session-${interval.session.id}`, ...interval });
    if (interval.end >= previous) last = interval.session;
    previous = Math.max(previous, interval.end);
  }
  return result;
});
const groups = computed(() =>
  groupByTitle(intervals.value, (i) => titleOf(i.session), (i) => i.end - i.start),
);

/** Akcje przy „Luce w zapisie” (`fill_gap`). */
async function fill(before: Session, after: Session, mode: GapMode) {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    emit("updated", await fillGap(before.id, after.id, mode));
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

/** Usuwany przedział (okno potwierdzenia); trwającego się nie usuwa. */
const removing = ref<Session | null>(null);
const removeError = ref("");
async function confirmRemove() {
  const session = removing.value;
  if (!session || busy.value) return;
  busy.value = true;
  removeError.value = "";
  try {
    emit("updated", await deleteSession(session.id));
    removing.value = null;
  } catch (e) {
    removeError.value = String(e);
  } finally {
    busy.value = false;
  }
}
function closeRemove() {
  removing.value = null;
  removeError.value = "";
}
const span = (s: Session) =>
  `${clock(s.startedAt, true)} → ${clock(s.endedAt ?? props.now, true)}`;
const until = (s: Session, end: number) =>
  s.endedAt === null ? "teraz" : clock(end, true);
</script>

<template>
  <div class="ts-heading">
    <h2>Oś dnia · {{ duration(total) }}</h2>
    <label class="inline-check day-log-toggle"
      ><input v-model="grouped" type="checkbox" />Grupuj po zadaniu</label
    >
    <button class="today-button" @click="emit('report', day)">
      <FileText :size="14" />Raport dzienny / TS
    </button>
  </div>
  <ManualTime
    :tasks="data.tasks"
    :day="day"
    :sessions="data.sessions"
    @updated="emit('updated', $event)"
  />
  <p class="report-note">
    Luki pokazujemy między zapisanymi pomiarami. Przedziały przez północ są
    przycięte do wybranego dnia.
  </p>
  <p v-if="error" class="error" role="alert">{{ error }}</p>
  <p v-if="!intervals.length" class="report-note">Brak zarejestrowanej pracy.</p>
  <template v-if="grouped">
    <details v-for="group in groups" :key="group.title" class="day-log-group">
      <summary class="timeline-row">
        <span class="mono">{{ group.items.length }} × przedział</span
        ><span>{{ group.title }}</span
        ><strong class="mono">{{ duration(group.total) }}</strong
        ><span class="timeline-actions"
          ><ChevronDown class="group-chevron" :size="16" aria-hidden="true"
        /></span>
      </summary>
      <div
        v-for="item in group.items"
        :key="item.session.id"
        class="timeline-row nested"
      >
        <span class="mono"
          >{{ clock(item.start, true) }} → {{ until(item.session, item.end) }}</span
        ><span /><strong class="mono">{{ duration(item.end - item.start) }}</strong
        ><SessionActions
          :session="item.session"
          :label="span(item.session)"
          :busy="busy"
          @edit="emit('edit', item.session)"
          @split="emit('split', item.session)"
          @remove="removing = item.session"
        />
      </div>
    </details>
  </template>
  <template v-else>
    <div
      v-for="item in timeline"
      :key="item.key"
      class="timeline-row"
      :class="{ gap: !item.session }"
    >
      <span class="mono"
        >{{ clock(item.start, true) }} →
        {{ item.session ? until(item.session, item.end) : clock(item.end, true) }}</span
      ><span>{{ item.session ? titleOf(item.session) : "Luka w zapisie" }}</span
      ><strong class="mono">{{ duration(item.end - item.start) }}</strong
      ><span v-if="item.trashed" class="timeline-actions report-note">{{
        TRASH_IN_GAP
      }}</span
      ><span
        v-else-if="item.before && item.after"
        class="timeline-actions gap-actions"
        role="group"
        :aria-label="`Luka ${clock(item.start, true)} → ${clock(item.end, true)}`"
        ><button
          class="today-button"
          :disabled="busy"
          title="Wydłuża koniec pomiaru wyżej do końca luki"
          @click="fill(item.before, item.after, 'previous')"
        >
          <ArrowUpToLine :size="14" />Dołącz do poprzedniego</button
        ><button
          class="today-button"
          :disabled="busy"
          title="Cofa początek pomiaru niżej do początku luki"
          @click="fill(item.before, item.after, 'next')"
        >
          <ArrowDownToLine :size="14" />Dołącz do następnego</button
        ><button
          class="today-button"
          :disabled="busy"
          title="Oba pomiary spotykają się w środku luki"
          @click="fill(item.before, item.after, 'split')"
        >
          <FoldVertical :size="14" />Podziel po pół
        </button></span
      ><SessionActions
        v-else-if="item.session"
        :session="item.session"
        :label="span(item.session)"
        :busy="busy"
        @edit="emit('edit', item.session)"
        @split="emit('split', item.session)"
        @remove="removing = item.session"
      /><span v-else />
    </div>
  </template>
  <ConfirmDialog
    v-if="removing"
    title="Usunąć przedział?"
    label="Usuń przedział"
    :error="removeError"
    :busy="busy"
    confirm-label="Usuń przedział"
    @close="closeRemove"
    @confirm="confirmRemove"
  >
    <p>
      „{{ titleOf(removing) }}” {{ span(removing) }} ({{
        duration((removing.endedAt ?? now) - removing.startedAt)
      }}) zniknie z osi dnia i z raportów. Tego nie można cofnąć.
    </p>
  </ConfirmDialog>
</template>

<style scoped>
.day-log-toggle {
  margin-left: auto;
  font-size: var(--fs-sm);
}
.day-log-group > summary {
  cursor: pointer;
  list-style: none;
}
.day-log-group > summary::-webkit-details-marker {
  display: none;
}
.day-log-group[open] > summary {
  font-weight: 600;
}
/* Szerokość trzech ikon akcji: suma grupy w jednej kolumnie z czasami przedziałów. */
.day-log-group > summary .timeline-actions {
  min-width: 108px;
  padding-right: 8px;
}
.group-chevron {
  color: var(--muted);
  transition: transform 0.15s ease;
}
.day-log-group[open] .group-chevron {
  transform: rotate(180deg);
}
@media (prefers-reduced-motion: reduce) {
  .group-chevron {
    transition: none;
  }
}
.timeline-row.nested {
  padding-left: 16px;
  color: var(--muted);
}
</style>
