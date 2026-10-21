import { test, expect } from "./fixtures/test";
import {
  dueChip,
  issueLabel,
  issueRef,
  redmineKey,
  statusTitle,
  validRedmineId,
} from "../src/issue";

// Testy jednostkowe wspólnego opisu zgłoszenia (PLAN_1.9.md, R2), bez przeglądarki.

test("issueRef: Jira task gives key, url, status and label", () => {
  const ref = issueRef({
    jiraKey: "WMS-12",
    jiraUrl: "https://x.atlassian.net/browse/WMS-12",
    jiraStatus: " W toku ",
  });
  expect(ref).toEqual({
    system: "jira",
    key: "WMS-12",
    url: "https://x.atlassian.net/browse/WMS-12",
    status: "W toku",
    label: "Jira",
  });
  expect(statusTitle(ref!)).toBe("Status w Jirze: W toku");
  // Bez adresu i statusu: puste teksty, nie `null`.
  expect(issueRef({ jiraKey: "WMS-1" })).toEqual({
    system: "jira",
    key: "WMS-1",
    url: "",
    status: "",
    label: "Jira",
  });
});

test("issueRef: Redmine task gives #number, url, status and label", () => {
  const ref = issueRef({
    redmineId: 1234,
    redmineUrl: "https://redmine.example.pl/issues/1234",
    redmineStatus: "Feedback",
  });
  expect(ref).toEqual({
    system: "redmine",
    key: "#1234",
    url: "https://redmine.example.pl/issues/1234",
    status: "Feedback",
    label: "Redmine",
  });
  expect(statusTitle(ref!)).toBe("Status w Redmine: Feedback");
  expect(issueLabel("redmine")).toBe("Redmine");
  expect(issueLabel("jira")).toBe("Jira");
  expect(redmineKey(7)).toBe("#7");
});

test("issueRef: no issue, empty fields and invalid numbers give null", () => {
  expect(issueRef(null)).toBeNull();
  expect(issueRef(undefined)).toBeNull();
  expect(issueRef({})).toBeNull();
  expect(issueRef({ jiraKey: "  ", redmineId: null })).toBeNull();
  for (const id of [0, -1, 1.5, NaN, 12_345_678_901]) {
    expect(issueRef({ redmineId: id })).toBeNull();
    expect(validRedmineId(id)).toBe(false);
  }
  expect(validRedmineId(1)).toBe(true);
  expect(validRedmineId("1" as unknown)).toBe(false);
});

test("issueRef: task with both fields shows Jira and warns in the console", () => {
  const warnings: string[] = [];
  const original = console.warn;
  console.warn = (message: string) => warnings.push(String(message));
  try {
    const ref = issueRef({
      jiraKey: "WMS-12",
      jiraStatus: "W toku",
      redmineId: 1234,
      redmineStatus: "New",
    });
    expect(ref?.system).toBe("jira");
    expect(ref?.key).toBe("WMS-12");
    expect(ref?.status).toBe("W toku");
    expect(warnings).toHaveLength(1);
    expect(warnings[0]).toContain("WMS-12");
    expect(warnings[0]).toContain("#1234");
  } finally {
    console.warn = original;
  }
});

test("dueChip: Redmine due date as DD.MM, empty otherwise", () => {
  expect(dueChip("2026-10-30")).toBe("30.10");
  expect(dueChip(" 2026-01-05 ")).toBe("05.01");
  expect(dueChip("")).toBe("");
  expect(dueChip(null)).toBe("");
  expect(dueChip("30.10.2026")).toBe("");
});
