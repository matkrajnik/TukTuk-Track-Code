<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import {
  Download,
  Plug,
  PlugZap,
  Plus,
  Trash2,
  Unplug,
} from "lucide-vue-next";
import ConfirmDialog from "./ConfirmDialog.vue";
import {
  redmineAccount,
  redmineAccountUrl,
  redmineConnect,
  redmineConnected,
  redmineDisconnect,
  redmineOpen,
  redmineProjects,
  redmineSelectProjects,
  redmineStatuses,
  redmineTest,
  type RedmineAccount,
  type RedmineProject,
  type RedmineStatus,
} from "../store";
import type { AppSettings } from "../settings";

/**
 * Ustawienia → Redmine (PLAN_1.9.md, R5): połączenie kluczem API (adres z
 * ewentualnym prefiksem ścieżki, klucz w magazynie poświadczeń Windows,
 * opcja niezaufanego certyfikatu), śledzone projekty widoku Redmine oraz
 * przełączniki i statusy. Klucz API nigdy nie wraca z backendu: pole jest
 * puste także po połączeniu. Redmine jest niezależne od Jiry.
 */
const props = defineProps<{ settings: AppSettings; saving: boolean }>();
const emit = defineEmits<{ save: [patch: Partial<AppSettings>] }>();

const account = ref<RedmineAccount>({
  config: {
    site: "",
    login: "",
    userId: 0,
    displayName: "",
    acceptInvalidCert: false,
    projects: [],
  },
  connected: false,
});
const site = ref("");
const apiKey = ref("");
const acceptInvalidCert = ref(false);
/** „Zaawansowane” rozwinięte, gdy opcja certyfikatu jest zapisana jako włączona. */
const advancedOpen = ref(false);
const busy = ref(false);
const error = ref("");
const notice = ref("");
const confirming = ref(false);
const message = (e: unknown) => String(e instanceof Error ? e.message : e);

