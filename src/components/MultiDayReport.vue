<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { ChevronLeft, ChevronRight, X } from "lucide-vue-next";
import { duration, type Snapshot } from "../store";
import { dayKey, inDay } from "../time";
import {
  addToDays,
  closedDayTotals,
  sortedDays,
  tsText,
  type TsLayout,
} from "../report";
import { isoWeekday, isWorkDay, useSettings } from "../target";
import { copyReport } from "../clipboard";
import ExportButtons from "./ExportButtons.vue";
import TsSendDialog from "./TsSendDialog.vue";
import { useTsSend } from "../composables/useTsSend";
import { openView } from "../navigation";

const props = defineProps<{ data: Snapshot; now: number }>();
const emit = defineEmits<{ open: [day: string]; updated: [data: Snapshot] }>();

/** Pamięć ostatniego zaznaczenia i układu (wygoda, nie dane użytkownika). */
const storageKey = "todo-track-report-days-v1";
const validDay = (value: unknown): value is string =>
  typeof value === "string" && /^\d{4}-\d{2}-\d{2}$/.test(value);

function restore(): { days: string[]; layout: TsLayout } {
  try {
    const raw = JSON.parse(localStorage.getItem(storageKey) ?? "{}");
    return {
      days: Array.isArray(raw?.days)
        ? sortedDays(raw.days.filter(validDay))
        : [],
      layout: raw?.layout === "summary" ? "summary" : "byDay",
    };
  } catch {
    return { days: [], layout: "byDay" };
  }
}

const saved = restore();
const selected = ref<string[]>(saved.days);
const layout = ref<TsLayout>(saved.layout);
const includeNotes = ref(true);
const copyStatus = ref("");
const area = ref<HTMLTextAreaElement | null>(null);
const settings = useSettings();

watch([selected, layout, includeNotes], () => {
  copyStatus.value = "";
  try {
    localStorage.setItem(
      storageKey,
      JSON.stringify({ days: selected.value, layout: layout.value }),
    );
  } catch {}
});

const todayKey = computed(() => dayKey(props.now));
const monthOf = (day: string) => day.slice(0, 7);
/** Wyświetlany miesiąc „RRRR-MM”: ostatni zaznaczony dzień albo bieżący. */
const month = ref(
  monthOf(selected.value[selected.value.length - 1] ?? dayKey(props.now)),
);
const monthParts = computed(() => month.value.split("-").map(Number));
const localDate = (day: string) => {
  const [y, m, d] = day.split("-").map(Number);
  return new Date(y, m - 1, d);
};
const monthLabel = computed(() => {
  const [y, m] = monthParts.value;
  return new Intl.DateTimeFormat("pl-PL", {
    month: "long",
    year: "numeric",
  }).format(new Date(y, m - 1, 1));
});
const fullLabel = (day: string) =>
  new Intl.DateTimeFormat("pl-PL", {
    day: "numeric",
    month: "long",
    year: "numeric",
  }).format(localDate(day));
const shortLabel = (day: string) =>
  new Intl.DateTimeFormat("pl-PL", {
    weekday: "short",
    day: "numeric",
    month: "short",
  }).format(localDate(day));

function shiftMonth(offset: number) {
  const [y, m] = monthParts.value;
  month.value = monthOf(dayKey(new Date(y, m - 1 + offset, 1).getTime()));
}
/** Dni miesiąca jako klucze dat lokalnych (odporne na zmianę czasu). */
function monthDays(y: number, m: number) {
  const count = new Date(y, m, 0).getDate();
  return Array.from({ length: count }, (_, i) =>
    dayKey(new Date(y, m - 1, i + 1).getTime()),
  );
}
/**
 * Sumy dni liczone jednym przejściem po sesjach. Zakończone przedziały
 * zależą tylko od danych; co sekundę przelicza się wyłącznie trwający
 * pomiar (zwykle jeden dzień).
 */
const closedTotals = computed(() => closedDayTotals(props.data));
const activeSession = computed(() => {
  const ids = new Set(props.data.tasks.map((t) => t.id));
  return props.data.sessions.find(
    (s) => s.endedAt === null && ids.has(s.taskId),
  );
});
const activeTotals = computed(() => {
  const totals = new Map<string, number>();
  const active = activeSession.value;
  if (active) addToDays(totals, active.startedAt, props.now);
  return totals;
});
const totalOf = (day: string) =>
  (closedTotals.value.get(day) ?? 0) + (activeTotals.value.get(day) ?? 0);
/**
 * Chwila dla treści raportu: bieżąca tylko wtedy, gdy trwający pomiar
 * dotyczy zaznaczonych dni. W przeciwnym razie stała, żeby tekst raportu
 * nie przeliczał się co sekundę (wynik jest wtedy identyczny).
 */
const reportNow = computed(() => {
  const active = activeSession.value;
  if (!active) return 0;
  const start = dayKey(active.startedAt);
  return selected.value.some((d) => d >= start) ? props.now : 0;
});

