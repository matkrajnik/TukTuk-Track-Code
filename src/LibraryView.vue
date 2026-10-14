<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { change, desktop, duration, type Snapshot } from "./store";
import { dayKey, inDay } from "./time";
const props = defineProps<{ data: Snapshot; now: number }>();
const emit = defineEmits<{
  updated: [data: Snapshot];
  report: [day: string];
}>();
const tab = ref("history"),
  query = ref(""),
  from = ref(""),
  to = ref(""),
  busy = ref(false),
  message = ref(""),
  error = ref("");
const settings = ref({ idleEnabled: true, shortcutError: "", version: "" });
const preview = ref<{
    name: string;
    tasks: number;
    sessions: number;
    trash: number;
  } | null>(null),
  confirmed = ref(false);
const days = computed(() => {
  const result = new Set<string>();
  for (const s of props.data.sessions) {
    const date = new Date(s.startedAt),
      end = Math.min(s.endedAt ?? props.now, props.now);
    date.setHours(0, 0, 0, 0);
    for (let n = 0; +date <= end && n < 36600; n++) {
      result.add(dayKey(+date));
      date.setDate(date.getDate() + 1);
    }
  }
  for (const t of props.data.tasks)
    for (const d of Object.keys(t.details?.notes ?? {})) result.add(d);
  return [...result]
    .sort()
    .reverse()
    .filter(
      (d) => (!from.value || d >= from.value) && (!to.value || d <= to.value),
    )
    .flatMap((day) =>
      props.data.tasks
        .map((task) => ({
          day,
          task,
          total: props.data.sessions
            .filter((s) => s.taskId === task.id)
            .reduce((sum, s) => sum + inDay(s, day, props.now), 0),
          note: task.details?.notes?.[day] ?? "",
        }))
        .filter(
          (r) =>
            (r.total > 0 || r.note) &&
            `${r.task.title} ${r.task.details?.project ?? ""} ${r.note} ${day}`
              .toLocaleLowerCase("pl")
              .includes(query.value.toLocaleLowerCase("pl")),
        ),
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
    settings.value = await invoke("app_settings");
    message.value =
      "Przywrócono kopię. Poprzednia baza została zachowana w automatycznej kopii bezpieczeństwa.";
  });
}
onMounted(async () => {
  if (desktop)
    await run(async () => {
      settings.value = await invoke("app_settings");
    });
});
</script>
<template>
  <div class="page-heading"><h1>Historia i kopie</h1></div>
  <div class="tabs journal-tabs">
    <button
      v-for="item in [
        { id: 'history', name: 'Szukaj w historii' },
        { id: 'archive', name: 'Archiwum' },
        { id: 'trash', name: 'Kosz' },
        { id: 'backup', name: 'Kopie i ustawienia' },
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
    <div class="planning-fields">
      <label
        >Szukaj tytułu, notatki, projektu lub daty<input
          v-model="query"
          type="search" /></label
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
        {{ row.day }} · {{ duration(row.total) }}
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
    <h2>Przerwy i skróty</h2>
    <label class="inline-check"
      ><input
        :checked="settings.idleEnabled"
        type="checkbox"
        :disabled="busy || !desktop"
        @change="
          run(async () => {
            const value = ($event.target as HTMLInputElement).checked;
            await invoke('set_idle_enabled', { enabled: value });
            settings.idleEnabled = value;
          })
        "
      />Zapytaj o czas po co najmniej 10 minutach bezczynności</label
    >
    <p class="hint">
      Decydujesz, czy zachować czas, czy zakończyć pomiar na początku przerwy.
      Nic nie jest obcinane automatycznie.
    </p>
    <p>
      Ctrl+K — szybkie przełączanie w aplikacji.<br />Ctrl+Shift+Space — szybkie
      przełączanie z dowolnego okna.
    </p>
    <p v-if="settings.shortcutError" class="error">
      Nie udało się zarejestrować globalnego skrótu:
      {{ settings.shortcutError }}
    </p>
    <small v-if="settings.version">Todo Track {{ settings.version }}</small>
  </section>
</template>
