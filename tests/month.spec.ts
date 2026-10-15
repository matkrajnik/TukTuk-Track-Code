import { readFileSync } from "node:fs";
import { test, expect, type Page } from "@playwright/test";
import { easter, holidayName, holidays } from "../src/holidays";

test.use({ timezoneId: "Europe/Warsaw" });

// Ta sama lista, z którą porównuje się src-tauri/src/holidays.rs.
const reference: [string, string][] = JSON.parse(
  readFileSync(
    new URL("./fixtures/holidays-pl-2024-2030.json", import.meta.url),
    "utf8",
  ),
);

test("holidays: 2024–2030 match the reference list shared with Rust", () => {
  const actual = [2024, 2025, 2026, 2027, 2028, 2029, 2030].flatMap((year) =>
    holidays(year).map((h) => [h.date, h.name]),
  );
  expect(actual).toEqual(reference);
  expect(easter(2285)).toEqual([3, 22]);
  expect(easter(2038)).toEqual([4, 25]);
  expect(holidayName("2024-12-24")).toBeNull();
  expect(holidayName("2025-12-24")).toBe("Wigilia");
  expect(holidayName("2026-06-04")).toBe("Boże Ciało");
});

const key = "todo-track-preview-v1";
const settingsKey = "todo-track-preview-settings-v1";

const tasks = [
  {
    id: 1,
    title: "Analiza importu",
    done: false,
    createdAt: 0,
    jiraKey: "WMS-12",
    details: { project: "WMS" },
  },
  {
    id: 2,
    title: "Spotkanie zespołu",
    done: false,
    createdAt: 0,
    details: { project: "Zespół" },
  },
  { id: 3, title: "Dyżur nocny", done: false, createdAt: 0 },
];
const sessions: [number, string, string][] = [
  // Październik: tylko w raporcie października.
  [1, "2026-10-20T09:00:00", "2026-10-20T13:00:00"],
  // Przez północ 31.10 → 1.11: do listopada należy tylko godzina.
  [3, "2026-10-31T23:00:00", "2026-11-01T01:00:00"],
  [1, "2026-11-02T09:00:00", "2026-11-02T11:00:00"],
  [2, "2026-11-03T09:00:00", "2026-11-03T12:00:00"],
  [1, "2026-11-10T10:00:00", "2026-11-10T11:30:00"],
];

async function seed(page: Page, now: string, settings?: object) {
  await page.clock.setFixedTime(new Date(now));
  await page.goto("/");
  await page.evaluate(
    ([k, sk, value, settings]) => {
      localStorage.setItem(k as string, value as string);
      if (settings) localStorage.setItem(sk as string, settings as string);
      else localStorage.removeItem(sk as string);
    },
    [
      key,
      settingsKey,
      JSON.stringify({
        tasks,
        sessions: sessions.map(([taskId, a, b], i) => ({
          id: i + 1,
          taskId,
          startedAt: +new Date(a + "+01:00"),
          endedAt: +new Date(b + "+01:00"),
        })),
      }),
      settings ? JSON.stringify(settings) : null,
    ] as const,
  );
  await page.reload();
}

