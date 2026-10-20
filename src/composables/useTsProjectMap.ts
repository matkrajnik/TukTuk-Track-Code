import { readonly, ref } from "vue";
import {
  onTsChanged,
  tsProjectMap,
  tsSetProjectMap,
  type TsProjectMapping,
} from "../store";

/**
 * Domyślne zadania TS projektów (`ts_project_map`), wspólne dla wszystkich
 * kafelków: wczytane raz przy pierwszym użyciu, podmieniane po każdej zmianie.
 * Błąd odczytu zostawia pustą listę (chip „TS: …” po prostu się nie pokazuje).
 */
const map = ref<TsProjectMapping[]>([]);
let loading: Promise<void> | null = null;

export function loadTsProjectMap(force = false): Promise<void> {
  if (!loading || force)
    loading = tsProjectMap()
      .then((list) => {
        map.value = list;
      })
      .catch(() => {
        loading = null;
      });
  return loading;
}

/** Ustawia (puste `tsTaskId` usuwa) domyślne zadanie TS projektu. */
export async function setTsProjectMap(
  project: string,
  tsTaskId: string,
  tsTaskName: string,
) {
  map.value = await tsSetProjectMap(project, tsTaskId, tsTaskName);
}

let listening = false;

export function useTsProjectMap() {
  // Jeden nasłuch na czas działania: mapowania zmienione w Ustawieniach,
  // w innym oknie albo w symulowanym TS podglądu.
  if (!listening) {
    listening = true;
    onTsChanged(() => void loadTsProjectMap(true));
  }
  void loadTsProjectMap();
  return readonly(map);
}
