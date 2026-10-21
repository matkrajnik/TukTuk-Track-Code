<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import ConfirmDialog from "./ConfirmDialog.vue";
import {
  onTsChanged,
  read,
  tsConnect,
  tsDefaultApi,
  tsDisconnect,
  tsSettings,
  tsTasks,
  tsTodayTasks,
  type TsProject,
  type TsSettings,
} from "../store";
import { dayKey, plural } from "../time";
import { findTsTask } from "../timesheet";
import {
  loadTsProjectMap,
  setTsProjectMap,
  useTsProjectMap,
} from "../composables/useTsProjectMap";

/**
 * Ustawienia → Timesheet (TS) (PLAN_1.8.md, T4): konto Ideo TS (login, hasło
 * w magazynie poświadczeń, adres API, identyfikator urządzenia), test
 * połączenia i domyślne zadania TS projektów (`ts_project_map`). Przełącznik
 * „Wysyłaj raport TS” jest w slocie `send` (zapis przez ustawienia panelu).
 * Hasło nigdy nie wraca z backendu: pole jest puste także po połączeniu.
 */
const account = ref<TsSettings>({
  config: { apiUrl: tsDefaultApi, login: "", deviceId: "", deviceName: "Todo Track" },
  connected: false,
  sendEnabled: true,
});
const login = ref("");
const password = ref("");
const apiUrl = ref(tsDefaultApi);
const busy = ref(false);
const error = ref("");
const notice = ref("");
const confirming = ref(false);

const map = useTsProjectMap();
const projects = ref<string[]>([]);
const tsProjects = ref<TsProject[]>([]);
const tsListError = ref("");
const newProject = ref("");
const newTask = ref("");
const mapBusy = ref(false);
const mapError = ref("");

const message = (e: unknown) => String(e instanceof Error ? e.message : e);

async function loadTsList(refresh = false) {
  tsListError.value = "";
  if (!account.value.connected) {
    tsProjects.value = [];
    return;
  }
  try {
    tsProjects.value = await tsTodayTasks(refresh);
  } catch (e) {
    tsProjects.value = [];
    tsListError.value = message(e);
  }
}
async function load(fields = false) {
  account.value = await tsSettings();
  if (fields) {
    login.value = account.value.config.login;
    apiUrl.value = account.value.config.apiUrl || tsDefaultApi;
  }
}

async function connect() {
  busy.value = true;
  error.value = "";
  notice.value = "";
  try {
    account.value = await tsConnect(apiUrl.value, login.value, password.value);
    login.value = account.value.config.login;
    apiUrl.value = account.value.config.apiUrl;
    notice.value = `Połączono jako ${account.value.config.login}.`;
    await loadTsList(true);
  } catch (e) {
    error.value = message(e);
    await load().catch(() => {});
  } finally {
    // Hasło nie zostaje w formularzu ani po sukcesie, ani po błędzie.
    password.value = "";
    busy.value = false;
  }
}
/** „Testuj połączenie”: lista zadań z formularza dnia dzisiejszego. */
async function testConnection() {
  busy.value = true;
  error.value = "";
  notice.value = "";
  try {
    const list = await tsTasks(dayKey(Date.now()));
    const tasks = list.reduce((sum, p) => sum + p.tasks.length, 0);
    notice.value = `Połączono: ${tasks} ${plural(tasks, "zadanie", "zadania", "zadań")} w ${list.length} ${plural(list.length, "projekcie", "projektach", "projektach")}.`;
    tsProjects.value = list;
    tsListError.value = "";
  } catch (e) {
    error.value = message(e);
  } finally {
    busy.value = false;
  }
}
async function disconnect() {
  busy.value = true;
  error.value = "";
  notice.value = "";
  try {
    account.value = await tsDisconnect();
    confirming.value = false;
    tsProjects.value = [];
  } catch (e) {
    error.value = message(e);
  } finally {
    busy.value = false;
  }
}

function openConfirm() {
  error.value = "";
  notice.value = "";
  confirming.value = true;
}

