<script setup lang="ts">
/** Ustawienia → Jira: zapisy, przesuwanie przy starcie, gałęzie i pola na kafelku. */
import LegacyReconnect from "./LegacyReconnect.vue";
import { computed, onMounted, ref } from "vue";
import { Download, Plus } from "lucide-vue-next";
import { openView } from "../../navigation";
import { maxCardFields, type AppSettings } from "../../settings";
import {
  jiraFieldName,
  jiraFields,
  jiraRefreshFields,
  type JiraFieldInfo,
} from "../../store";
import { checked, message, text, useSettingsSection } from "./section";
import JiraWatchLine from "../jira/JiraWatchLine.vue";
const settings = defineModel<AppSettings>("settings", { required: true });
const { busy, error, save } = useSettingsSection(settings);

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
    fieldsError.value = message(e);
  } finally {
    fieldsBusy.value = false;
  }
}
// Nazwy wybranych pól (także niestandardowych) z pamięci podręcznej listy pól
// po pokazaniu sekcji; błąd (np. konto niepołączone) nic nie pokazuje.
onMounted(async () => {
  try {
    const list = await jiraFields(false);
    if (!catalog.value.length) catalog.value = list;
  } catch {
    /* identyfikatory zamiast nazw do czasu pobrania listy */
  }
});
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
    fieldsError.value = `Zapisano, ale nie udało się odświeżyć wartości: ${message(e)}`;
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
</script>
<template>
  <p v-if="error" class="error" role="alert">{{ error }}</p>
  <h2>Jira</h2>
  <LegacyReconnect kind="jira" />
  <label class="inline-check"
    ><input
      :checked="settings.jiraWrite"
      type="checkbox"
      :disabled="busy"
      @change="save({ jiraWrite: checked($event) })"
    />Pozwól aplikacji zmieniać zgłoszenia w Jirze</label
  >
  <p class="hint">
    Wyłączone blokuje każdą zmianę w Jirze z TukTuk Track (przesuwanie
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
  <JiraWatchLine />
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
      <Plus v-if="!pickingFields" :size="14" />{{
        pickingFields ? "Zwiń listę pól" : "Dodaj pole…"
      }}</button
    ><button
      class="secondary-button"
      :disabled="busy || fieldsBusy"
      @click="loadFields(true)"
    >
      <Download :size="14" />Pobierz listę pól z Jiry</button
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