/** Przy `http` klucz API idzie otwartym tekstem. */
const insecure = computed(() => /^\s*http:\/\//i.test(site.value));
/** „Gdzie znaleźć klucz API”: `<adres>/my/account`, aktywne po wpisaniu adresu. */
const accountUrl = computed(() => redmineAccountUrl(site.value));
const accountLabel = computed(() => {
  const c = account.value.config;
  if (!account.value.connected) return "Nie połączono";
  const name = c.displayName || c.login;
  return c.login && name !== c.login
    ? `Połączono jako ${name} (${c.login})`
    : `Połączono jako ${name}`;
});

function apply(next: RedmineAccount, fields: boolean) {
  account.value = next;
  redmineConnected.value = next.connected;
  if (fields) {
    site.value = next.config.site;
    acceptInvalidCert.value = next.config.acceptInvalidCert;
    if (next.config.acceptInvalidCert) advancedOpen.value = true;
  }
}
async function load(fields = false) {
  apply(await redmineAccount(), fields);
}
function clearMessages() {
  error.value = "";
  notice.value = "";
}

async function connect() {
  busy.value = true;
  clearMessages();
  try {
    apply(
      await redmineConnect(site.value, apiKey.value, acceptInvalidCert.value),
      true,
    );
    notice.value = `${accountLabel.value}.`;
  } catch (e) {
    error.value = message(e);
    await load().catch(() => {});
  } finally {
    // Klucz nie zostaje w formularzu ani po sukcesie, ani po błędzie.
    apiKey.value = "";
    busy.value = false;
  }
}
async function testConnection() {
  busy.value = true;
  clearMessages();
  try {
    notice.value = await redmineTest();
    await load().catch(() => {});
  } catch (e) {
    error.value = message(e);
  } finally {
    busy.value = false;
  }
}
async function disconnect() {
  busy.value = true;
  clearMessages();
  try {
    apply(await redmineDisconnect(), false);
    confirming.value = false;
    picking.value = false;
    statuses.value = [];
  } catch (e) {
    error.value = message(e);
  } finally {
    busy.value = false;
  }
}
function openConfirm() {
  clearMessages();
  confirming.value = true;
}
async function openAccountPage() {
  if (!accountUrl.value) return;
  clearMessages();
  try {
    await redmineOpen(accountUrl.value);
  } catch (e) {
    error.value = message(e);
  }
}

// --- Śledzone projekty -----------------------------------------------------

const maxProjects = 50;
const tracked = computed(() => account.value.config.projects);
const picking = ref(false),
  projectsBusy = ref(false),
  projectQuery = ref(""),
  projectsError = ref("");
const allProjects = ref<RedmineProject[]>([]);
const chosen = ref<RedmineProject[]>([]);

/** Projekty w kolejności drzewa: podprojekt pod rodzicem, z głębokością wcięcia. */
const tree = computed(() => {
  const list = allProjects.value;
  const ids = new Set(list.map((p) => p.id));
  const children = new Map<number | null, RedmineProject[]>();
  for (const p of list) {
    const parent = p.parentId !== null && ids.has(p.parentId) ? p.parentId : null;
    children.set(parent, [...(children.get(parent) ?? []), p]);
  }
  const out: { project: RedmineProject; depth: number }[] = [];
  const seen = new Set<number>();
  const walk = (parent: number | null, depth: number) => {
    for (const p of children.get(parent) ?? []) {
      if (seen.has(p.id)) continue;
      seen.add(p.id);
      out.push({ project: p, depth });
      walk(p.id, depth + 1);
    }
  };
  walk(null, 0);
  // Projekty w cyklu rodziców (błędne dane) na końcu bez wcięcia.
  for (const p of list) if (!seen.has(p.id)) out.push({ project: p, depth: 0 });
  return out;
});
const visibleProjects = computed(() => {
  const q = projectQuery.value.trim().toLocaleLowerCase("pl");
  return q
    ? tree.value.filter(({ project }) =>
        project.name.toLocaleLowerCase("pl").includes(q),
      )
    : tree.value;
});
const isChosen = (id: number) => chosen.value.some((p) => p.id === id);
function toggleProject(project: RedmineProject) {
  chosen.value = isChosen(project.id)
    ? chosen.value.filter((p) => p.id !== project.id)
    : [...chosen.value, { ...project }];
}
async function openPicker() {
  if (picking.value) {
    picking.value = false;
    return;
  }
  projectsBusy.value = true;
  projectsError.value = "";
  try {
    allProjects.value = await redmineProjects();
    chosen.value = tracked.value.map((p) => ({ ...p }));
    projectQuery.value = "";
    picking.value = true;
  } catch (e) {
    projectsError.value = message(e);
  } finally {
    projectsBusy.value = false;
  }
}
async function saveProjects(next: RedmineProject[]) {
  projectsBusy.value = true;
  projectsError.value = "";
  try {
    await redmineSelectProjects(next);
    await load();
    return true;
  } catch (e) {
    projectsError.value = message(e);
    return false;
  } finally {
    projectsBusy.value = false;
  }
}
async function savePicked() {
  if (await saveProjects(chosen.value)) picking.value = false;
}
function removeProject(id: number) {
  void saveProjects(tracked.value.filter((p) => p.id !== id));
}

// --- Statusy z Redmine -----------------------------------------------------

type Target = "new" | "move" | "waiting";
const statuses = ref<RedmineStatus[]>([]);
const statusTarget = ref<Target>("new");
const statusesBusy = ref(false),
  statusesError = ref(""),
  statusesNotice = ref("");
async function loadStatuses() {
  if (statuses.value.length) {
    statuses.value = [];
    return;
  }
  statusesBusy.value = true;
  statusesError.value = "";
  statusesNotice.value = "";
  try {
    statuses.value = await redmineStatuses(true);
    if (!statuses.value.length)
      statusesNotice.value = "Redmine nie zwrócił żadnych statusów.";
  } catch (e) {
    statusesError.value = message(e);
  } finally {
    statusesBusy.value = false;
  }
}
const sameName = (a: string, b: string) =>
  a.toLocaleLowerCase("pl") === b.toLocaleLowerCase("pl");
/** Kliknięty status trafia do wybranego pola (listy bez powtórek). */
function insertStatus(name: string) {
  statusesNotice.value = "";
  const s = props.settings;
  if (statusTarget.value === "move") {
    emit("save", { redmineMoveStatus: name });
    statusesNotice.value = `Docelowy status: „${name}”.`;
    return;
  }
  const field =
    statusTarget.value === "new" ? "redmineNewStatuses" : "waitingRedmineStatuses";
  const list = s[field];
  const label =
    statusTarget.value === "new" ? "statusów początkowych" : "statusów „Czekam na”";
  if (list.some((n) => sameName(n, name))) {
    statusesNotice.value = `„${name}” już jest na liście ${label}.`;
    return;
  }
  emit("save", { [field]: [...list, name] });
  statusesNotice.value = `Dodano „${name}” do ${label}.`;
}
const lines = (e: Event) => (e.target as HTMLTextAreaElement).value.split("\n");
const checked = (e: Event) => (e.target as HTMLInputElement).checked;
const text = (e: Event) => (e.target as HTMLInputElement).value;

let alive = true;
onMounted(async () => {
  try {
    const next = await redmineAccount();
    if (alive) apply(next, true);
  } catch (e) {
    if (alive) error.value = message(e);
  }
});
onUnmounted(() => {
  alive = false;
});
</script>

<template>
  <p class="hint">
    Połącz konto Redmine kluczem API, żeby widok Redmine pokazywał Twoje
    zgłoszenia, a zadania mogły być powiązane ze zgłoszeniami Redmine.
  </p>
  <h3>Połączenie</h3>
  <p class="ts-account redmine-account">{{ accountLabel }}</p>
  <div class="planning-fields settings-fields redmine-connection-fields">
    <label
      >Adres Redmine<input
        v-model="site"
        type="url"
        spellcheck="false"
        autocomplete="off"
        placeholder="https://redmine.firma.pl"
        :disabled="busy"
    /></label>
    <label
      >Klucz API<input
        v-model="apiKey"
        type="password"
        spellcheck="false"
        autocomplete="off"
        :placeholder="account.connected ? 'zapisany' : ''"
        :disabled="busy"
        @keydown.enter="connect"
    /></label>
  </div>
  <p v-if="insecure" class="hint redmine-warning" role="note">
    Adres zaczyna się od http: klucz API jest wysyłany bez szyfrowania. Użyj
    https, jeśli Twój Redmine go obsługuje.
  </p>
  <p class="hint">
    Pełny adres, pod którym otwierasz Redmine, także z prefiksem ścieżki (np.
    https://firma.pl/redmine).
    <template v-if="account.connected">
      Klucz API jest zapisany w magazynie poświadczeń Windows. Wpisz go tylko,
      gdy chcesz połączyć się ponownie (np. po zmianie klucza lub adresu).
    </template>
  </p>
  <p class="hint">
    <button
      type="button"
      class="text-link"
      :disabled="!accountUrl"
      @click="openAccountPage"
    >
      Gdzie znaleźć klucz API</button
    >: w Redmine Moje konto → Klucz dostępu API → Pokaż.
  </p>
  <details class="ts-advanced" :open="advancedOpen">
    <summary>Zaawansowane</summary>
    <label class="inline-check"
      ><input
        v-model="acceptInvalidCert"
        type="checkbox"
        :disabled="busy"
      />Akceptuj niezaufany certyfikat (tylko sieć wewnętrzna)</label
    >
    <p class="hint">
      Domyślnie aplikacja ufa certyfikatom z magazynu Windows, także firmowego
      urzędu certyfikacji. Włącz tylko dla serwera w sieci wewnętrznej, którego
      certyfikatu nie da się zainstalować w Windows; zmiana działa po „Połącz”.
    </p>
    <p v-if="acceptInvalidCert" class="hint redmine-warning">
      Połączenie nie sprawdza tożsamości serwera.
    </p>
  </details>
  <p v-if="error" class="error" role="alert">{{ error }}</p>
  <p v-if="notice" class="hint ts-notice" role="status">{{ notice }}</p>
  <div class="planning-actions">
    <button
      class="action-button"
      :disabled="busy || !site.trim() || !apiKey"
      @click="connect"
    >
      <Plug :size="14" />Połącz
    </button>
    <template v-if="account.connected">
      <button class="today-button" :disabled="busy" @click="testConnection">
        <PlugZap :size="14" />Testuj połączenie
      </button>
      <button class="today-button" :disabled="busy" @click="openConfirm">
        <Unplug :size="14" />Rozłącz
      </button>
    </template>
  </div>

  <h3 id="redmine-projects-heading">Śledzone projekty</h3>
  <p class="hint">
    Widok Redmine pokazuje zgłoszenia tych projektów; nowe przypisania są
    sprawdzane we wszystkich projektach.
  </p>
  <ul
    v-if="tracked.length"
    class="redmine-tracked"
    aria-labelledby="redmine-projects-heading"
  >
    <li v-for="p in tracked" :key="p.id">
      <span>{{ p.name }}</span>
      <small v-if="p.identifier" class="mono">{{ p.identifier }}</small>
      <button
        class="action-button"
        :aria-label="`Usuń projekt ${p.name}`"
        :disabled="projectsBusy"
        @click="removeProject(p.id)"
      >
        <Trash2 :size="14" />Usuń
      </button>
    </li>
  </ul>
  <p v-else class="hint">
    Brak śledzonych projektów: widok Redmine pokazuje moje zgłoszenia ze
    wszystkich projektów.
  </p>
  <div class="planning-actions">
    <button
      class="secondary-button"
      :disabled="projectsBusy || !account.connected"
      :aria-expanded="picking"
      @click="openPicker"
    >
      <Plus v-if="!picking" :size="14" />{{
        picking ? "Zwiń listę projektów" : "Dodaj projekt…"
      }}
    </button>
  </div>
  <p v-if="!account.connected" class="hint">
    Lista projektów pojawi się po połączeniu konta.
  </p>
  <p v-if="projectsError" class="error" role="alert">{{ projectsError }}</p>
  <section
    v-if="picking"
    class="board-picker redmine-project-picker"
    aria-label="Wybór projektów Redmine"
  >
    <input
      v-model="projectQuery"
      type="search"
      aria-label="Szukaj projektu"
      placeholder="Nazwa projektu…"
      :disabled="projectsBusy"
    />
    <div class="board-options redmine-project-options">
      <label
        v-for="{ project, depth } in visibleProjects"
        :key="project.id"
        :style="{ paddingLeft: `${8 + depth * 20}px` }"
        ><input
          type="checkbox"
          :checked="isChosen(project.id)"
          :disabled="
            projectsBusy ||
            (chosen.length >= maxProjects && !isChosen(project.id))
          "
          @change="toggleProject(project)"
        />{{ project.name }}
        <small v-if="project.identifier">{{ project.identifier }}</small></label
      >
      <p v-if="!visibleProjects.length" class="hint">
        {{
          allProjects.length
            ? "Brak projektów pasujących do wyszukiwania."
            : "Redmine nie zwrócił żadnych projektów."
        }}
      </p>
    </div>
    <div class="connection-actions">
      <span class="hint">Wybrano {{ chosen.length }} / {{ maxProjects }}</span
      ><button
        class="secondary-button"
        :disabled="projectsBusy"
        @click="picking = false"
      >
        Anuluj</button
      ><button
        class="primary-button"
        :disabled="projectsBusy"
        @click="savePicked"
      >
        Zapisz projekty
      </button>
    </div>
  </section>

  <h3>Zgłoszenia i statusy</h3>
  <label class="inline-check"
    ><input
      :checked="settings.redmineWrite"
      type="checkbox"
      :disabled="saving"
      @change="emit('save', { redmineWrite: checked($event) })"
    />Pozwól aplikacji zmieniać zgłoszenia w Redmine</label
  >
  <p class="hint">
    Wyłączone blokuje każdą zmianę w Redmine z TukTuk Track (status,
    przypisanie, komentarz); aplikacja wtedy tylko czyta zgłoszenia.
  </p>
  <label class="inline-check"
    ><input
      :checked="settings.redmineAskMove"
      type="checkbox"
      :disabled="saving || !settings.redmineWrite"
      @change="emit('save', { redmineAskMove: checked($event) })"
    />Pytaj o przesunięcie zgłoszenia przy starcie pracy</label
  >
  <label class="settings-statuses"
    >Statusy początkowe (jeden w wierszu)<textarea
      :value="settings.redmineNewStatuses.join('\n')"
      rows="3"
      :disabled="saving"
      @change="emit('save', { redmineNewStatuses: lines($event) })"
    />
  </label>
  <div class="planning-fields settings-fields">
    <label
      >Docelowy status przy starcie pracy<input
        :value="settings.redmineMoveStatus"
        maxlength="80"
        :disabled="saving || !settings.redmineWrite"
        @change="emit('save', { redmineMoveStatus: text($event) })"
    /></label>
  </div>
  <p class="hint">
    Redmine nie ma kategorii statusów, więc „Do zrobienia” to statusy
    początkowe z tej listy. Start pomiaru zadania ze zgłoszeniem w takim
    statusie pyta o przesunięcie do docelowego statusu (polskie instalacje
    mają zwykle „W toku”). Pomiar startuje od razu, a zgłoszenie zmienia się
    dopiero po kliknięciu „Przesuń”.
  </p>
  <label class="settings-statuses"
    >Statusy oznaczające „Czekam na” (jeden w wierszu)<textarea
      :value="settings.waitingRedmineStatuses.join('\n')"
      rows="3"
      :disabled="saving"
      @change="emit('save', { waitingRedmineStatuses: lines($event) })"
    />
  </label>
  <p class="hint">
    Gdy zgłoszenie zadania wejdzie w taki status, kafelek proponuje
    oznaczenie „Czekam na”, a gdy z niego wyjdzie — odblokowanie. Wielkość
    liter nie ma znaczenia.
  </p>
  <template v-if="account.connected">
    <div class="planning-actions">
      <button
        class="secondary-button"
        :disabled="statusesBusy"
        :aria-expanded="statuses.length > 0"
        @click="loadStatuses"
      >
        <Download v-if="!statuses.length" :size="14" />{{
          statuses.length ? "Zwiń statusy z Redmine" : "Wstaw statusy z Redmine"
        }}
      </button>
    </div>
    <p v-if="statusesError" class="error" role="alert">{{ statusesError }}</p>
    <div
      v-if="statuses.length"
      class="redmine-status-picker"
      role="group"
      aria-label="Statusy z Redmine"
    >
      <div class="redmine-status-target">
        <span aria-hidden="true">Wstaw do</span
        ><select v-model="statusTarget" aria-label="Wstaw do" :disabled="saving">
          <option value="new">statusów początkowych</option>
          <option value="move">statusu docelowego</option>
          <option value="waiting">statusów „Czekam na”</option>
        </select>
      </div>
      <div class="chosen-boards redmine-status-chips">
        <button
          v-for="s in statuses"
          :key="s.id"
          type="button"
          :aria-label="`Wstaw status ${s.name}`"
          :disabled="saving"
          @click="insertStatus(s.name)"
        >
          {{ s.name }}<small v-if="s.isClosed"> (zamknięty)</small>
        </button>
      </div>
    </div>
    <p v-if="statusesNotice" class="hint" role="status">
      {{ statusesNotice }}
    </p>
  </template>
  <label class="inline-check"
    ><input
      :checked="settings.redmineAutoImport"
      type="checkbox"
      :disabled="saving"
      @change="emit('save', { redmineAutoImport: checked($event) })"
    />Dodawaj nowe zgłoszenia przypisane do mnie</label
  >
  <p class="hint">
    Co 5 minut, gdy konto Redmine jest połączone, we wszystkich projektach.
    Nowe zgłoszenie trafia do „Do zaplanowania” z powiadomieniem; obecne
    przypisania przy pierwszym sprawdzeniu nie są importowane.
  </p>
  <div class="planning-fields settings-fields">
    <label
      >Prefiks nazwy gałęzi<input
        :value="settings.redmineBranchPrefix"
        maxlength="40"
        placeholder="feature/"
        :disabled="saving"
        @change="emit('save', { redmineBranchPrefix: text($event) })"
    /></label>
  </div>
  <p class="hint">
    „Kopiuj nazwę gałęzi” dla zgłoszenia Redmine daje np.
    {{ settings.redmineBranchPrefix }}1234-krotki-tytul. Pusty prefiks — sama
    nazwa.
  </p>

  <h3>Jak to działa</h3>
  <p class="hint">
    Aplikacja czyta z Redmine Twoje konto, listę projektów, statusy i
    zgłoszenia: przypisane do Ciebie, śledzonych projektów i powiązane z
    zadaniami. Zapisuje w Redmine wyłącznie po kliknięciu: zmianę statusu,
    „Przypisz do mnie” i komentarz. Nic nie zmienia się samo, a wyłączony
    przełącznik zapisów blokuje je także w aplikacji, nie tylko w interfejsie.
  </p>
  <p class="hint">
    Redmine i Jira są od siebie niezależne: osobne konta, ustawienia i widoki.
    Zadanie ma najwyżej jedno zgłoszenie — z Jiry albo z Redmine.
  </p>
  <ConfirmDialog
    v-if="confirming"
    title="Rozłączyć konto Redmine?"
    confirm-label="Rozłącz"
    :busy="busy"
    :error="error"
    @close="confirming = false"
    @confirm="disconnect"
  >
    <p>
      Klucz API zostanie usunięty z magazynu poświadczeń Windows. Adres,
      śledzone projekty i powiązania zadań ze zgłoszeniami zostają.
    </p>
  </ConfirmDialog>
</template>
