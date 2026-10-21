/**
 * Symulowany TS podglądu w localStorage (`previewTsKey`) z regułami walidacji
 * i zapisu z `timesheet.rs`: zapis zastępuje cały dzień, `keep` dokleja wpisy
 * TS o zadaniach spoza wysyłanej listy.
 */
import {
  notifyTs,
  tsCheckDate,
  tsControl,
  tsDefaultApi,
  tsEntryRow,
  tsLoginRejected,
  tsMergeKeep,
  tsNormalizeApi,
  tsNotConnected,
  tsSendDisabled,
  tsSavedDiffers,
  tsSentOutcome,
  tsSortMap,
  tsValidTaskId,
  tsValidTaskName,
  tsValidateEntries,
  type TsEntry,
  type TsFilled,
  type TsMode,
  type TsProject,
  type TsProjectMapping,
  type TsSendResult,
  type TsSent,
  type TsSettings,
} from "../api/timesheet";
import { previewJson, previewSettingsKey } from "./storage";
import { read } from "./snapshot";
import { tsProjectKey } from "../timesheet";
/**
 * Symulowany TS podglądu. `failLogin` wymusza odrzucenie logowania; hasło „zle”
 * odrzuca zawsze. `tasks` nadpisuje przykładową listę zadań. `failReadBack`:
 * zapis przyjęty, odczyt po zapisie nieudany. `ignoreSave`: TS przyjmuje zapis,
 * ale dzień się nie zmienia (odczyt bez wysłanych wpisów). Nadgodziny wpisów
 * (`overtime`) są zapisywane i odczytywane jak godziny; `dropOvertime`: TS
 * zapisuje nadgodziny jako „0” (rozjazd po zapisie). `changeBeforeSave`: wpisy
 * dopisane w TS „przez kogoś innego” tuż przed zapisem (między otwarciem
 * podglądu a zapisem), jednorazowo. `failRead`: dni, których odczyt
 * (`ts_filled`) kończy się błędem.
 */
