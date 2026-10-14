<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import {
  ExternalLink,
  Play,
  RefreshCw,
  Search,
  Settings2,
  Square,
} from "lucide-vue-next";
import { desktop, type Snapshot } from "./store";
interface Board {
  id: number;
  name: string;
}
interface Config {
  site: string;
  email: string;
  displayName: string;
  boards: Board[];
}
interface Settings {
  config: Config;
  connected: boolean;
}
interface Issue {
  key: string;
  summary: string;
  status: string;
  boards: string[];
  url: string;
}
const props = defineProps<{ activeUrl?: string | null }>();
const emit = defineEmits<{ updated: [data: Snapshot]; stop: [] }>();
const settings = ref<Settings>({
  config: {
    site: "https://asistonwms.atlassian.net",
    email: "",
    displayName: "",
    boards: [],
  },
  connected: false,
});
const site = ref(settings.value.config.site),
  email = ref(""),
  apiToken = ref("");
const busy = ref(false),
  error = ref(""),
  warnings = ref<string[]>([]);
const editingConnection = ref(false),
  pickingBoards = ref(false);
const boardName = ref(""),
  boards = ref<Board[]>([]),
  chosen = ref<Board[]>([]),
  nextStart = ref<number | null>(null);
const issues = ref<Issue[]>([]),
  query = ref(""),
  openOnly = ref(true),
  loaded = ref(false),
  updatedAt = ref("");
