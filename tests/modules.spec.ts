import { test, expect } from "@playwright/test";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import * as store from "../src/store";
import * as jiraRules from "../src/api/jira";
import * as redmineRules from "../src/api/redmine";
import * as tsRules from "../src/api/timesheet";
import * as taskRules from "../src/api/task";
import * as calendarRules from "../src/api/calendar";
import { previewJiraKey, previewWorkflow } from "../src/preview/jira-state";
import { previewRedmineKey, previewRedmineIssues } from "../src/preview/redmine";
import { previewTsKey, previewTsTasks } from "../src/preview/timesheet";
import { calendarKey } from "../src/preview/calendar";

// Podział `store.ts` (B10): reguły w `src/api/`, symulatory podglądu w
// `src/preview/`, a `store.ts` dalej eksportuje wszystko pod starymi nazwami.

const src = fileURLToPath(new URL("../src", import.meta.url));
const sources = (dir: string) =>
  readdirSync(join(src, dir))
    .filter((f) => f.endsWith(".ts"))
    .map((f) => ({ file: `${dir}/${f}`, text: readFileSync(join(src, dir, f), "utf8") }));

test("moduły api/ i preview/ nie importują store.ts ani Tauri", () => {
  for (const { file, text } of [...sources("api"), ...sources("preview")]) {
    expect(text, file).not.toMatch(/from "\.\.\/store"/);
    expect(text, file).not.toMatch(/@tauri-apps\//);
  }
  // Reguły (api/) nie sięgają do symulatorów.
  for (const { file, text } of sources("api"))
    expect(text, file).not.toMatch(/from "\.\.\/preview\//);
});

test("store.ts eksportuje reguły i dane podglądu pod dotychczasowymi nazwami", () => {
  for (const rules of [jiraRules, redmineRules, tsRules, taskRules, calendarRules])
    for (const [name, value] of Object.entries(rules))
      expect((store as Record<string, unknown>)[name], name).toBe(value);
  expect(store.previewJiraKey).toBe(previewJiraKey);
  expect(store.previewWorkflow).toBe(previewWorkflow);
  expect(store.previewRedmineKey).toBe(previewRedmineKey);
  expect(store.previewRedmineIssues).toBe(previewRedmineIssues);
  expect(store.previewTsKey).toBe(previewTsKey);
  expect(store.previewTsTasks).toBe(previewTsTasks);
  expect(store.calendarKey).toBe(calendarKey);
});

test("store.ts jest cienką warstwą: bez bezpośredniego localStorage", () => {
  const text = readFileSync(join(src, "store.ts"), "utf8");
  expect(text).not.toMatch(/localStorage\./);
  expect(text.split("\n").length).toBeLessThan(1200);
});
