import type { Snapshot } from "./store";
import { clock, dayBounds } from "./time";
import { redmineKey, validRedmineId } from "./issue";

/**
 * Eksport CSV w trybie podglądu: ten sam układ co `src-tauri/src/export.rs`
 * (UTF-8 z BOM, średnik, przecinek dziesiętny, CRLF, jeden wiersz na część
 * przedziału w dniu; trwający pomiar do chwili eksportu z dopiskiem „trwa”).
 */
export interface ExportRow {
  day: string;
  start: number;
  end: number;
  toMidnight: boolean;
  ms: number;
  running: boolean;
  taskId: number;
  task: string;
  jiraKey: string;
  project: string;
  note: string;
  /** Zgłoszenie Redmine jako `#1234` (ostatnia kolumna; „Klucz Jiry” bez zmian). */
  redmine: string;
}

/** Znacznik kolejności bajtów: polski Excel czyta wtedy plik jako UTF-8. */
export const BOM = String.fromCharCode(0xfeff);

export const exportHeaders = [
  "Data",
  "Start",
  "Koniec",
  "Minuty",
  "Godziny",
  "Zadanie",
  "Klucz Jiry",
  "Projekt",
  "Notatka dnia",
  "Uwagi",
  "Redmine",
];

export function exportRows(
  data: Snapshot,
  days: string[],
  now: number,
): ExportRow[] {
  const tasks = new Map(data.tasks.map((t) => [t.id, t]));
  const rows: ExportRow[] = [];
  for (const day of [...new Set(days)].sort()) {
    const [from, to] = dayBounds(day);
    for (const s of data.sessions) {
      const task = tasks.get(s.taskId);
      if (!task) continue;
      const until = s.endedAt ?? now;
      const start = Math.max(s.startedAt, from);
      const end = Math.min(until, to);
      if (end <= start) continue;
      rows.push({
        day,
        start,
        end,
        toMidnight: end === to,
        ms: end - start,
        running: s.endedAt === null && end === until,
        taskId: task.id,
        task: task.title,
        jiraKey: task.jiraKey ?? "",
        project: task.details?.project?.trim() ?? "",
        note: task.details?.notes?.[day]?.trim() ?? "",
        redmine: validRedmineId(task.redmineId) ? redmineKey(task.redmineId) : "",
      });
    }
  }
  return rows.sort((a, b) =>
    a.day !== b.day
      ? a.day < b.day
        ? -1
        : 1
      : a.start - b.start || a.taskId - b.taskId,
  );
}

const field = (value: string) => {
  const safe = /^[=+\-@]/.test(value) ? `'${value}` : value;
  return /[;"\r\n]/.test(safe) ? `"${safe.replace(/"/g, '""')}"` : safe;
};
export const exportMinutes = (ms: number) => Math.floor((ms + 30_000) / 60_000);
export function exportHours(ms: number) {
  const hundredths = Math.floor((ms + 18_000) / 36_000);
  return `${Math.floor(hundredths / 100)},${String(hundredths % 100).padStart(2, "0")}`;
}

export function exportCsv(rows: ExportRow[]): string {
  const lines = [exportHeaders.join(";")];
  for (const row of rows)
    lines.push(
      [
        row.day,
        clock(row.start),
        row.toMidnight ? "24:00" : clock(row.end),
        String(exportMinutes(row.ms)),
        exportHours(row.ms),
        field(row.task),
        field(row.jiraKey),
        field(row.project),
        field(row.note),
        row.running ? "trwa" : "",
        field(row.redmine),
      ].join(";"),
    );
  return BOM + lines.map((line) => line + "\r\n").join("");
}
