import { computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { desktop, type TsProject } from "./store";
import type { TsOption, TsSuggestion } from "./api/aiassign";
import * as aiSim from "./preview/ai";
import { useSettings } from "./target";

export * from "./api/aiassign";

/**
 * Podpowiedzi przypisań (A4) są dostępne: wybrany silnik (nie „off” ani „jeszcze
 * nie pytano”) i włączone „Podpowiedzi przypisań” (`aiAssignSuggest`).
 */
export function useAssignEnabled() {
  const settings = useSettings();
  return computed(
    () =>
      (settings.value.aiEngine === "claude" ||
        settings.value.aiEngine === "codex") &&
      settings.value.aiAssignSuggest,
  );
}

/** Lista zadań TS z projektami jako płaska lista dla asystenta. */
export function tsOptionsOf(projects: TsProject[]): TsOption[] {
  return projects.flatMap((p) =>
    p.tasks.map((t) => ({ id: t.id, name: t.name, project: p.name })),
  );
}

/**
 * `ai_suggest_ts_task`: podpowiedź zadania TS dla zadania (A4) z listy zadań TS
 * `options`; nic nie zapisuje (zapis dopiero po wyborze użytkownika). W podglądzie
 * odpowiada symulowany asystent (`preview/ai.ts`).
 */
export async function aiSuggestTsTask(
  taskId: number,
  day: string,
  options: TsOption[],
): Promise<TsSuggestion> {
  if (desktop) return invoke("ai_suggest_ts_task", { taskId, day, options });
  return aiSim.aiSuggestTsTask(taskId, day, options);
}
