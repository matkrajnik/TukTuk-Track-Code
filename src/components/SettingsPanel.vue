<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import ShortcutInput from "./ShortcutInput.vue";
import TemplateEditor from "./TemplateEditor.vue";
import CalendarSettings from "./CalendarSettings.vue";
import TimesheetSettings from "./TimesheetSettings.vue";
import BrowserExtensionStatus from "./BrowserExtensionStatus.vue";
import { openView, type SettingsSection } from "../navigation";
import { checkForUpdate } from "../updater";
import {
  defaultSettings,
  maxCardFields,
  onSettingsChanged,
  onShortcutsChanged,
  readAppInfo,
  readSettings,
  writeSettings,
  type AppInfo,
  type AppSettings,
} from "../settings";
import {
  jiraFieldName,
  jiraFields,
  jiraRefreshFields,
  loadTemplates,
  readAutostart,
  templates,
  writeAutostart,
  type Autostart,
  type JiraFieldInfo,
} from "../store";
/** Widoczna sekcja; zakładka Ustawienia pokazuje jedną naraz. */
const props = defineProps<{ section: SettingsSection }>();
/** Formularz nowego szablonu w sekcji Szablony. */
const addingTemplate = ref(false);
const settings = ref<AppSettings>({
  ...defaultSettings,
  workDays: [...defaultSettings.workDays],
});
const info = ref<AppInfo>({
  version: "",
  shortcutErrors: { quick: "", toggle: "" },
});
/** Błąd zapisu skrótu (walidacja) pokazywany przy polu, nie w ogólnym alercie. */
const shortcutSaveError = ref({ quick: "", toggle: "" });
const autostart = ref<Autostart>({ enabled: false, hidden: false });
const busy = ref(false),
  error = ref("");
const weekDays = [
  { day: 1, name: "Pon" },
  { day: 2, name: "Wt" },
  { day: 3, name: "Śr" },
  { day: 4, name: "Czw" },
  { day: 5, name: "Pt" },
  { day: 6, name: "Sob" },
  { day: 7, name: "Nd" },
];
const hm = (minutes: number) =>
  `${Math.floor(minutes / 60)}:${String(minutes % 60).padStart(2, "0")}`;
const weekTarget = computed(() =>
  hm(settings.value.targetDayMinutes * settings.value.workDays.length),
);
async function save(patch: Partial<AppSettings>) {
  busy.value = true;
  error.value = "";
  try {
    settings.value = await writeSettings(patch);
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
    // Pole wraca do zapisanej wartości.
    const saved = await readSettings().catch(() => settings.value);
    settings.value = { ...saved, workDays: [...saved.workDays] };
  } finally {
    busy.value = false;
  }
}
const checked = (e: Event) => (e.target as HTMLInputElement).checked;
const text = (e: Event) => (e.target as HTMLInputElement).value;

/** „Sprawdź teraz” (Uruchamianie → Aktualizacje): wynik pod przyciskiem. */
const updateBusy = ref(false),
  updateMessage = ref("");
async function checkNow() {
  updateBusy.value = true;
  updateMessage.value = "";
  try {
    const result = await checkForUpdate();
    updateMessage.value =
      result.status === "latest" && info.value.version
        ? `Masz najnowszą wersję (${info.value.version}).`
        : result.message;
  } finally {
    updateBusy.value = false;
  }
}

/** Pola Jiry na kafelku: lista pól z Jiry, wybór i kolejność. */
const catalog = ref<JiraFieldInfo[]>([]);
const pickingFields = ref(false),
  fieldsBusy = ref(false),
  fieldQuery = ref(""),
  fieldsError = ref(""),
  fieldsNotice = ref("");