export const previewTsKey = "todo-track-preview-ts-v1";
interface PreviewTs {
  connected?: boolean;
  login?: string;
  apiUrl?: string;
  deviceId?: string;
  failLogin?: boolean;
  failReadBack?: boolean;
  failRead?: string[];
  ignoreSave?: boolean;
  dropOvertime?: boolean;
  changeBeforeSave?: Record<string, TsFilled[]>;
  tasks?: TsProject[];
  filled?: Record<string, TsFilled[]>;
  sent?: Record<string, { sentAt: number; hours: string }>;
  /** Domyślne zadania TS projektów (`ts_project_map`). */
  projectMap?: TsProjectMapping[];
}
export const previewTsTasks: readonly TsProject[] = Object.freeze([
  {
    name: "WMS",
    tasks: [
      { id: "1101", name: "Rozwój" },
      { id: "1102", name: "Błędy i wsparcie" },
      { id: "1103", name: "Code review" },
    ],
  },
  {
    name: "Wewnętrzne",
    tasks: [
      { id: "2201", name: "Spotkania" },
      { id: "2202", name: "Administracja" },
    ],
  },
]);
function previewTs(): PreviewTs {
  return previewJson(previewTsKey) as PreviewTs;
}
function savePreviewTs(state: PreviewTs) {
  localStorage.setItem(previewTsKey, JSON.stringify(state));
}
function previewTsSettings(): TsSettings {
  const state = previewTs();
  return {
    config: {
      apiUrl: state.apiUrl ?? tsDefaultApi,
      login: state.login ?? "",
      deviceId: state.deviceId ?? "",
      deviceName: "Todo Track",
    },
    connected: state.connected === true && !!state.login,
    sendEnabled: previewJson(previewSettingsKey).tsSend !== false,
  };
}
function previewTsConnected(): PreviewTs {
  const state = previewTs();
  if (!(state.connected === true && state.login)) throw new Error(tsNotConnected);
  return state;
}
export async function tsSettings(): Promise<TsSettings> {
  return previewTsSettings();
}
export async function tsConnect(
  apiUrl: string,
  login: string,
  password: string,
): Promise<TsSettings> {
  const name = login.trim();
  if (!name || [...name].length > 100 || tsControl.test(name))
    throw new Error("Podaj login TS (do 100 znaków).");
  if (!password || [...password].length > 1024)
    throw new Error("Podaj hasło TS.");
  const api = tsNormalizeApi(apiUrl);
  const state = previewTs();
  if (state.failLogin || password === "zle") throw new Error(tsLoginRejected);
  state.apiUrl = api;
  state.login = name;
  state.connected = true;
  if (!/^[0-9a-f]{32}$/.test(state.deviceId ?? ""))
    state.deviceId = Array.from(
      crypto.getRandomValues(new Uint8Array(16)),
      (b) => b.toString(16).padStart(2, "0"),
    ).join("");
  savePreviewTs(state);
  notifyTs();
  return previewTsSettings();
}
export async function tsDisconnect(): Promise<TsSettings> {
  const state = previewTs();
  state.connected = false;
  savePreviewTs(state);
  notifyTs();
  return previewTsSettings();
}
export async function tsTasks(date: string): Promise<TsProject[]> {
  tsCheckDate(date);
  const state = previewTsConnected();
  return structuredClone(state.tasks ?? (previewTsTasks as TsProject[]));
}
export async function tsFilled(date: string): Promise<TsFilled[]> {
  tsCheckDate(date);
  const state = previewTsConnected();
  if (state.failRead?.includes(date))
    throw new Error("TS nie odpowiada (symulowany błąd odczytu).");
  return structuredClone(state.filled?.[date] ?? []);
}
export async function tsSend(
  date: string,
  entries: TsEntry[],
  mode: TsMode,
): Promise<TsSendResult> {
  if (previewJson(previewSettingsKey).tsSend === false)
    throw new Error(tsSendDisabled);
  tsCheckDate(date);
  if (mode !== "replace" && mode !== "keep")
    throw new Error("Niepoprawny tryb zapisu w TS.");
  tsValidateEntries(entries);
  const state = previewTsConnected();
  // Zmiana w TS między otwarciem podglądu a zapisem (jednorazowa).
  const injected = state.changeBeforeSave?.[date];
  if (injected) {
    state.filled = {
      ...state.filled,
      [date]: [...(state.filled?.[date] ?? []), ...injected],
    };
    delete state.changeBeforeSave![date];
  }
  // Jak `ts_send`: `keep` dokleja wpisy z aktualnego dnia w TS.
  const current = state.filled?.[date] ?? [];
  const saved = mode === "keep" ? tsMergeKeep(entries, current) : entries;
  if (!state.ignoreSave)
    state.filled = {
      ...state.filled,
      [date]: saved.map(tsEntryRow).map((row) =>
        state.dropOvertime ? { ...row, overtime: "0" } : row,
      ),
    };
  savePreviewTs(state);
  const read = state.failReadBack ? null : (state.filled?.[date] ?? []);
  const { filled, hours, warning } = tsSentOutcome(saved, read);
  const sentAt = Date.now();
  state.sent = { ...state.sent, [date]: { sentAt, hours } };
  savePreviewTs(state);
  const result: TsSendResult = {
    filled: structuredClone(filled),
    sent: structuredClone(saved),
    differs: read !== null && tsSavedDiffers(saved, read),
    sentAt,
  };
  if (warning) result.warning = warning;
  notifyTs();
  return result;
}
export async function tsSent(day: string): Promise<TsSent | null> {
  tsCheckDate(day);
  const sent = previewTs().sent?.[day];
  return sent ? { day, ...sent } : null;
}
export async function tsProjectMap(): Promise<TsProjectMapping[]> {
  return tsSortMap(structuredClone(previewTs().projectMap ?? []));
}
export async function tsSetProjectMap(
  project: string,
  tsTaskId: string,
  tsTaskName: string,
): Promise<TsProjectMapping[]> {
  const name = project.trim();
  const id = tsTaskId.trim();
  const label = tsTaskName.trim();
  if (!name || [...name].length > 80 || tsControl.test(name))
    throw new Error("Niepoprawna nazwa projektu.");
  const state = previewTs();
  const key = tsProjectKey(name);
  const rest = (state.projectMap ?? []).filter(
    (m) => tsProjectKey(m.project) !== key,
  );
  if (id) {
    if (!tsValidTaskId(id))
      throw new Error("Niepoprawny identyfikator zadania TS.");
    if (!tsValidTaskName(label))
      throw new Error("Niepoprawna nazwa zadania TS.");
    rest.push({ project: name, tsTaskId: id, tsTaskName: label });
  }
  state.projectMap = rest;
  savePreviewTs(state);
  notifyTs();
  return tsSortMap(structuredClone(rest));
}