const weekdays = ["Pn", "Wt", "Śr", "Cz", "Pt", "So", "Nd"];
const cells = computed(() => {
  const [y, m] = monthParts.value;
  const lead = (new Date(y, m - 1, 1).getDay() + 6) % 7;
  return [
    ...Array.from({ length: lead }, () => null),
    ...monthDays(y, m).map((day) => ({
      day,
      date: Number(day.slice(8)),
      total: totalOf(day),
    })),
  ];
});

const isSelected = (day: string) => selected.value.includes(day);
function toggle(day: string) {
  selected.value = isSelected(day)
    ? selected.value.filter((d) => d !== day)
    : sortedDays([...selected.value, day]);
}
function remove(day: string) {
  selected.value = selected.value.filter((d) => d !== day);
}
/** Skróty zaznaczają dni do dzisiaj włącznie, bez dni przyszłych. */
function choose(days: string[]) {
  selected.value = sortedDays(days.filter((d) => d <= todayKey.value));
  if (selected.value.length)
    month.value = monthOf(selected.value[selected.value.length - 1]);
}
/** Dni tygodnia Pon–Nd przesuniętego o `offset` tygodni względem dzisiaj. */
function weekOf(offset: number) {
  const today = new Date(props.now);
  const first = today.getDate() - ((today.getDay() + 6) % 7) + offset * 7;
  return Array.from({ length: 7 }, (_, i) =>
    dayKey(
      new Date(today.getFullYear(), today.getMonth(), first + i).getTime(),
    ),
  );
}
/**
 * Dzień do skrótu „Dni robocze”: dzień roboczy albo święto w dniu roboczym
 * tygodnia, w które zapisano czas (żeby praca w święto nie znikała z TS).
 */
const workOrWorkedDay = (d: string) =>
  isWorkDay(settings.value, d) ||
  (settings.value.workDays.includes(isoWeekday(d)) && totalOf(d) > 0);
/** Bieżący miesiąc (od dzisiaj, jak skróty tygodnia); kalendarz wraca na niego. */
function currentMonth(workOnly: boolean) {
  const [y, m] = todayKey.value.split("-").map(Number);
  choose(monthDays(y, m).filter((d) => !workOnly || workOrWorkedDay(d)));
  month.value = monthOf(todayKey.value);
}

const summaries = computed(() =>
  selected.value.map((day) => ({ day, total: totalOf(day) })),
);
const total = computed(() =>
  summaries.value.reduce((sum, d) => sum + d.total, 0),
);
const text = computed(() =>
  tsText(selected.value, props.data, reportNow.value, {
    layout: layout.value,
    includeNotes: includeNotes.value,
  }),
);
const running = computed(() => {
  const active = activeSession.value;
  return Boolean(
    active && selected.value.some((d) => inDay(active, d, props.now) > 0),
  );
});
/** Nazwa pliku eksportu: zakres od pierwszego do ostatniego wybranego dnia. */
const exportName = computed(() => {
  const first = selected.value[0] ?? "";
  const last = selected.value[selected.value.length - 1] ?? "";
  return first === last ? `Todo-Track-${first}` : `Todo-Track-${first}_${last}`;
});
async function copy() {
  copyStatus.value = "";
  copyStatus.value = await copyReport(text.value, area.value);
}

/**
 * „Wyślij dni do TS” (B4): jeden podgląd z zakładkami dni i jednym
 * podsumowaniem. Wysyłane są zaznaczone dni do dzisiaj z zapisaną pracą.
 */
const MAX_TS_DAYS = 31;
const {
  account: tsAccount,
  enabled: tsEnabled,
  connected: tsConnected,
  markSent: tsMarkSent,
} = useTsSend(
  () => todayKey.value,
  () => "",
);
const tsDays = computed(() =>
  selected.value.filter((d) => d <= todayKey.value && totalOf(d) > 0),
);
const tsTitle = computed(() =>
  tsDays.value.length > MAX_TS_DAYS
    ? `Jednorazowo najwyżej ${MAX_TS_DAYS} dni`
    : "Podgląd wpisów TS zaznaczonych dni z pracą (do dzisiaj)",
);
const tsOpen = ref(false);
/** Dni ustalone przy otwarciu podglądu. */
const tsOpenDays = ref<string[]>([]);
function openTs() {
  tsOpenDays.value = [...tsDays.value];
  tsOpen.value = true;
}
function connectTs() {
  openView({ view: "settings", section: "timesheet" });
}
</script>

