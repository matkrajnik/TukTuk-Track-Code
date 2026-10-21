import { test, expect } from "@playwright/test";
import {
  isRecurring,
  lastWorkDay,
  normalizeRule,
  occurs,
  ruleText,
  type RepeatRule,
} from "../src/recurrence";
import { openPreview, stored } from "./fixtures/seed";

/**
 * P3: reguły cykli. Testy jednostkowe odpowiednika w podglądzie
 * (`src/recurrence.ts`) na tych samych przypadkach co testy Rust
 * (`recurrence.rs`), potem wybór reguły w rozwiniętym kafelku.
 */
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });

const work = { workDays: [1, 2, 3, 4, 5], holidaysInTarget: true };
const rule = (r: Partial<RepeatRule>): RepeatRule => ({
  kind: "",
  interval: 0,
  day: 0,
  nth: 0,
  weekday: 0,
  ...r,
});
/** Dni wystąpień reguły od `from` do `to` włącznie. */
function days(
  details: { repeatDays?: number[]; repeatRule?: RepeatRule },
  from: string,
  to: string,
  s = work,
) {
  const out: string[] = [];
  for (
    let t = Date.parse(from + "T00:00:00Z");
    t <= Date.parse(to + "T00:00:00Z");
    t += 86_400_000
  ) {
    const day = new Date(t).toISOString().slice(0, 10);
    if (occurs(details, from, day, s)) out.push(day);
  }
  return out;
}

test("recurrence: day of month uses the last day of shorter months across years", () => {
  const day31 = { repeatRule: rule({ kind: "monthDay", day: 31 }) };
  expect(days(day31, "2025-11-15", "2026-04-30")).toEqual([
    "2025-11-30",
    "2025-12-31",
    "2026-01-31",
    "2026-02-28",
    "2026-03-31",
    "2026-04-30",
  ]);
  expect(
    days(
      { repeatRule: rule({ kind: "monthDay", day: 30 }) },
      "2028-01-01",
      "2028-03-31",
    ),
  ).toEqual(["2028-01-30", "2028-02-29", "2028-03-30"]);
});

test("recurrence: last work day skips weekends and holidays from settings", () => {
  const last = { repeatRule: rule({ kind: "lastWorkday" }) };
  expect(days(last, "2026-10-01", "2027-02-28")).toEqual([
    "2026-10-30",
    "2026-11-30",
    "2026-12-31",
    "2027-01-29",
    "2027-02-26",
  ]);
  // Boże Ciało w czwartek 31.05.2029.
  expect(lastWorkDay(work, 2029, 5)).toBe("2029-05-30");
  expect(lastWorkDay({ ...work, holidaysInTarget: false }, 2029, 5)).toBe(
    "2029-05-31",
  );
  expect(lastWorkDay({ ...work, workDays: [1, 2, 3, 4, 5, 6] }, 2026, 10)).toBe(
    "2026-10-31",
  );
  expect(lastWorkDay({ ...work, workDays: [] }, 2026, 10)).toBeNull();
});

test("recurrence: n-th and last weekday of the month across years", () => {
  expect(
    days(
      { repeatRule: rule({ kind: "nthWeekday", nth: 1, weekday: 1 }) },
      "2026-11-15",
      "2027-02-28",
    ),
  ).toEqual(["2026-12-07", "2027-01-04", "2027-02-01"]);
  expect(
    days(
      { repeatRule: rule({ kind: "nthWeekday", nth: -1, weekday: 5 }) },
      "2026-12-01",
      "2027-02-28",
    ),
  ).toEqual(["2026-12-25", "2027-01-29", "2027-02-26"]);
});

test("recurrence: every N weeks counts from the week of the start and old repeatDays stay weekly", () => {
  const every2 = {
    repeatDays: [1, 3],
    repeatRule: rule({ kind: "weeks", interval: 2 }),
  };
  expect(days(every2, "2026-12-23", "2027-01-20")).toEqual([
    "2026-12-23",
    "2027-01-04",
    "2027-01-06",
    "2027-01-18",
    "2027-01-20",
  ]);
  // Zapis sprzed 1.7: bez `repeatRule`.
  const old = { repeatDays: [1, 3, 5] };
  expect(isRecurring(old)).toBe(true);
  expect(days(old, "2026-12-28", "2027-01-08")).toEqual([
    "2026-12-28",
    "2026-12-30",
    "2027-01-01",
    "2027-01-04",
    "2027-01-06",
    "2027-01-08",
  ]);
  expect(isRecurring({ repeatDays: [] })).toBe(false);
  expect(isRecurring({ repeatRule: rule({ kind: "lastWorkday" }) })).toBe(true);
});

test("recurrence: rules are normalized and described like in Rust", () => {
  expect(normalizeRule(rule({ kind: "monthDay", day: 31, interval: 5 }))).toEqual(
    rule({ kind: "monthDay", day: 31 }),
  );
  for (const bad of [
    rule({ kind: "monthDay", day: 32 }),
    rule({ kind: "weeks", interval: 9 }),
    rule({ kind: "nthWeekday", nth: 5, weekday: 1 }),
    rule({ kind: "nthWeekday", nth: 1, weekday: 8 }),
    { ...rule({}), kind: "yearly" } as unknown as RepeatRule,
  ])
    expect(normalizeRule(bad)).toBeNull();
  expect(ruleText({ repeatRule: rule({ kind: "nthWeekday", nth: -1, weekday: 5 }) })).toBe(
    "ostatni piątek miesiąca",
  );
  expect(ruleText({ repeatRule: rule({ kind: "weeks", interval: 2 }) })).toBe(
    "co 2 tygodnie",
  );
});

test("recurrence: monthly rule chosen in the tile creates dated copies", async ({
  page,
}) => {
  await openPreview(page, {
    now: "2026-03-02T12:00:00+01:00",
    data: { tasks: [], sessions: [] },
  });
  await page.getByLabel("Nowe zadanie", { exact: true }).fill("Faktury");
  await page.getByRole("button", { name: "Dodaj zadanie", exact: true }).click();
  await page
    .getByRole("button", { name: "Przedziały: Faktury", exact: true })
    .click();
  await page.getByLabel("Rodzaj cyklu").selectOption("monthDay");
  await page.getByLabel("Początek cyklu").fill("2026-01-01");
  // Reguła miesięczna nie używa dni tygodnia.
  await expect(page.getByLabel("Pon", { exact: true })).toHaveCount(0);
  await page.getByLabel("Dzień miesiąca").fill("31");
  await expect(page.getByText("31. dnia miesiąca")).toBeVisible();
  await page.getByRole("button", { name: "Zapisz plan i notatki" }).click();
  await page.reload();
  await expect(page.locator(".task-card")).toHaveCount(3);
  const data = await stored(page);
  expect(
    data.tasks.map((t: any) => t.details.plannedDay).sort(),
  ).toEqual(["2026-01-01", "2026-01-31", "2026-02-28"]);
  const copy = data.tasks.find(
    (t: any) => t.details.plannedDay === "2026-02-28",
  );
  expect(copy.details.repeatRule.kind).toBe("");
  // N-ty dzień tygodnia: wybór w kafelku i opis reguły.
  await page
    .getByRole("button", { name: "Przedziały: Faktury" })
    .first()
    .click();
  const tile = page.locator(".planning-form").first();
  await tile.getByLabel("Rodzaj cyklu").selectOption("nthWeekday");
  await tile.getByLabel("Który w miesiącu").selectOption("-1");
  await tile.getByLabel("Dzień tygodnia cyklu").selectOption("5");
  await expect(tile.getByText("ostatni piątek miesiąca")).toBeVisible();
});