const fieldName = (id: string) => jiraFieldName(id, catalog.value);
const visibleFields = computed(() => {
  const q = fieldQuery.value.trim().toLocaleLowerCase("pl");
  return catalog.value.filter(
    (f) =>
      !["summary", "status"].includes(f.id) &&
      (!q ||
        f.name.toLocaleLowerCase("pl").includes(q) ||
        f.id.toLocaleLowerCase("pl").includes(q)),
  );
});
async function loadFields(refresh: boolean) {
  fieldsBusy.value = true;
  fieldsError.value = "";
  fieldsNotice.value = "";
  try {
    catalog.value = await jiraFields(refresh);
    pickingFields.value = true;
    if (refresh) fieldsNotice.value = `Pobrano ${catalog.value.length} pól.`;
  } catch (e) {
    fieldsError.value = String(e instanceof Error ? e.message : e);
  } finally {
    fieldsBusy.value = false;
  }
}
// Nazwy wybranych pól (także niestandardowych) z pamięci podręcznej listy pól,
// gdy sekcja Jira jest widoczna; błąd (np. konto niepołączone) nic nie pokazuje.
watch(
  () => props.section,
  async (section) => {
    if (section !== "jira" || catalog.value.length) return;
    try {
      catalog.value = await jiraFields(false);
    } catch {
      /* identyfikatory zamiast nazw do czasu pobrania listy */
    }
  },
  { immediate: true },
);
async function toggleFieldPicker() {
  if (pickingFields.value) {
    pickingFields.value = false;
    return;
  }
  if (catalog.value.length) pickingFields.value = true;
  else await loadFields(false);
}
/** Zapis listy pól i od razu odświeżenie wartości na zadaniach. */
async function saveFields(next: string[]) {
  fieldsError.value = "";
  fieldsNotice.value = "";
  await save({ jiraCardFields: next });
  if (error.value) return;
  try {
    const count = await jiraRefreshFields();
    fieldsNotice.value = count
      ? `Odświeżono pola ${count} zgłoszeń.`
      : "Zapisano. Wartości pojawią się przy najbliższym pobraniu z Jiry.";
  } catch (e) {
    fieldsError.value = `Zapisano, ale nie udało się odświeżyć wartości: ${String(
      e instanceof Error ? e.message : e,
    )}`;
  }
}
function toggleField(id: string) {
  const current = settings.value.jiraCardFields;
  saveFields(
    current.includes(id) ? current.filter((f) => f !== id) : [...current, id],
  );
}
function removeField(id: string) {
  saveFields(settings.value.jiraCardFields.filter((f) => f !== id));
}
function moveField(index: number, delta: -1 | 1) {
  const next = [...settings.value.jiraCardFields];
  const target = index + delta;
  if (target < 0 || target >= next.length) return;
  [next[index], next[target]] = [next[target], next[index]];
  saveFields(next);
}
const number = (e: Event) => {
  const value = (e.target as HTMLInputElement).value;
  return value === "" ? NaN : Number(value);
};
async function saveShortcut(field: "quick" | "toggle", value: string) {
  busy.value = true;
  shortcutSaveError.value = { quick: "", toggle: "" };
  try {
    settings.value = await writeSettings(
      field === "quick" ? { shortcutQuick: value } : { shortcutToggle: value },
    );
  } catch (e) {
    shortcutSaveError.value[field] = String(e instanceof Error ? e.message : e);
  } finally {
    busy.value = false;
  }
}
const shortcutError = (field: "quick" | "toggle") =>
  shortcutSaveError.value[field] || info.value.shortcutErrors[field];
