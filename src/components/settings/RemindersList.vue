<script setup lang="ts">
/**
 * „Moje przypomnienia” (C3) w Ustawienia → Powiadomienia: nadchodzące według
 * terminu, potem wyłączone i wykonane; dodawanie, edycja, włączanie i
 * wyłączanie oraz usuwanie (po potwierdzeniu).
 */
import { onMounted, onUnmounted, ref } from "vue";
import { BellPlus, Pencil, Trash2 } from "lucide-vue-next";
import ConfirmDialog from "../ConfirmDialog.vue";
import ReminderForm from "../ReminderForm.vue";
import {
  atFromMs,
  effectiveAt,
  onRemindersChanged,
  reminderDelete,
  reminderList,
  reminderUpdate,
  repeatText,
  whenLabel,
  type Reminder,
} from "../../remindersapi";
import { dayKey } from "../../time";
import { message } from "./section";

const list = ref<Reminder[]>([]);
const error = ref("");
const busy = ref(false);
/** Formularz: `false` zamknięty, `null` nowe, przypomnienie = edycja. */
const form = ref<Reminder | null | false>(false);
const removing = ref<Reminder | null>(null);
const today = ref(dayKey(Date.now()));

async function load() {
  try {
    list.value = await reminderList();
    today.value = dayKey(Date.now());
  } catch (e) {
    error.value = message(e);
  }
}
let stop: (() => void) | undefined;
let alive = true;
onMounted(async () => {
  void load();
  const off = await onRemindersChanged(() => void load()).catch(
    () => undefined,
  );
  if (!off) return;
  if (alive) stop = off;
  else off();
});
onUnmounted(() => {
  alive = false;
  stop?.();
});

/** Jednorazowe z minionym terminem: włączenie wymaga nowego terminu. */
const pastOneOff = (r: Reminder) =>
  !r.enabled && r.repeat === "none" && r.at < atFromMs(Date.now());
/** Włącza albo wyłącza przypomnienie (pełna walidacja jak przy edycji). */
async function toggle(r: Reminder, event: Event) {
  if (busy.value) return;
  const box = event.target as HTMLInputElement;
  // Termin minął: walidacja i tak by odrzuciła włączenie, więc otwiera się
  // edycja (zapis włącza jednorazowe przypomnienie z nowym terminem).
  if (pastOneOff(r)) {
    box.checked = false;
    error.value = "";
    form.value = r;
    return;
  }
  busy.value = true;
  error.value = "";
  try {
    await reminderUpdate(r.id, {
      text: r.text,
      at: r.at,
      repeat: r.repeat,
      weekdays: r.weekdays,
      taskId: r.taskId,
      enabled: !r.enabled,
    });
    await load();
  } catch (e) {
    error.value = message(e);
    // Pole wraca do zapisanego stanu (lista się nie zmieniła, więc sam Vue go nie cofnie).
    box.checked = r.enabled;
    await load();
  } finally {
    busy.value = false;
  }
}
async function remove() {
  const r = removing.value;
  if (!r || busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    await reminderDelete(r.id);
    removing.value = null;
    await load();
  } catch (e) {
    error.value = message(e);
  } finally {
    busy.value = false;
  }
}
const status = (r: Reminder) =>
  r.enabled
    ? whenLabel(effectiveAt(r), today.value)
    : r.repeat === "none" && r.lastFiredAt
      ? "wykonane"
      : "wyłączone";
</script>

<template>
  <section class="my-reminders" aria-label="Moje przypomnienia">
    <div class="my-reminders-head">
      <h3>Moje przypomnienia</h3>
      <button type="button" class="today-button" @click="form = null">
        <BellPlus :size="14" />Dodaj przypomnienie
      </button>
    </div>
    <p v-if="error" class="error" role="alert">{{ error }}</p>
    <ul v-if="list.length" class="reminder-list">
      <li
        v-for="r in list"
        :key="r.id"
        :class="{ off: !r.enabled }"
        :data-reminder="r.id"
      >
        <input
          type="checkbox"
          :checked="r.enabled"
          :disabled="busy"
          :aria-label="'Włączone: ' + r.text"
          :title="
            pastOneOff(r) ? 'Termin minął — włączenie otworzy edycję' : undefined
          "
          @change="toggle(r, $event)"
        />
        <span class="reminder-main"
          ><span class="reminder-text">{{ r.text }}</span>
          <small class="reminder-meta"
            >{{ repeatText(r)
            }}<template v-if="r.taskTitle"> · {{ r.taskTitle }}</template></small
          ></span
        >
        <span class="reminder-when mono">{{ status(r) }}</span>
        <button
          type="button"
          class="icon-button"
          :aria-label="'Edytuj przypomnienie: ' + r.text"
          title="Edytuj"
          :disabled="busy"
          @click="form = r"
        >
          <Pencil :size="14" />
        </button>
        <button
          type="button"
          class="icon-button"
          :aria-label="'Usuń przypomnienie: ' + r.text"
          title="Usuń"
          :disabled="busy"
          @click="removing = r"
        >
          <Trash2 :size="14" />
        </button>
      </li>
    </ul>
    <p v-else class="hint">
      Brak przypomnień. Dodaj własne na datę i godzinę, także powtarzalne.
    </p>
    <ReminderForm
      v-if="form !== false"
      :reminder="form"
      @close="form = false"
      @saved="
        form = false;
        load();
      "
    />
    <ConfirmDialog
      v-if="removing"
      title="Usunąć przypomnienie?"
      confirm-label="Usuń"
      :busy="busy"
      :error="error"
      @close="removing = null"
      @confirm="remove"
    >
      <p>„{{ removing.text }}” nie wystrzeli.</p>
    </ConfirmDialog>
  </section>
</template>
