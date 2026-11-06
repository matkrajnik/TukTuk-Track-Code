/**
 * Wspólny stan odblokowania modułu Workflows (PLAN_WDROZENIA.md, 4.1.1).
 * Nawigacja, Ustawienia i widoki czytają ten sam `ref`. Odblokowanie
 * ustawia też jednorazowy znacznik animacji odsłonięcia (4.1.2), który
 * nie wraca po restarcie ani odświeżeniu widoku.
 * Stan w interfejsie nie jest kontrolą dostępu: sprawdza ją backend.
 */
import { ref } from "vue";
import { unlockWorkflows, workflowAccess } from "./workflows";

export const workflowsUnlocked = ref(false);
/**
 * Przebieg do otwarcia w monitorze po starcie spoza widoku Workflows (np. z karty
 * zadania); `App.vue` przełącza widok i zeruje wartość.
 */
export const openRunRequest = ref<number | null>(null);
/** Animacja odsłonięcia czeka na odegranie (tylko po udanym odblokowaniu). */
export const workflowsReveal = ref(false);

let loading: Promise<void> | null = null;

/** Odczyt stanu raz na sesję okna; błąd zostawia moduł ukryty. */
export function loadWorkflowAccess(): Promise<void> {
  loading ??= workflowAccess()
    .then((access) => {
      workflowsUnlocked.value = access.unlocked;
    })
    .catch((e) => {
      // Bez stanu moduł zostaje ukryty; reszta aplikacji działa normalnie.
      console.warn("Nie udało się odczytać dostępu do Workflows:", e);
      loading = null;
    });
  return loading;
}

/**
 * Sprawdza klucz w backendzie i dopiero po trwałym zapisie uprawnienia
 * zgłasza sukces. Klucz nie trafia do logów ani komunikatów.
 */
export async function unlockWithKey(key: string): Promise<void> {
  const access = await unlockWorkflows(key);
  workflowsUnlocked.value = access.unlocked;
  if (access.unlocked) workflowsReveal.value = true;
}

/** Koniec (albo pominięcie) animacji odsłonięcia. */
export function finishReveal(): void {
  workflowsReveal.value = false;
}