const tsTaskName = (id: string) => findTsTask(tsProjects.value, id)?.name ?? "";
async function addMapping() {
  const project = newProject.value.trim();
  const id = newTask.value;
  if (!project || !id) return;
  mapBusy.value = true;
  mapError.value = "";
  try {
    await setTsProjectMap(project, id, tsTaskName(id));
    newProject.value = "";
    newTask.value = "";
  } catch (e) {
    mapError.value = message(e);
  } finally {
    mapBusy.value = false;
  }
}
async function removeMapping(project: string) {
  mapBusy.value = true;
  mapError.value = "";
  try {
    await setTsProjectMap(project, "", "");
  } catch (e) {
    mapError.value = message(e);
  } finally {
    mapBusy.value = false;
  }
}
const mappedLabel = (id: string, name: string) => {
  const listed = findTsTask(tsProjects.value, id);
  return listed ? `${listed.project} → ${listed.name}` : name || id;
};
const canAdd = computed(
  () => !mapBusy.value && !!newProject.value && !!newTask.value,
);

let alive = true;
onMounted(async () => {
  try {
    await load(true);
    void loadTsProjectMap(true);
    const data = await read();
    if (!alive) return;
    projects.value = [
      ...new Set(
        data.tasks
          .map((t) => t.details?.project?.trim())
          .filter((p): p is string => Boolean(p)),
      ),
    ].sort((a, b) => a.localeCompare(b, "pl"));
    await loadTsList();
  } catch (e) {
    if (alive) error.value = message(e);
  }
});
// Zmiana konta w innym oknie (w podglądzie zdarzenie `storage` symulowanego TS).
// Zmianę wywołaną tutaj (Połącz, Rozłącz) obsługuje sama akcja, bez
// drugiego pobrania listy zadań z TS.
const stopTs = onTsChanged(() => {
  if (busy.value) return;
  void load()
    .then(() => (alive ? loadTsList() : undefined))
    .catch(() => {});
});
onUnmounted(() => {
  alive = false;
  stopTs();
});
</script>

