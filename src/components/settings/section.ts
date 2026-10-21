import { ref, type Ref } from "vue";
import { readSettings, writeSettings, type AppSettings } from "../../settings";

/**
 * Stan sekcji Ustawień: własne `busy` i `error` oraz zapis zmiany ustawień.
 * `settings` to model z `SettingsPanel` (`v-model:settings`); po błędzie pole
 * wraca do zapisanej wartości.
 */
export function useSettingsSection(settings: Ref<AppSettings>) {
  const busy = ref(false);
  const error = ref("");
  async function save(patch: Partial<AppSettings>) {
    busy.value = true;
    error.value = "";
    try {
      settings.value = await writeSettings(patch);
    } catch (e) {
      error.value = message(e);
      // Pole wraca do zapisanej wartości.
      const saved = await readSettings().catch(() => settings.value);
      settings.value = { ...saved, workDays: [...saved.workDays] };
    } finally {
      busy.value = false;
    }
  }
  return { busy, error, save };
}

/** Treść błędu z wyjątku albo odrzucenia komendy. */
export const message = (e: unknown) => String(e instanceof Error ? e.message : e);
/** Stan pola wyboru ze zdarzenia `change`. */
export const checked = (e: Event) => (e.target as HTMLInputElement).checked;
/** Tekst pola ze zdarzenia `change`. */
export const text = (e: Event) => (e.target as HTMLInputElement).value;
/** Liczba z pola; puste pole to NaN (walidacja odrzuci zapis). */
export const number = (e: Event) => {
  const value = (e.target as HTMLInputElement).value;
  return value === "" ? NaN : Number(value);
};