function toggleDay(day: number, on: boolean) {
  const days = settings.value.workDays.filter((d) => d !== day);
  void save({ workDays: on ? [...days, day] : days });
}
/** Autostart: stan z rejestru, „schowane” jako ustawienie autostartHidden. */
async function saveAutostart(field: keyof Autostart, e: Event) {
  const input = e.target as HTMLInputElement;
  const next = { ...autostart.value, [field]: input.checked };
  busy.value = true;
  error.value = "";
  try {
    autostart.value = await writeAutostart(next.enabled, next.hidden);
    settings.value = {
      ...settings.value,
      autostartHidden: autostart.value.hidden,
    };
  } catch (err) {
    error.value = String(err instanceof Error ? err.message : err);
    autostart.value = await readAutostart().catch(() => autostart.value);
    // Odczytana wartość bywa taka sama jak przed kliknięciem, więc Vue nie
    // poprawi pola samo; cofamy je ręcznie do stanu z rejestru i ustawień.
    input.checked = autostart.value[field];
  } finally {
    busy.value = false;
  }
}
let unlisten: (() => void) | undefined;
let unlistenShortcuts: (() => void) | undefined;
let alive = true;
onMounted(async () => {
  try {
    const stop = await onSettingsChanged((s) => (settings.value = s));
    // Panel mógł zostać odmontowany w trakcie rejestracji nasłuchu.
    if (!alive) return stop();
    unlisten = stop;
    const stopShortcuts = await onShortcutsChanged(
      (errors) => (info.value = { ...info.value, shortcutErrors: errors }),
    );
    if (!alive) return stopShortcuts();
    unlistenShortcuts = stopShortcuts;
    settings.value = await readSettings();
    info.value = await readAppInfo();
    autostart.value = await readAutostart();
    await loadTemplates();
  } catch (e) {
    if (alive) error.value = String(e);
  }
});
onUnmounted(() => {
  alive = false;
  unlisten?.();
  unlistenShortcuts?.();
});
</script>
<template>
  <section class="dashboard-card settings-panel">
    <p v-if="error" class="error" role="alert">{{ error }}</p>
    <template v-if="section === 'work'">
      <h2>Czas pracy</h2>
      <label class="inline-check"
        ><input
          :checked="settings.idleEnabled"
          type="checkbox"
          :disabled="busy"
          @change="save({ idleEnabled: checked($event) })"
        />Zapytaj o czas po co najmniej 10 minutach bezczynności</label
      >
      <p class="hint">
        Decydujesz, czy zachować czas, czy zakończyć pomiar na początku przerwy.
        Nic nie jest obcinane automatycznie.
      </p>
      <label class="inline-check"
        ><input
          :checked="settings.idleAgents"
          type="checkbox"
          :disabled="busy || !settings.idleEnabled"
          @change="save({ idleAgents: checked($event) })"
        />Praca agentów AI (Claude Code, Codex) liczy się jako aktywność</label
      >
      <label class="inline-check"
        ><input
          :checked="settings.agentSuggest"
          type="checkbox"
          :disabled="busy"
          @change="save({ agentSuggest: checked($event) })"
        />Proponuj przełączenie na zadanie, nad którym pracuje agent AI</label
      >
      <p class="hint">
        Po 2 minutach pracy agenta nad innym zadaniem niż mierzone. Timer nigdy
        nie przełącza się sam.
      </p>
      <label class="inline-check"
        ><input
          :checked="settings.windowSuggest"
          type="checkbox"
          :disabled="busy"
          @change="save({ windowSuggest: checked($event) })"
        />Podpowiadaj zadanie na podstawie aktywnego okna</label
      >
      <p class="hint">
        Co 5 sekund aplikacja czyta tytuł i nazwę programu okna na pierwszym
        planie: klucz Jiry z przeglądarki, Teams lub Outlooka i folder projektu
        z VS Code, Cursora lub JetBrains. Tytuły zostają tylko w pamięci, nie
        trafiają do bazy ani logów. Propozycja przychodzi po 2 minutach przy tym
        samym zadaniu.
      </p>
      <label class="inline-check"
        ><input
          :checked="settings.browserExtension"
          type="checkbox"
          :disabled="busy"
          @change="save({ browserExtension: checked($event) })"
        />Rozszerzenie przeglądarki (Chrome i Edge)</label
      >
      <BrowserExtensionStatus :enabled="settings.browserExtension" />
      <template v-if="settings.agentMutedFolders.length">
        <p class="hint">Bez propozycji dla folderów (z podfolderami):</p>
        <ul class="muted-folders" aria-label="Foldery bez propozycji agentów">
          <li v-for="folder in settings.agentMutedFolders" :key="folder">
            <code class="mono">{{ folder }}</code
            ><button
              class="text-link"
              :disabled="busy"
              :aria-label="`Proponuj znowu dla ${folder}`"
              @click="
                save({
                  agentMutedFolders: settings.agentMutedFolders.filter(
                    (f) => f !== folder,
                  ),
                })
              "
            >
              Proponuj znowu
            </button>
          </li>
        </ul>
      </template>
      <div class="planning-fields settings-fields">
        <label
          >Cel dzienny (minuty)<input
            :value="settings.targetDayMinutes"
            type="number"
            min="0"
            max="1440"
            step="15"
            :disabled="busy"
            @change="save({ targetDayMinutes: number($event) })"
        /></label>
        <label
          >Początek dnia<input
            :value="settings.dayStart"
            type="time"
            :disabled="busy"
            @change="save({ dayStart: text($event) })"
        /></label>
      </div>
      <fieldset class="settings-days">
        <legend>Dni robocze</legend>
        <label v-for="d in weekDays" :key="d.day" class="inline-check"
          ><input
            type="checkbox"
            :checked="settings.workDays.includes(d.day)"
            :disabled="busy"
            :aria-label="'Dzień roboczy: ' + d.name"
            @change="toggleDay(d.day, checked($event))"
          />{{ d.name }}</label
        >
      </fieldset>
      <label class="inline-check"
        ><input
          :checked="settings.holidaysInTarget"
          type="checkbox"
          :disabled="busy"
          @change="save({ holidaysInTarget: checked($event) })"
        />Uwzględniaj święta w celu</label
      >
      <p class="hint">
        Cel dnia {{ hm(settings.targetDayMinutes) }}, cel tygodnia
        {{ weekTarget }}.
      </p>
      <p v-if="settings.holidaysInTarget" class="hint">
        Polskie święta ustawowe, także Wielkanoc i Boże Ciało, nie są dniami
        roboczymi: cel tygodnia i miesiąca jest wtedy niższy.
      </p>
      <label class="inline-check"
        ><input
          :checked="settings.planWithCorrection"
          type="checkbox"
          :disabled="busy"
          @change="save({ planWithCorrection: checked($event) })"
        />Planuj z poprawką szacunków</label
      >
      <p class="hint">
        Poprawka to mediana stosunku czasu rzeczywistego do szacunku ukończonych
        zadań z 90 dni (projektu albo wszystkich, co najmniej 5 zadań). Bez tej
        opcji jest tylko podpowiedzią „zwykle ~…”; włączona liczy się w
        pojemności tablicy tygodnia, osi planu i „Rozłóż”.
      </p>
    </template>
    <template v-if="section === 'focus'">
      <h2>Skupienie</h2>
      <div class="planning-fields settings-fields">
        <label
          >Blok skupienia (minuty)<input
            :value="settings.focusBlockMinutes"
            type="number"
            min="1"
            max="240"
            :disabled="busy"
            @change="save({ focusBlockMinutes: number($event) })"
        /></label>
        <label
          >Przerwa (minuty)<input
            :value="settings.focusBreakMinutes"
            type="number"
            min="1"
            max="60"
            :disabled="busy"
            @change="save({ focusBreakMinutes: number($event) })"
        /></label>
      </div>
      <div class="planning-actions">
        <button
          v-for="m in [25, 50]"
          :key="'b' + m"
          class="today-button"
          :aria-pressed="settings.focusBlockMinutes === m"
          :disabled="busy"
          @click="save({ focusBlockMinutes: m })"
        >
          Blok {{ m }} min</button
        ><button
          v-for="m in [5, 10]"
          :key="'p' + m"
          class="today-button"
          :aria-pressed="settings.focusBreakMinutes === m"
          :disabled="busy"
          @click="save({ focusBreakMinutes: m })"
        >
          Przerwa {{ m }} min
        </button>
      </div>
      <label class="inline-check"
        ><input
          :checked="settings.focusBreakStops"
          type="checkbox"
          :disabled="busy"
          @change="save({ focusBreakStops: checked($event) })"
        />Przerwa zatrzymuje pomiar</label
      >
      <p class="hint">
        „Skupienie” na pasku bieżącego pomiaru albo w widgecie zaczyna blok i
        uruchamia pomiar, jeśli nie trwa. W czasie bloku powiadomienia i
        propozycje agentów czekają i przychodzą po bloku w jednym zbiorczym.
        Stop albo inne zadanie kończy blok jako nieukończony.
      </p>
    </template>
    <template v-if="section === 'windows'">
      <h2>Okna</h2>
      <label class="inline-check"
        ><input
          :checked="settings.widgetEnabled"
          type="checkbox"
          :disabled="busy"
          @change="save({ widgetEnabled: checked($event) })"
        />Pokazuj mini-widget zawsze na wierzchu</label
      >
      <label class="inline-check"
        ><input
          :checked="settings.widgetBackgroundOnly"
          type="checkbox"
          :disabled="busy || !settings.widgetEnabled"
          @change="save({ widgetBackgroundOnly: checked($event) })"
        />Tylko gdy główne okno jest schowane lub bez fokusu</label
      >
      <p class="hint">
        Mini-widget pokazuje bieżący pomiar z przyciskami Start/Stop i ⇄. X na
        widgecie chowa go do końca sesji; przywrócisz go przełącznikiem w
        nagłówku.
      </p>
    </template>
    <template v-if="section === 'jira'">
      <h2>Jira</h2>
      <label class="inline-check"
        ><input
          :checked="settings.jiraWrite"
          type="checkbox"
          :disabled="busy"
          @change="save({ jiraWrite: checked($event) })"
        />Pozwól aplikacji zmieniać zgłoszenia w Jirze</label
      >
      <p class="hint">
        Wyłączone blokuje każdą zmianę w Jirze z Todo Track (przesuwanie
        zgłoszeń i inne zapisy); aplikacja wtedy tylko czyta zgłoszenia.
      </p>
      <label class="inline-check"
        ><input
          :checked="settings.jiraAskMove"
          type="checkbox"
          :disabled="busy || !settings.jiraWrite"
          @change="save({ jiraAskMove: checked($event) })"
        />Pytaj o przesunięcie zgłoszenia przy starcie pracy</label
      >
      <div class="planning-fields settings-fields">
        <label
          >Docelowy status przy starcie pracy<input
            :value="settings.jiraMoveStatus"
            maxlength="80"
            :disabled="busy || !settings.jiraWrite"
            @change="save({ jiraMoveStatus: text($event) })"
        /></label>
      </div>
      <p class="hint">
        Start pomiaru zadania ze zgłoszeniem w statusie „Do zrobienia” pyta o
        przesunięcie do statusu w toku. Przy kilku przejściach wybierane jest to
        o tej nazwie, a bez dopasowania pytanie pokazuje listę. Pomiar startuje
        od razu, a zgłoszenie zmienia się dopiero po kliknięciu „Przesuń”.
      </p>
      <div class="planning-fields settings-fields">
        <label
          >Prefiks nazwy gałęzi<input
            :value="settings.jiraBranchPrefix"
            maxlength="40"
            placeholder="feature/"
            :disabled="busy"
            @change="save({ jiraBranchPrefix: text($event) })"
        /></label>
      </div>
      <p class="hint">
        „Kopiuj nazwę gałęzi” na tablicy Jiry i w kafelku zadania daje np.
        {{ settings.jiraBranchPrefix }}WMS-123-krotki-tytul (małe litery, bez
        polskich znaków, do 50 znaków po prefiksie). Pusty prefiks — sama nazwa.
      </p>
      <label class="inline-check"
        ><input
          :checked="settings.jiraAutoImport"
          type="checkbox"
          :disabled="busy"
          @change="save({ jiraAutoImport: checked($event) })"
        />Dodawaj nowe zgłoszenia przypisane do mnie</label
      >
      <p class="hint">
        Co 5 minut, gdy konto Jiry jest połączone. Nowe zgłoszenie trafia do „Do
        zaplanowania” z powiadomieniem; obecne przypisania przy pierwszym
        sprawdzeniu nie są importowane. Status z Jiry widać na kafelku zadania.
      </p>
      <label class="settings-statuses"
        >Statusy oznaczające „Czekam na” (jeden w wierszu)<textarea
          :value="settings.waitingJiraStatuses.join('\n')"
          rows="5"
          :disabled="busy"
          @change="
            save({
              waitingJiraStatuses: (
                $event.target as HTMLTextAreaElement
              ).value.split('\n'),
            })
          "
        />
      </label>
      <p class="hint">
        Gdy zgłoszenie zadania wejdzie w taki status, kafelek proponuje
        oznaczenie „Czekam na”, a gdy z niego wyjdzie — odblokowanie. Nic nie
        zmienia się samo. Wielkość liter nie ma znaczenia.
      </p>
      <h3 id="jira-card-fields-heading">Pola Jiry na kafelku</h3>
      <div
        class="chosen-boards jira-card-fields"
        role="list"
        aria-labelledby="jira-card-fields-heading"
      >
        <span
          v-for="(id, i) in settings.jiraCardFields"
          :key="id"
          role="listitem"
          class="jira-card-field"
          >{{ fieldName(id) }}
          <button
            :aria-label="'Przesuń wyżej: ' + fieldName(id)"
            title="Wyżej"
            :disabled="busy || i === 0"
            @click="moveField(i, -1)"
          >
            ↑</button
          ><button
            :aria-label="'Przesuń niżej: ' + fieldName(id)"
            title="Niżej"
            :disabled="busy || i === settings.jiraCardFields.length - 1"
            @click="moveField(i, 1)"
          >
            ↓</button
          ><button
            :aria-label="'Usuń pole: ' + fieldName(id)"
            title="Usuń"
            :disabled="busy"
            @click="removeField(id)"
          >
            ×
          </button></span
        >
        <span v-if="!settings.jiraCardFields.length" class="hint"
          >Brak pól: kafelek pokazuje tylko klucz i status.</span
        >
      </div>
      <div class="connection-actions">
        <button
          class="secondary-button"
          :disabled="busy || fieldsBusy"
          :aria-expanded="pickingFields"
          @click="toggleFieldPicker()"
        >
          {{ pickingFields ? "Zwiń listę pól" : "Dodaj pole…" }}</button
        ><button
          class="secondary-button"
          :disabled="busy || fieldsBusy"
          @click="loadFields(true)"
        >
          Pobierz listę pól z Jiry</button
        ><span class="hint"
          >Wybrano {{ settings.jiraCardFields.length }} / {{ maxCardFields }}</span
        >
      </div>
      <p v-if="fieldsError" class="error" role="alert">{{ fieldsError }}</p>
      <p v-else-if="fieldsNotice" class="hint" role="status">
        {{ fieldsNotice }}
      </p>
      <div v-if="pickingFields" class="board-picker jira-field-picker">
        <input
          v-model="fieldQuery"
          aria-label="Szukaj pola"
          placeholder="Nazwa pola…"
          :disabled="busy || fieldsBusy"
        />
        <div class="board-options">
          <label v-for="f in visibleFields" :key="f.id"
            ><input
              type="checkbox"
              :checked="settings.jiraCardFields.includes(f.id)"
              :disabled="
                busy ||
                fieldsBusy ||
                (settings.jiraCardFields.length >= maxCardFields &&
                  !settings.jiraCardFields.includes(f.id))
              "
              @change="toggleField(f.id)"
            />{{ f.name }}
            <small>{{ f.custom ? f.id : "systemowe" }}</small></label
          >
          <p v-if="fieldsBusy" class="hint" role="status">Proszę czekać…</p>
          <p v-else-if="!visibleFields.length" class="hint">
            {{
              catalog.length
                ? "Brak pól pasujących do wyszukiwania."
                : "Lista pól nie jest pobrana."
            }}
          </p>
        </div>
      </div>
      <p class="hint">
        Wybrane pola (także niestandardowe) widać w zwiniętym kafelku zadania,
        na karcie tygodnia i na kartach w widoku Jira. Wartości odświeżają się
        przy pobraniu listy lub tablicy, co 5 minut dla zgłoszeń otwartych zadań
        (także przekazanych komuś innemu) i od razu po zmianie tej listy.
        Priorytet z Jiry zawsze ustawia pasmo priorytetu zadania (Highest i High
        → wysoki, Medium → normalny, Low i Lowest → niski); ręczna zmiana pasma
        zostaje do kolejnej zmiany priorytetu w Jirze.
      </p>
      <p class="hint">
        Konto Jiry (adres, e-mail, token API) i tablice ustawiasz w widoku Jira.
      </p>
      <button class="action-button" @click="openView({ view: 'jira' })">
        Połączenie z Jirą
      </button>
    </template>
    <template v-if="section === 'timesheet'">
      <h2>Timesheet (TS)</h2>
      <TimesheetSettings>
        <template #send>
          <h3>Wysyłka</h3>
          <label class="inline-check"
            ><input
              :checked="settings.tsSend"
              type="checkbox"
              :disabled="busy"
              @change="save({ tsSend: checked($event) })"
            />Wysyłaj raport TS</label
          >
          <p class="hint">
            Wysyłka zawsze wymaga potwierdzenia w podglądzie („Zapisz w TS”).
            Wyłączone chowa przyciski „Wyślij do TS” i blokuje zapis w TS także
            w aplikacji, nie tylko w interfejsie.
          </p>
        </template>
      </TimesheetSettings>
    </template>
    <template v-if="section === 'calendar'">
      <h2>Kalendarz</h2>
      <CalendarSettings />
      <label class="inline-check"
        ><input
          :checked="settings.calendarShowTitles"
          type="checkbox"
          :disabled="busy"
          @change="save({ calendarShowTitles: checked($event) })"
        />Pokazuj tytuły spotkań</label
      >
      <p class="hint">
        Wyłączone: zamiast tytułu widać „Spotkanie”, np. przy udostępnianiu
        ekranu. Spotkania są tylko do odczytu; czas spotkania dodajesz sam.
      </p>
    </template>
    <template v-if="section === 'notifications'">
      <h2>Powiadomienia</h2>
      <label class="inline-check"
        ><input
          :checked="settings.notifyDue"
          type="checkbox"
          :disabled="busy"
          @change="save({ notifyDue: checked($event) })"
        />Przypominaj o terminach na dziś i przekroczonych</label
      >
      <div class="planning-fields settings-fields">
        <label
          >Długi pomiar bez Stop (minuty, 0 wyłącza)<input
            :value="settings.notifyLongMinutes"
            type="number"
            min="0"
            max="1440"
            step="15"
            :disabled="busy"
            @change="save({ notifyLongMinutes: number($event) })"
        /></label>
        <label
          >Wieczorne „pomiar nadal trwa”<input
            :value="settings.notifyEvening"
            type="time"
            :disabled="busy || !settings.notifyEveningEnabled"
            @change="save({ notifyEvening: text($event) })"
        /></label>
      </div>
      <label class="inline-check"
        ><input
          :checked="settings.notifyEveningEnabled"
          type="checkbox"
          :disabled="busy"
          @change="save({ notifyEveningEnabled: checked($event) })"
        />Przypominaj wieczorem, że pomiar nadal trwa</label
      >
      <label class="inline-check"
        ><input
          :checked="settings.notifyTarget"
          type="checkbox"
          :disabled="busy"
          @change="save({ notifyTarget: checked($event) })"
        />Powiadom o osiągnięciu celu dnia</label
      >
      <label class="inline-check"
        ><input
          :checked="settings.notifyMorningReview"
          type="checkbox"
          :disabled="busy"
          @change="save({ notifyMorningReview: checked($event) })"
        />Przypominaj o porannym przeglądzie dnia</label
      >
      <label class="inline-check"
        ><input
          :checked="settings.notifyWeekReview"
          type="checkbox"
          :disabled="busy"
          @change="save({ notifyWeekReview: checked($event) })"
        />Przypominaj o przeglądzie tygodnia</label
      >
      <div class="planning-fields settings-fields">
        <label
          >Przegląd tygodnia o godzinie<input
            :value="settings.weekReviewTime"
            type="time"
            :disabled="busy || !settings.notifyWeekReview"
            @change="save({ weekReviewTime: text($event) })"
        /></label>
      </div>
      <div class="planning-fields settings-fields">
        <label
          >Przypomnij o „Czekam na” po (dni robocze, 0 wyłącza)<input
            :value="settings.waitingNotifyDays"
            type="number"
            min="0"
            max="30"
            step="1"
            :disabled="busy"
            @change="save({ waitingNotifyDays: number($event) })"
        /></label>
      </div>
      <p class="hint">
        Powiadomienia Windows: terminy przy starcie i o 09:00, długi pomiar raz
        na przedział, wieczór o wybranej godzinie, cel raz dziennie, przegląd
        dnia w dzień roboczy po początku dnia, dopóki nie klikniesz „Zaczynam
        dzień”, przegląd tygodnia raz w ostatni dzień roboczy tygodnia o
        wybranej godzinie, „Czekam na” raz na blokadę po wybranej liczbie dni
        roboczych (z „Odblokuj” i „Przypomnij jutro”). Każde pojawia się
        najwyżej raz, także po ponownym uruchomieniu. Potwierdzenie skrótu
        Start/Stop pokazuje się, gdy widget jest schowany.
      </p>
    </template>
    <template v-if="section === 'templates'">
      <h2>Szablony</h2>
      <p class="hint">
        Wpisz /nazwa w polu „Nowe zadanie” albo w szybkim przełączaniu, np.
        /review WMS-123. W tytule {klucz} to klucz Jiry, a {tytuł} to tytuł z
        linii albo zgłoszenia. Pola wpisane w linii (~, #, !) wygrywają z
        szablonem. Szablon zapiszesz też z rozwiniętego zadania.
      </p>
      <div class="template-list" aria-label="Szablony zadań">
        <TemplateEditor v-for="t in templates" :key="t.id" :template="t" />
        <p v-if="!templates.length && !addingTemplate" class="empty-detail">
          Brak szablonów.
        </p>
        <TemplateEditor
          v-if="addingTemplate"
          :template="null"
          @done="addingTemplate = false"
        />
      </div>
      <button
        v-if="!addingTemplate"
        class="action-button"
        @click="addingTemplate = true"
      >
        Nowy szablon
      </button>
    </template>
    <template v-if="section === 'shortcuts'">
      <h2>Skróty</h2>
      <p>
        Ctrl+K — szybkie przełączanie w aplikacji.<br />{{
          settings.shortcutQuick
        }}
        — szybkie przełączanie z dowolnego okna.<br />{{
          settings.shortcutToggle
        }}
        — Start/Stop z dowolnego okna: zatrzymuje pomiar albo wznawia ostatnio
        mierzone zadanie.
      </p>
      <p class="hint">
        Kliknij pole i naciśnij nową kombinację z Ctrl, Alt, Shift lub Win. Esc
        anuluje.
      </p>
      <ShortcutInput
        :model-value="settings.shortcutQuick"
        label="Szybkie przełączanie (globalnie)"
        :default-value="defaultSettings.shortcutQuick"
        :error="shortcutError('quick')"
        :disabled="busy"
        @update:model-value="saveShortcut('quick', $event)"
      />
      <ShortcutInput
        :model-value="settings.shortcutToggle"
        label="Start/Stop (globalnie)"
        :default-value="defaultSettings.shortcutToggle"
        :error="shortcutError('toggle')"
        :disabled="busy"
        @update:model-value="saveShortcut('toggle', $event)"
      />
    </template>
    <template v-if="section === 'startup'">
      <h2>Uruchamianie</h2>
      <label class="inline-check"
        ><input
          :checked="autostart.enabled"
          type="checkbox"
          :disabled="busy"
          @change="saveAutostart('enabled', $event)"
        />Uruchamiaj z Windows</label
      >
      <label class="inline-check"
        ><input
          :checked="autostart.hidden"
          type="checkbox"
          :disabled="busy || !autostart.enabled"
          @change="saveAutostart('hidden', $event)"
        />Uruchamiaj schowane do traya</label
      >
      <p class="hint">
        Przy starcie schowanym okno się nie pokazuje; otworzysz je ikoną w
        trayu. Wyłączenie wpisu w Menedżerze zadań też będzie tu widoczne.
      </p>
      <h3>Aktualizacje</h3>
      <label class="inline-check"
        ><input
          :checked="settings.autoUpdateCheck"
          type="checkbox"
          :disabled="busy"
          @change="save({ autoUpdateCheck: checked($event) })"
        />Sprawdzaj aktualizacje automatycznie</label
      >
      <p class="hint">
        Przy starcie i raz dziennie aplikacja pyta serwer wydań o nową wersję.
        Pobranie i instalacja zaczynają się dopiero po kliknięciu w banerze.
      </p>
      <div class="planning-actions">
        <button
          type="button"
          class="secondary-button"
          :disabled="updateBusy"
          @click="checkNow"
        >
          {{ updateBusy ? "Sprawdzanie…" : "Sprawdź teraz" }}
        </button>
      </div>
      <p v-if="updateMessage" class="hint" role="status">
        {{ updateMessage }}
      </p>
    </template>
    <small v-if="info.version">Todo Track {{ info.version }}</small>
  </section>
</template>