<template>
  <section class="multi-report" aria-label="Wybór dni raportu">
    <div class="month-calendar">
      <div class="month-nav">
        <button
          class="icon-button"
          aria-label="Poprzedni miesiąc"
          @click="shiftMonth(-1)"
        >
          <ChevronLeft :size="18" />
        </button>
        <strong class="month-label" aria-live="polite">{{ monthLabel }}</strong>
        <button
          class="icon-button"
          aria-label="Następny miesiąc"
          @click="shiftMonth(1)"
        >
          <ChevronRight :size="18" />
        </button>
      </div>
      <div class="month-grid" role="group" :aria-label="'Dni: ' + monthLabel">
        <span
          v-for="w in weekdays"
          :key="w"
          class="month-weekday"
          aria-hidden="true"
          >{{ w }}</span
        >
        <template v-for="(cell, i) in cells" :key="cell?.day ?? 'pad' + i">
          <span v-if="!cell" class="month-pad" aria-hidden="true" />
          <button
            v-else
            class="month-day"
            :class="{
              selected: isSelected(cell.day),
              today: cell.day === todayKey,
              worked: cell.total > 0,
            }"
            :aria-label="fullLabel(cell.day)"
            :aria-pressed="isSelected(cell.day)"
            :title="
              cell.total > 0
                ? 'Praca: ' + duration(cell.total)
                : 'Brak zarejestrowanej pracy'
            "
            @click="toggle(cell.day)"
          >
            {{ cell.date
            }}<span v-if="cell.total > 0" class="work-dot" aria-hidden="true" />
          </button>
        </template>
      </div>
      <div class="month-shortcuts" role="group" aria-label="Skróty zaznaczenia">
        <button class="today-button" @click="choose(weekOf(0))">
          Ten tydzień
        </button>
        <button class="today-button" @click="choose(weekOf(-1))">
          Poprzedni tydzień
        </button>
        <button class="today-button" @click="currentMonth(false)">
          Ten miesiąc
        </button>
        <button class="today-button" @click="currentMonth(true)">
          Dni robocze tego miesiąca
        </button>
        <button class="today-button" @click="selected = []">Wyczyść</button>
      </div>
    </div>
    <div class="multi-side">
      <div class="selected-heading">
        <h2>Wybrane dni ({{ selected.length }})</h2>
        <strong class="mono" aria-label="Suma wybranych dni">{{
          duration(total)
        }}</strong>
      </div>
      <ul class="selected-days" aria-label="Wybrane dni">
        <li v-for="item in summaries" :key="item.day">
          <button
            class="selected-open"
            :title="'Otwórz raport dnia ' + item.day"
            @click="emit('open', item.day)"
          >
            <span class="mono">{{ item.day }}</span
            ><span class="muted">{{ shortLabel(item.day) }}</span>
          </button>
          <span class="mono">{{ duration(item.total) }}</span>
          <button
            class="icon-button"
            :aria-label="'Usuń ' + item.day + ' z zestawu'"
            @click="remove(item.day)"
          >
            <X :size="14" />
          </button>
        </li>
        <li v-if="!selected.length" class="empty-state">
          Kliknij dni w kalendarzu albo użyj skrótu.
        </li>
      </ul>
      <ExportButtons
        :days="selected"
        :name="exportName"
        label="Eksport wybranych dni"
      />
    </div>
  </section>
  <section class="ts-report" aria-label="Raport TS z wybranych dni">
    <div class="ts-heading">
      <h2>Raport TS</h2>
      <div class="ts-heading-actions">
        <button class="today-button" :disabled="!selected.length" @click="copy">
          Kopiuj raport
        </button>
        <button
          v-if="tsEnabled && tsConnected"
          class="today-button"
          :disabled="!tsDays.length || tsDays.length > MAX_TS_DAYS"
          :title="tsTitle"
          @click="openTs"
        >
          Wyślij dni do TS
        </button>
      </div>
    </div>
    <p v-if="tsEnabled && tsAccount && !tsConnected" class="report-note">
      <button type="button" class="text-link" @click="connectTs">
        Połącz konto TS w Ustawieniach</button
      >, żeby wysłać dni do TS.
    </p>
    <div class="ts-options">
      <div class="tabs" role="group" aria-label="Układ raportu">
        <button
          :class="{ selected: layout === 'byDay' }"
          :aria-pressed="layout === 'byDay'"
          @click="layout = 'byDay'"
        >
          Po dniach</button
        ><button
          :class="{ selected: layout === 'summary' }"
          :aria-pressed="layout === 'summary'"
          @click="layout = 'summary'"
        >
          Zbiorczo
        </button>
      </div>
      <label class="inline-check"
        ><input v-model="includeNotes" type="checkbox" />Dołącz notatki</label
      >
    </div>
    <p class="report-note">
      Godziny dziesiętne, dwa miejsca po przecinku; każdy wiersz zaokrąglany
      osobno, sumy liczone z pełnego czasu. Zbiorczo łączy notatki z kilku dni
      średnikiem. Dni bez pracy nie trafiają do treści raportu.
    </p>
    <p v-if="running" class="report-note">
      Trwający pomiar jest uwzględniony do bieżącej chwili.
    </p>
    <textarea
      ref="area"
      aria-label="Treść raportu TS z wybranych dni"
      :value="text"
      readonly
      spellcheck="false"
      @focus="($event.target as HTMLTextAreaElement).select()"
    />
    <p class="ts-copy-status" role="status">{{ copyStatus }}</p>
    <TsSendDialog
      v-if="tsOpen"
      :data="data"
      :days="tsOpenDays"
      :now="now"
      @close="tsOpen = false"
      @sent="tsMarkSent"
      @updated="emit('updated', $event)"
    />
  </section>
</template>