<template>
  <p class="hint">
    Raport TS dnia wysyłasz do Ideo Timesheet przyciskiem „Wyślij do TS” w
    Zamknięciu dnia i w Raporcie dnia. Każda wysyłka otwiera podgląd i czeka na
    „Zapisz w TS”.
  </p>
  <h3>Połączenie</h3>
  <p class="ts-account">
    {{
      account.connected
        ? `Połączono jako ${account.config.login}`
        : "Nie połączono"
    }}
  </p>
  <div class="planning-fields settings-fields">
    <label
      >Login<input
        v-model="login"
        type="text"
        spellcheck="false"
        autocomplete="username"
        :disabled="busy"
    /></label>
    <label
      >Hasło<input
        v-model="password"
        type="password"
        autocomplete="current-password"
        :placeholder="account.connected ? 'zapisane' : ''"
        :disabled="busy"
        @keydown.enter="connect"
    /></label>
  </div>
  <p v-if="account.connected" class="hint">
    Hasło jest zapisane w magazynie poświadczeń Windows. Wpisz je tylko, gdy
    chcesz połączyć się ponownie (np. po zmianie hasła).
  </p>
  <details class="ts-advanced">
    <summary>Zaawansowane</summary>
    <div class="planning-fields settings-fields">
      <label
        >Adres API<input
          v-model="apiUrl"
          type="url"
          spellcheck="false"
          autocomplete="off"
          :placeholder="tsDefaultApi"
          :disabled="busy"
      /></label>
      <label
        >Identyfikator urządzenia<input
          :value="account.config.deviceId"
          type="text"
          readonly
          placeholder="nadawany przy pierwszym połączeniu"
      /></label>
    </div>
  </details>
  <p v-if="error" class="error" role="alert">{{ error }}</p>
  <p v-if="notice" class="hint ts-notice" role="status">{{ notice }}</p>
  <div class="planning-actions">
    <button
      class="action-button"
      :disabled="busy || !login.trim() || !password"
      @click="connect"
    >
      Połącz
    </button>
    <template v-if="account.connected">
      <button class="today-button" :disabled="busy" @click="testConnection">
        Testuj połączenie
      </button>
      <button class="today-button" :disabled="busy" @click="openConfirm">
        Rozłącz
      </button>
    </template>
  </div>
  <slot name="send" />
  <h3 id="ts-project-map-heading">Domyślne zadania TS dla projektów</h3>
  <p class="hint">
    Zadanie projektu bez wybranego zadania TS trafia do domyślnego zadania TS
    swojego projektu. Wybór w kafelku zadania ma pierwszeństwo.
  </p>
  <table
    v-if="map.length"
    class="ts-project-map"
    aria-labelledby="ts-project-map-heading"
  >
    <thead>
      <tr>
        <th scope="col">Projekt</th>
        <th scope="col">Zadanie TS</th>
        <th scope="col"><span class="sr-only">Akcje</span></th>
      </tr>
    </thead>
    <tbody>
      <tr v-for="m in map" :key="m.project">
        <td>{{ m.project }}</td>
        <td>{{ mappedLabel(m.tsTaskId, m.tsTaskName) }}</td>
        <td>
          <button
            class="today-button"
            :aria-label="`Usuń mapowanie projektu ${m.project}`"
            :disabled="mapBusy"
            @click="removeMapping(m.project)"
          >
            Usuń
          </button>
        </td>
      </tr>
    </tbody>
  </table>
  <p v-else class="hint">Brak domyślnych zadań TS.</p>
  <div class="planning-fields settings-fields ts-map-add">
    <label
      >Projekt<select
        v-model="newProject"
        aria-label="Projekt"
        :disabled="mapBusy || !projects.length"
      >
        <option value="">— wybierz —</option>
        <option v-for="p in projects" :key="p" :value="p">{{ p }}</option>
      </select></label
    >
    <label
      >Domyślne zadanie TS<select
        aria-label="Domyślne zadanie TS"
        v-model="newTask"
        :disabled="mapBusy || !tsProjects.length"
      >
        <option value="">— wybierz —</option>
        <optgroup v-for="p in tsProjects" :key="p.name" :label="p.name">
          <option v-for="t in p.tasks" :key="t.id" :value="t.id">
            {{ t.name }}
          </option>
        </optgroup>
      </select></label
    >
  </div>
  <div class="planning-actions">
    <button class="action-button" :disabled="!canAdd" @click="addMapping">
      Dodaj mapowanie
    </button>
    <button
      v-if="account.connected"
      class="today-button"
      :disabled="mapBusy"
      @click="loadTsList(true)"
    >
      Odśwież listę zadań TS
    </button>
  </div>
  <p v-if="!account.connected" class="hint">
    Lista zadań TS pojawi się po połączeniu konta.
  </p>
  <p v-else-if="tsListError" class="error">{{ tsListError }}</p>
  <p v-if="!projects.length" class="hint">
    Żadne zadanie nie ma jeszcze projektu.
  </p>
  <p v-if="mapError" class="error" role="alert">{{ mapError }}</p>
  <h3>Jak to działa</h3>
  <p class="hint">
    Godziny pochodzą z pomiarów dnia, jak w Raporcie TS: dziesiętne, z dwoma
    miejscami po przecinku. Opis wpisu to notatka dnia zadania, a bez notatki —
    jego tytuł. Kilka zadań na to samo zadanie TS daje jeden wpis z sumą czasu
    i opisami po średniku. Zapis w TS zastępuje cały dzień: „Zachowaj pozostałe
    wpisy TS” dopisuje wpisy TS o innych zadaniach bez zmian, a „Zastąp
    wszystko” usuwa wszystkie dotychczasowe wpisy dnia.
  </p>
  <ConfirmDialog
    v-if="confirming"
    title="Rozłączyć konto TS?"
    confirm-label="Rozłącz"
    :busy="busy"
    :error="error"
    @close="confirming = false"
    @confirm="disconnect"
  >
    <p>
      Hasło zostanie usunięte z magazynu poświadczeń Windows. Login,
      identyfikator urządzenia i domyślne zadania projektów zostają.
    </p>
  </ConfirmDialog>
</template>