test("month tab: total against the month target without holidays, tables and day chart", async ({
  page,
}) => {
  // Piątek 20 listopada 2026. Listopad: 21 dni Pon–Pt, w tym święto 11.11 (środa).
  await seed(page, "2026-11-20T12:00:00+01:00");
  await page
    .getByRole("button", { name: "Raporty dzienne", exact: true })
    .click();
  await page.getByRole("button", { name: "Miesiąc", exact: true }).click();

  const summary = page.getByRole("region", { name: "Podsumowanie miesiąca" });
  await expect(summary).toContainText("listopad 2026");
  await expect(summary.locator(".report-total")).toHaveText("07:30:00");
  await expect(
    page.getByRole("progressbar", { name: "Miesiąc względem celu" }),
  ).toHaveAttribute("aria-valuetext", "07:30 / 160:00");
  await expect(summary.locator(".monthly-split dd")).toHaveText([
    "07:30",
    "152:30",
    "160:00",
  ]);
  await expect(summary).toContainText("20 dni roboczych × 08:00");
  await expect(summary).toContainText("11 lis Święto Niepodległości");

  // Według projektu: WMS 3:30, Zespół 3:00, bez projektu 1:00 (część po północy).
  const projects = page.getByRole("region", { name: "Według projektu" });
  await expect(projects.locator("tbody tr")).toHaveText([
    /WMS\s*1\s*03:30\s*47%/,
    /Zespół\s*1\s*03:00\s*40%/,
    /Bez projektu\s*1\s*01:00\s*13%/,
  ]);
  const byTask = page.getByRole("region", { name: "Według zadania" });
  await expect(byTask.locator("tbody tr").first()).toHaveText(
    /Analiza importu\s*WMS-12\s*WMS\s*03:30\s*3,50/,
  );
  await expect(byTask.locator("tfoot")).toContainText("7,50");

  // Wykres dni: 30 dni, linia celu tylko w 20 dniach roboczych, święto opisane.
  const chart = page.getByRole("region", {
    name: "Czas pracy w dniach miesiąca",
  });
  await expect(chart.locator(".monthly-day")).toHaveCount(30);
  await expect(chart.locator(".monthly-day .target-line")).toHaveCount(20);
  const holiday = chart.getByRole("button", {
    name: "Raport 2026-11-11: 00:00:00",
  });
  await expect(holiday).toContainText("Święto Niepodległości");
  await expect(holiday.locator(".target-line")).toHaveCount(0);

  // Poprzedni miesiąc: październik z sesją 20.10 i godziną przed północą 31.10.
  await page.getByRole("button", { name: "Poprzedni miesiąc" }).click();
  await expect(page.getByLabel("Miesiąc raportu")).toHaveValue("2026-10");
  await expect(summary.locator(".report-total")).toHaveText("05:00:00");
  // Październik 2026: 22 dni Pon–Pt, bez świąt.
  await expect(
    page.getByRole("progressbar", { name: "Miesiąc względem celu" }),
  ).toHaveAttribute("aria-valuetext", "05:00 / 176:00");
  await page.getByRole("button", { name: "Bieżący miesiąc" }).click();
  await expect(page.getByLabel("Miesiąc raportu")).toHaveValue("2026-11");
  // Zakładka Miesiąc nie pokazuje raportu jednego dnia.
  await expect(
    page.getByRole("region", { name: "Podsumowanie dnia" }),
  ).toHaveCount(0);

  // Kliknięcie dnia otwiera raport jednego dnia.
  await chart
    .getByRole("button", { name: "Raport 2026-11-03: 03:00:00" })
    .click();
  await expect(page.getByLabel("Dzień raportu")).toHaveValue("2026-11-03");
});

test("month target counts holidays again when the setting is off", async ({
  page,
}) => {
  await seed(page, "2026-11-20T12:00:00+01:00");
  await page
    .getByRole("button", { name: "Historia i kopie", exact: true })
    .click();
  await page.getByRole("button", { name: "Ustawienia", exact: true }).click();
  const box = page.getByRole("checkbox", { name: "Uwzględniaj święta w celu" });
  await expect(box).toBeChecked();
  await box.uncheck();
  await expect(box).not.toBeChecked();
  await page
    .getByRole("button", { name: "Raporty dzienne", exact: true })
    .click();
  await page.getByRole("button", { name: "Miesiąc", exact: true }).click();
  await expect(
    page.getByRole("progressbar", { name: "Miesiąc względem celu" }),
  ).toHaveAttribute("aria-valuetext", "07:30 / 168:00");
  await expect(
    page
      .getByRole("region", { name: "Czas pracy w dniach miesiąca" })
      .locator(".monthly-day .target-line"),
  ).toHaveCount(21);
});

test("a holiday has no day target and the week board marks it as free", async ({
  page,
}) => {
  // Środa 11 listopada 2026, Święto Niepodległości.
  await seed(page, "2026-11-11T12:00:00+01:00");
  // Zadania: brak paska celu dnia.
  await expect(page.locator(".target-text")).toHaveCount(0);

  // Dashboard: dzień wolny z nazwą święta, cel tygodnia 4 × 8 h.
  await page.getByRole("button", { name: "Dashboard", exact: true }).click();
  await expect(page.locator(".dashboard-stats")).toContainText(
    "Święto Niepodległości",
  );
  await expect(
    page.getByRole("progressbar", { name: "Ten tydzień względem celu" }),
  ).toHaveAttribute("aria-valuetext", "01:30 / 32:00");

  // Tablica tygodnia: środa bez pojemności, z opisem święta.
  await page
    .getByRole("button", { name: "Plan i historia", exact: true })
    .click();
  await page.getByRole("button", { name: "Tydzień", exact: true }).click();
  const wednesday = page.getByRole("region", {
    name: "Środa 2026-11-11",
    exact: true,
  });
  await expect(wednesday).toHaveClass(/free/);
  await expect(wednesday).toContainText("Święto Niepodległości");
  await expect(
    page.getByRole("progressbar", {
      name: "Pojemność: Środa 2026-11-11",
      exact: true,
    }),
  ).toHaveAttribute("aria-valuetext", "00:00 / wolne");
  await expect(
    page.getByRole("progressbar", {
      name: "Pojemność: Czwartek 2026-11-12",
      exact: true,
    }),
  ).toHaveAttribute("aria-valuetext", "00:00 / 08:00");

  // Podsumowanie tygodnia: cel bez święta.
  await page.getByRole("button", { name: "Podsumowanie", exact: true }).click();
  await expect(page.getByText("Cel tygodnia 32:00 · −30:30")).toBeVisible();
});
