import { test, expect, type Page } from "./fixtures/test";
import { BOM, exportCsv, exportRows } from "../src/export";

test.use({ timezoneId: "Europe/Warsaw" });

const key = "tuktuk-track-preview-v1";

const tasks = [
  {
    id: 1,
    title: "Zażółć gęślą jaźń",
    done: false,
    createdAt: 0,
    jiraKey: "WMS-7",
    details: { project: "Magazyn Łódź", notes: { "2026-11-02": "Poprawki źródeł" } },
  },
  { id: 2, title: 'Spotkanie; "pilne"', done: false, createdAt: 0 },
];
const sessions: [number, string, string | null][] = [
  // Przez północ 1.11 → 2.11: dwa wiersze.
  [1, "2026-11-01T23:00:00", "2026-11-02T01:30:00"],
  [2, "2026-11-02T09:00:00", "2026-11-02T10:15:00"],
  // Trwa od 10:30; eksport o 12:00.
  [1, "2026-11-20T10:30:00", null],
];

async function seed(page: Page) {
  await page.clock.setFixedTime(new Date("2026-11-20T12:00:00+01:00"));
  await page.goto("/");
  await page.evaluate(
    ([k, value]) => localStorage.setItem(k, value),
    [
      key,
      JSON.stringify({
        tasks,
        sessions: sessions.map(([taskId, a, b], i) => ({
          id: i + 1,
          taskId,
          startedAt: +new Date(a + "+01:00"),
          endedAt: b ? +new Date(b + "+01:00") : null,
        })),
      }),
    ] as const,
  );
  await page.reload();
  await page
    .getByRole("button", { name: "Raporty dzienne", exact: true })
    .click();
}

// Od 1.9 na końcu kolumna „Redmine” (`#1234`); „Klucz Jiry” bez zmian.
const header =
  "Data;Start;Koniec;Minuty;Godziny;Zadanie;Klucz Jiry;Projekt;Notatka dnia;Uwagi;Redmine";

test("month export in preview returns CSV text; XLSX is unavailable", async ({
  page,
}) => {
  await seed(page);
  await page.getByRole("button", { name: "Miesiąc", exact: true }).click();
  const group = page.getByRole("group", { name: "Eksport miesiąca" });
  await group.getByRole("button", { name: "Eksport CSV" }).click();
  await expect(group.getByRole("status")).toContainText("Podgląd");
  const text = await group
    .getByRole("textbox", { name: "Treść eksportu CSV" })
    .inputValue();
  expect(text.startsWith(BOM)).toBe(true);
  // Pole tekstowe normalizuje CRLF do LF; CRLF sprawdza test exportCsv niżej.
  expect(text.slice(1).split("\n")).toEqual([
    header,
    "2026-11-01;23:00;24:00;60;1,00;Zażółć gęślą jaźń;WMS-7;Magazyn Łódź;;;",
    "2026-11-02;00:00;01:30;90;1,50;Zażółć gęślą jaźń;WMS-7;Magazyn Łódź;Poprawki źródeł;;",
    '2026-11-02;09:00;10:15;75;1,25;"Spotkanie; ""pilne""";;;;;',
    "2026-11-20;10:30;12:00;90;1,50;Zażółć gęślą jaźń;WMS-7;Magazyn Łódź;;trwa;",
    "",
  ]);

  await group.getByRole("button", { name: "Eksport XLSX" }).click();
  await expect(group.getByRole("status")).toHaveText(
    "Eksport XLSX jest dostępny tylko w aplikacji Windows.",
  );
  await expect(
    group.getByRole("textbox", { name: "Treść eksportu CSV" }),
  ).toHaveCount(0);

  // Październik bez przedziałów: sam nagłówek.
  await page.getByRole("button", { name: "Poprzedni miesiąc" }).click();
  await group.getByRole("button", { name: "Eksport CSV" }).click();
  await expect(
    group.getByRole("textbox", { name: "Treść eksportu CSV" }),
  ).toHaveValue(BOM + header + "\n");
});

test("selected days export covers only the chosen days", async ({ page }) => {
  await seed(page);
  await page.getByRole("button", { name: "Wybrane dni", exact: true }).click();
  await page.getByRole("button", { name: "Wyczyść", exact: true }).click();
  const group = page.getByRole("group", { name: "Eksport wybranych dni" });
  await expect(group.getByRole("button", { name: "Eksport CSV" })).toBeDisabled();
  await page
    .getByRole("button", { name: "2 listopada 2026", exact: true })
    .click();
  await group.getByRole("button", { name: "Eksport CSV" }).click();
  await expect(
    group.getByRole("textbox", { name: "Treść eksportu CSV" }),
  ).toHaveValue(
    [
      BOM + header,
      "2026-11-02;00:00;01:30;90;1,50;Zażółć gęślą jaźń;WMS-7;Magazyn Łódź;Poprawki źródeł;;",
      '2026-11-02;09:00;10:15;75;1,25;"Spotkanie; ""pilne""";;;;;',
      "",
    ].join("\n"),
  );
  // Zmiana zaznaczenia czyści poprzedni wynik.
  await page
    .getByRole("button", { name: "20 listopada 2026", exact: true })
    .click();
  await expect(
    group.getByRole("textbox", { name: "Treść eksportu CSV" }),
  ).toHaveCount(0);
});

test("exportCsv: BOM, CRLF, rounding and formula guard match the Rust export", () => {
  // Czas lokalny procesu testów (bez strefy), żeby wynik nie zależał od TZ.
  const at = (h: number, m: number, s = 0) =>
    new Date(2026, 10, 2, h, m, s).getTime();
  const rows = exportRows(
    {
      tasks: [
        { id: 1, title: "=SUMA(A1)", done: false, createdAt: 0 },
        // Zgłoszenie Redmine: `#1234` w ostatniej kolumnie, „Klucz Jiry” pusty.
        { id: 3, title: "Import CSV", done: false, createdAt: 0, redmineId: 1234 },
      ],
      sessions: [
        // 7,5 minuty = 0,125 h: połówka w górę, jak w Rust.
        { id: 1, taskId: 1, startedAt: at(8, 0), endedAt: at(8, 7, 30) },
        // Zadanie spoza listy (np. w koszu) nie trafia do eksportu.
        { id: 2, taskId: 2, startedAt: at(9, 0), endedAt: at(10, 0) },
        { id: 3, taskId: 3, startedAt: at(10, 0), endedAt: at(11, 0) },
      ],
    },
    ["2026-11-02"],
    at(12, 0),
  );
  expect(exportCsv(rows)).toBe(
    BOM +
      "Data;Start;Koniec;Minuty;Godziny;Zadanie;Klucz Jiry;Projekt;Notatka dnia;Uwagi;Redmine\r\n" +
      "2026-11-02;08:00;08:07;8;0,13;'=SUMA(A1);;;;;\r\n" +
      "2026-11-02;10:00;11:00;60;1,00;Import CSV;;;;;#1234\r\n",
  );
});
