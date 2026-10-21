<script setup lang="ts">
import { computed, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { change, desktop, duration, type Snapshot } from "./store";
import { dayText } from "./time";
import { dayTaskTotals } from "./report";
const props = defineProps<{ data: Snapshot; now: number }>();
const emit = defineEmits<{
  updated: [data: Snapshot];
  report: [day: string];
}>();
/** Zakładka; App pamięta ją między wejściami do widoku. */
const tab = defineModel<string>("tab", { default: "history" });
const query = ref(""),
  from = ref(""),
  to = ref(""),
  busy = ref(false),
  message = ref(""),
  error = ref("");
const preview = ref<{
    name: string;
    tasks: number;
    sessions: number;
    trash: number;
  } | null>(null),
  confirmed = ref(false);
/**
 * Wiersze historii (dzień, zadanie) z czasem albo notatką, od najnowszego dnia,
 * w kolejności zadań ze snapshotu. Jedno przejście po przedziałach zgrupowane
 * po dniu i zadaniu (B6), liczone tylko przy zmianie danych: trwający pomiar
 * do chwili odczytu danych, bez odliczania co sekundę.
 */
const history = computed(() => {
  const totals = dayTaskTotals(props.data.sessions, Date.now());
  const days = new Set(totals.keys());
  for (const t of props.data.tasks)
    for (const d of Object.keys(t.details?.notes ?? {})) days.add(d);
  return [...days]
    .sort()
    .reverse()
    .flatMap((day) =>
      props.data.tasks
        .map((task) => {
          const note = task.details?.notes?.[day] ?? "";
          return {
            day,
            task,
            total: totals.get(day)?.get(task.id) ?? 0,
            note,
            text: `${task.title} ${task.details?.project ?? ""} ${note} ${day}`.toLocaleLowerCase(
              "pl",
            ),
          };
        })
        .filter((r) => r.total > 0 || r.note),
    );
});
const days = computed(() => {
  const needle = query.value.toLocaleLowerCase("pl");
  return history.value.filter(
    (r) =>
      (!from.value || r.day >= from.value) &&
      (!to.value || r.day <= to.value) &&
      r.text.includes(needle),
  );
});
async function run(action: () => Promise<void>) {
  busy.value = true;
  error.value = "";
  message.value = "";
  try {
    await action();
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}
async function mutate(action: string, id: number) {
  await run(async () => {
    emit("updated", await change(action, id));
  });
}
async function inspect() {
  await run(async () => {
    preview.value = null;
    confirmed.value = false;
    preview.value = await invoke("inspect_backup");
  });
}
async function restore() {
  if (!confirmed.value || !preview.value) return;
  await run(async () => {
    emit("updated", await invoke("restore_backup"));
    preview.value = null;
    confirmed.value = false;
    message.value =
      "Przywrócono kopię. Poprzednia baza została zachowana w automatycznej kopii bezpieczeństwa.";
  });
}
</script>
<template>
  <div class="page-heading"><h1>Historia i kopie</h1></div>
  <div class="tabs journal-tabs">
    <button
      v-for="item in [
        { id: 'history', name: 'Szukaj w historii' },
        { id: 'archive', name: 'Archiwum' },
        { id: 'trash', name: 'Kosz' },
        { id: 'backup', name: 'Kopie' },
      ]"
      :key="item.id"
      :class="{ selected: tab === item.id }"
      @click="tab = item.id"
    >
      {{ item.name }}
    </button>
  </div>
  <p v-if="error" class="error" role="alert">{{ error }}</p>
  <p v-if="message" role="status">{{ message }}</p>
  <section v-if="tab === 'history'" class="dashboard-card">
    <div class="filter-fields">
      <label class="filter-query"
        >Szukaj tytułu, notatki, projektu lub daty<input
          v-model="query"
          type="search"
          placeholder="np. WMS albo 2026-10" /></label
      ><label>Od dnia<input v-model="from" type="date" /></label
      ><label>Do dnia<input v-model="to" type="date" /></label>
    </div>
    <p class="hint">Historia obejmuje również zadania z archiwum.</p>
    <div
      v-for="row in days"
      :key="row.day + '-' + row.task.id"
      class="library-row"
    >
      <div>
        <strong>{{ row.task.title }}</strong>
        <p>{{ row.note }}</p>
        <small>{{ row.task.details?.project }}</small>
      </div>
      <button class="today-button" @click="emit('report', row.day)">
        {{ dayText(row.day) }} · {{ duration(row.total) }}
      </button>
    </div>
    <p v-if="!days.length" class="report-note">Brak pasującej historii.</p>
  </section>
  <section
    v-else-if="tab === 'archive' || tab === 'trash'"
    class="dashboard-card"
  >
    <p class="hint">
      {{
        tab === "archive"
          ? "Archiwizacja ukrywa zadania z listy i planowania, zachowując czas w raportach."
          : "Kosz przechowuje zadania razem z czasem. Przywrócenie ponownie uwzględni je w raportach."
      }}
    </p>
    <div
      v-for="task in tab === 'archive'
        ? data.tasks.filter((t) => t.archived)
        : (data.trash ?? [])"
      :key="task.id"
      class="library-row"
    >
      <strong>{{ task.title }}</strong
      ><button
        class="today-button"
        :disabled="busy"
        @click="mutate(tab === 'archive' ? 'unarchive' : 'restore', task.id)"
      >
        Przywróć: {{ task.title }}
      </button>
    </div>
    <p
      v-if="
        !(
          tab === 'archive'
            ? data.tasks.filter((t) => t.archived)
            : (data.trash ?? [])
        ).length
      "
      class="report-note"
    >
      Tutaj jest pusto.
    </p>
  </section>
  <section v-else class="dashboard-card">
    <h2>Kopie zapasowe</h2>
    <p class="hint">
      Kopia zawiera zadania, notatki, plan i całą historię czasu. Token Jiry
      pozostaje w Menedżerze poświadczeń Windows.
    </p>
    <p v-if="!desktop">
      Eksport i przywracanie są dostępne w aplikacji Windows.
    </p>
    <div class="planning-actions">
      <button
        class="today-button"
        :disabled="busy || !desktop"
        @click="
          run(async () => {
            const path = await invoke<string | null>('export_backup');
            if (path) message = 'Zapisano kopię: ' + path;
          })
        "
      >
        Eksportuj kopię</button
      ><button
        class="today-button"
        :disabled="busy || !desktop"
        @click="inspect"
      >
        Wybierz kopię do przywrócenia
      </button>
    </div>
    <div v-if="preview" class="restore-review">
      <h3>{{ preview.name }}</h3>
      <p>
        Zadania: {{ preview.tasks }}, przedziały czasu: {{ preview.sessions }},
        zadania w koszu: {{ preview.trash }}.
      </p>
      <p>
        Przywrócenie zastąpi aktualne dane. Najpierw powstanie kopia obecnej
        bazy. Bieżące połączenie z Jirą pozostanie zachowane.
      </p>
      <label class="inline-check"
        ><input v-model="confirmed" type="checkbox" />Chcę zastąpić dane wybraną
        kopią</label
      ><button
        class="today-button"
        :disabled="busy || !confirmed"
        @click="restore"
      >
        Przywróć wybraną kopię</button
      ><button class="today-button" @click="preview = null">Anuluj</button>
    </div>
  </section>
</template>