const visible = computed(() =>
  issues.value.filter((i) =>
    `${i.key} ${i.summary}`
      .toLocaleLowerCase("pl")
      .includes(query.value.toLocaleLowerCase("pl")),
  ),
);
async function run(fn: () => Promise<void>) {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    await fn();
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}
async function fetchIssues() {
  issues.value = [];
  warnings.value = [];
  loaded.value = false;
  updatedAt.value = "";
  if (!settings.value.config.boards.length) return;
  const result = await invoke<{ issues: Issue[]; warnings: string[] }>(
    "jira_issues",
    { openOnly: openOnly.value },
  );
  issues.value = result.issues;
  warnings.value = result.warnings;
  loaded.value = true;
  updatedAt.value = new Date().toLocaleTimeString("pl-PL", {
    hour: "2-digit",
    minute: "2-digit",
  });
}
async function connect() {
  await run(async () => {
    try {
      settings.value = await invoke<Settings>("jira_connect", {
        site: site.value,
        email: email.value,
        apiToken: apiToken.value,
      });
    } finally {
      apiToken.value = "";
    }
    site.value = settings.value.config.site;
    email.value = settings.value.config.email;
    editingConnection.value = false;
    if (!settings.value.config.boards.length) {
      chosen.value = [];
      pickingBoards.value = true;
      await fetchBoards(false);
    } else await fetchIssues();
  });
}
async function disconnect() {
  await run(async () => {
    settings.value = await invoke<Settings>("jira_disconnect");
    issues.value = [];
    boards.value = [];
    chosen.value = [];
    apiToken.value = "";
    email.value = "";
    warnings.value = [];
    loaded.value = false;
    updatedAt.value = "";
    editingConnection.value = false;
    pickingBoards.value = false;
  });
}
async function fetchBoards(more: boolean) {
  const page = await invoke<{ values: Board[]; nextStart: number | null }>(
    "jira_boards",
    {
      name: boardName.value.trim(),
      startAt: more ? (nextStart.value ?? 0) : 0,
    },
  );
  const combined = more ? [...boards.value, ...page.values] : page.values;
  boards.value = [...new Map(combined.map((b) => [b.id, b])).values()];
  nextStart.value = page.nextStart;
}
function select(board: Board) {
  if (chosen.value.some((b) => b.id === board.id))
    chosen.value = chosen.value.filter((b) => b.id !== board.id);
  else if (chosen.value.length < 20) chosen.value.push(board);
}
async function chooseBoards() {
  await run(async () => {
    chosen.value = [...settings.value.config.boards];
    pickingBoards.value = true;
    await fetchBoards(false);
  });
}
async function saveBoards() {
  await run(async () => {
    await invoke("jira_select_boards", { boards: chosen.value });
    settings.value.config.boards = [...chosen.value];
    pickingBoards.value = false;
    await fetchIssues();
  });
}
async function start(issue: Issue) {
  await run(async () =>
    emit("updated", await invoke<Snapshot>("jira_start", { key: issue.key })),
  );
}
async function open(url: string) {
  await run(async () => {
    await invoke("jira_open", { url });
  });
}
onMounted(async () => {
  if (!desktop) return;
  await run(async () => {
    settings.value = await invoke<Settings>("jira_settings");
    site.value = settings.value.config.site;
    email.value = settings.value.config.email;
    if (settings.value.connected) {
      if (settings.value.config.boards.length) await fetchIssues();
      else {
        pickingBoards.value = true;
        await fetchBoards(false);
      }
    }
  });
});
</script>
<template>
  <section class="jira-view">
    <div class="page-heading">
      <h1>Jira</h1>
      <div v-if="settings.connected" class="jira-actions">
        <span class="hint account-name">{{ settings.config.displayName }}</span
        ><button
          class="icon-button"
          aria-label="Ustawienia połączenia Jiry"
          title="Ustawienia połączenia"
          :disabled="busy"
          @click="editingConnection = !editingConnection"
        >
          <Settings2 :size="18" />
        </button>
      </div>
    </div>
    <p v-if="!desktop" class="empty-state">
      Połączenie z Jirą jest dostępne w aplikacji desktopowej.
    </p>
    <template v-else>
      <p v-if="error" class="error" role="alert">{{ error }}</p>
      <form
        v-if="!settings.connected || editingConnection"
        class="jira-connection"
        @submit.prevent="connect"
      >
        <h2>
          {{ settings.connected ? "Połączenie z Jirą" : "Połącz swoje konto" }}
        </h2>
        <div class="connection-fields">
          <label
            >Adres Jiry<input
              v-model="site"
              type="url"
              required
              aria-label="Adres Jiry"
              :disabled="busy" /></label
          ><label
            >E-mail Atlassian<input
              v-model="email"
              type="email"
              required
              autocomplete="username"
              aria-label="E-mail Atlassian"
              :disabled="busy" /></label
          ><label
            >Token API<input
              v-model="apiToken"
              type="password"
              required
              autocomplete="off"
              aria-label="Token API"
              :disabled="busy"
          /></label>
        </div>
        <p class="hint">
          Użyj tokenu API bez zakresów. Token zostanie zapisany w magazynie
          poświadczeń Windows.
        </p>
        <div class="connection-actions">
          <button
            class="text-link"
            type="button"
            :disabled="busy"
            @click="
              open(
                'https://id.atlassian.com/manage-profile/security/api-tokens',
              )
            "
          >
            Utwórz token Atlassian <ExternalLink :size="13" /></button
          ><button
            v-if="settings.connected"
            type="button"
            class="danger"
            :disabled="busy"
            @click="disconnect"
          >
            Rozłącz konto</button
          ><button class="primary-button" :disabled="busy">
            {{ busy ? "Łączenie…" : "Połącz i sprawdź" }}
          </button>
        </div>
      </form>
      <template v-if="settings.connected">
        <div class="jira-toolbar">
          <div class="jira-board-chips">
            <span v-for="board in settings.config.boards" :key="board.id">{{
              board.name
            }}</span
            ><span v-if="!settings.config.boards.length" class="hint"
              >Brak wybranych tablic</span
            >
          </div>
          <button
            class="secondary-button"
            :disabled="busy"
            @click="chooseBoards"
          >
            Wybierz tablice
          </button>
        </div>
        <section
          v-if="pickingBoards"
          class="board-picker"
          aria-label="Wybór tablic"
        >
          <form
            class="board-search"
            @submit.prevent="run(() => fetchBoards(false))"
          >
            <input
              v-model="boardName"
              aria-label="Szukaj tablicy"
              placeholder="Nazwa tablicy…"
              :disabled="busy"
            /><button class="secondary-button" :disabled="busy">
              Szukaj tablic
            </button>
          </form>
          <div class="chosen-boards">
            <button
              v-for="board in chosen"
              :key="board.id"
              :disabled="busy"
              :aria-label="'Odznacz tablicę: ' + board.name"
              @click="select(board)"
            >
              {{ board.name }} ×
            </button>
          </div>
          <div class="board-options">
            <label v-for="board in boards" :key="board.id"
              ><input
                type="checkbox"
                :checked="chosen.some((b) => b.id === board.id)"
                :disabled="
                  busy ||
                  (chosen.length >= 20 &&
                    !chosen.some((b) => b.id === board.id))
                "
                @change="select(board)"
              />{{ board.name }} <small>#{{ board.id }}</small></label
            >
            <p v-if="!boards.length && !busy" class="hint">
              Brak tablic pasujących do wyszukiwania.
            </p>
          </div>
          <div class="connection-actions">
            <button
              v-if="nextStart !== null"
              class="secondary-button"
              :disabled="busy"
              @click="run(() => fetchBoards(true))"
            >
              Pokaż więcej tablic</button
            ><span class="hint">Wybrano {{ chosen.length }} / 20</span
            ><button
              class="secondary-button"
              :disabled="busy"
              @click="pickingBoards = false"
            >
              Anuluj</button
            ><button
              class="primary-button"
              :disabled="busy"
              @click="saveBoards"
            >
              Zapisz tablice
            </button>
          </div>
        </section>
        <div v-if="settings.config.boards.length" class="jira-search-toolbar">
          <label class="search"
            ><Search :size="15" /><input
              v-model="query"
              aria-label="Szukaj moich zgłoszeń"
              placeholder="Klucz lub nazwa zgłoszenia…" /></label
          ><label class="check-label"
            ><input
              v-model="openOnly"
              type="checkbox"
              :disabled="busy"
              @change="run(fetchIssues)"
            />Tylko otwarte</label
          ><button
            class="secondary-button"
            :disabled="busy"
            @click="run(fetchIssues)"
          >
            <RefreshCw :size="15" />Odśwież
          </button>
        </div>
        <div v-if="warnings.length" class="error" role="alert">
          <div>
            <strong>Nie wszystkie tablice zostały pobrane.</strong>
            <p v-for="warning in warnings" :key="warning">{{ warning }}</p>
          </div>
        </div>
        <p v-if="settings.config.boards.length" class="jira-result-info">
          Przypisane do mnie · {{ visible.length }} zgłoszeń<span
            v-if="updatedAt"
          >
            · Odświeżono {{ updatedAt }}</span
          >
        </p>
        <p v-if="busy" class="hint" role="status">Proszę czekać…</p>
        <div class="jira-issues" :aria-busy="busy">
          <article
            v-for="issue in visible"
            :key="issue.key"
            class="jira-issue"
            :class="{ isRunning: props.activeUrl === issue.url }"
          >
            <div class="jira-issue-body">
              <div class="jira-issue-meta">
                <button
                  class="text-link"
                  :aria-label="'Otwórz w Jirze: ' + issue.key"
                  :disabled="busy"
                  @click="open(issue.url)"
                >
                  {{ issue.key }}<ExternalLink :size="12" /></button
                ><span class="jira-status">{{ issue.status }}</span>
              </div>
              <h2>{{ issue.summary }}</h2>
              <p>{{ issue.boards.join(" · ") }}</p>
            </div>
            <button
              v-if="props.activeUrl === issue.url"
              class="row-timer stop"
              :disabled="busy"
              :aria-label="'Zatrzymaj: ' + issue.key"
              @click="emit('stop')"
            >
              <Square :size="12" />Stop</button
            ><button
              v-else
              class="row-timer"
              :disabled="busy"
              :aria-label="'Rozpocznij: ' + issue.key"
              @click="start(issue)"
            >
              <Play :size="12" />Rozpocznij
            </button>
          </article>
        </div>
        <p
          v-if="loaded && !visible.length && !busy && !warnings.length"
          class="empty-state"
        >
          {{
            query
              ? "Brak zgłoszeń pasujących do wyszukiwania."
              : "Brak przypisanych Ci zgłoszeń na wybranych tablicach."
          }}
        </p>
      </template>
    </template>
  </section>
</template>
