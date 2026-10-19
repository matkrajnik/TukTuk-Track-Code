import { test, expect } from "@playwright/test";
import {
  tsHoursTotal,
  tsMergeKeep,
  tsNormalizeApi,
  tsValidateEntries,
  tsValidHours,
  type TsEntry,
} from "../src/store";

// Testy jednostkowe reguł symulowanego TS (te same co w src-tauri/src/timesheet.rs).
const entry = (taskId: string, hours: string, description = ""): TsEntry => ({
  taskId,
  hours,
  overtime: "0",
  description,
});

test("ts: hours format like timesheet::valid_hours", () => {
  for (const ok of ["1", "1,5", "1.25", "0", "12,00"])
    expect(tsValidHours(ok), ok).toBe(true);
  for (const bad of ["1,375", "abc", "", ",5", "1,", "-1", "1e2", "1 ,5"])
    expect(tsValidHours(bad), bad).toBe(false);
});

test("ts: entry validation like timesheet::validate_entries", () => {
  expect(() => tsValidateEntries([entry("101", "1,37", "Opis; ąę")])).not.toThrow();
  expect(() => tsValidateEntries([])).toThrow("Brak wpisów");
  expect(() => tsValidateEntries([entry("", "1")])).toThrow("identyfikator");
  expect(() => tsValidateEntries([entry("1".repeat(21), "1")])).toThrow(
    "identyfikator",
  );
  expect(() => tsValidateEntries([entry("101", "1,375")])).toThrow("Godziny");
  expect(() => tsValidateEntries([entry("101", "1", "a\nb")])).toThrow(
    "sterujących",
  );
  expect(() => tsValidateEntries([entry("101", "1", "ż".repeat(2001))])).toThrow(
    "2000",
  );
  const many = Array.from({ length: 51 }, (_, i) => entry(String(i), "1"));
  expect(() => tsValidateEntries(many)).toThrow("50");
  expect(() => tsValidateEntries(many.slice(0, 50))).not.toThrow();
});

test("ts: keep appends other TS entries after ours", () => {
  const filled = [
    { id: "101", hours: "1,5", overtime: "0", description: "Stary" },
    { id: "205", hours: "0.25", overtime: "0", description: "Daily" },
  ];
  expect(tsMergeKeep([entry("205", "1,00", "Nowy")], filled)).toEqual([
    entry("205", "1,00", "Nowy"),
    { taskId: "101", hours: "1,5", overtime: "0", description: "Stary" },
  ]);
  expect(tsMergeKeep([entry("1", "1")], [])).toEqual([entry("1", "1")]);
  expect(tsHoursTotal(filled)).toBe("1,75");
  expect(tsHoursTotal([])).toBe("0,00");
});

test("ts: api address like timesheet::normalize_api", () => {
  expect(tsNormalizeApi(" https://api.ts.ideo.pl/ ")).toBe(
    "https://api.ts.ideo.pl",
  );
  for (const bad of [
    "http://api.ts.ideo.pl",
    "https://api.ts.ideo.pl/api",
    "https://api.ts.ideo.pl/?a=1",
    "https://user:pass@api.ts.ideo.pl",
    "api.ts.ideo.pl",
  ])
    expect(() => tsNormalizeApi(bad), bad).toThrow("adres API");
});
