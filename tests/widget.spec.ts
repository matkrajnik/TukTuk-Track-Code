import { test, expect, type Page } from "./fixtures/test";
import { openSettings } from "./fixtures/settings";

test.use({ timezoneId: "Europe/Warsaw" });
const dataKey = "tuktuk-track-preview-v1";
const settingsKey = "tuktuk-track-preview-settings-v1";

async function seed(page: Page) {
  await page.clock.install({ time: new Date("2026-10-14T09:59:50+02:00") });
  await page.goto("/#widget");
  await page.evaluate((key) => {
    const at = (s: string) => new Date(s).getTime();
    localStorage.setItem(
      key,
      JSON.stringify({
        tasks: [
          {
            id: 1,
            title: "Przegląd kodu",
            done: false,
            createdAt: at("2026-10-14T07:00:00+02:00"),
          },
          {
            id: 2,
            title: "Raport miesięczny dla działu sprzedaży i marketingu",
            done: false,
            createdAt: at("2026-10-14T07:00:00+02:00"),
            details: { project: "Biuro" },
          },
        ],
        sessions: [
          {
            id: 1,
            taskId: 1,
            startedAt: at("2026-10-14T08:00:00+02:00"),
            endedAt: at("2026-10-14T09:00:00+02:00"),
          },
          {
            id: 2,
            taskId: 2,
            startedAt: at("2026-10-14T09:00:00+02:00"),
            endedAt: null,
          },
        ],
      }),
    );
  }, dataKey);
  await page.reload();
  await page.clock.pauseAt(new Date("2026-10-14T10:00:00+02:00"));
}
const active = (page: Page) =>
  page.evaluate(
    (key) =>
      JSON.parse(localStorage.getItem(key) ?? "{}").sessions.filter(
        (s: { endedAt: number | null }) => s.endedAt === null,
      ),
    dataKey,
  );

test("widget shows the running measurement and toggles Stop and Start", async ({
  page,
}) => {
  await page.setViewportSize({ width: 320, height: 64 });
  await seed(page);
  const widget = page.locator("#widget");
  await expect(widget).toBeVisible();
  await expect(widget).toHaveAttribute("aria-label", "Mini-widget");
  const name = widget.getByRole("button", { name: "Pokaż główne okno" });
  await expect(name).toContainText(
    "Raport miesięczny dla działu sprzedaży i marketingu",
  );
  await expect(name).toContainText("Biuro");
  await expect(widget.getByLabel("Czas bieżącego pomiaru")).toHaveText(
    "01:00:00",
  );
  await page.clock.runFor(2000);
  await expect(widget.getByLabel("Czas bieżącego pomiaru")).toHaveText(
    "01:00:02",
  );
  // Nazwa mieści się w oknie 320 px (wielokropek), nic nie wystaje poza widget.
  const scroll = await page.evaluate(() => ({
    width: document.documentElement.scrollWidth,
    height: document.documentElement.scrollHeight,
  }));
  expect(scroll).toEqual({ width: 320, height: 64 });
  await page.screenshot({ path: "test-results/widget-running.png" });

  // Stop: pomiar zakończony, potwierdzenie z czasem, przycisk zmienia się na Start.
  await widget.getByRole("button", { name: "Zatrzymaj pomiar" }).click();
  await expect(name).toContainText("Brak pomiaru");
  await expect(widget.getByRole("status")).toHaveText(
    "Stop: Raport miesięczny dla działu sprzedaży i marketingu · 01:00:02",
  );
  await expect(widget.getByLabel("Czas bieżącego pomiaru")).toHaveCount(0);
  expect(await active(page)).toEqual([]);
  await page.clock.runFor(5000);
  await expect(widget.getByRole("status")).toHaveText(
    "Ostatnio: Raport miesięczny dla działu sprzedaży i marketingu",
  );
  await page.screenshot({ path: "test-results/widget-idle.png" });

  // Start: wznawia ostatnio mierzone zadanie (ta sama logika co skrót Start/Stop).
  await widget.getByRole("button", { name: "Wznów ostatnie zadanie" }).click();
  await expect(widget.getByRole("status")).toHaveText(
    "Start: Raport miesięczny dla działu sprzedaży i marketingu",
  );
  await expect(
    widget.getByRole("button", { name: "Zatrzymaj pomiar" }),
  ).toBeVisible();
  await expect(widget.getByLabel("Czas bieżącego pomiaru")).toHaveText(
    "00:00:00",
  );
  expect((await active(page)).map((s: { taskId: number }) => s.taskId)).toEqual(
    [2],
  );

  // Stan przetrwał przeładowanie, bo pochodzi z magazynu.
  await page.reload();
  await expect(
    page.locator("#widget").getByRole("button", { name: "Zatrzymaj pomiar" }),
  ).toBeVisible();
});

test("widget without anything to resume opens quick switch, and X hides it", async ({
  page,
}) => {
  await page.goto("/#widget");
  const widget = page.locator("#widget");
  await expect(widget.getByRole("button", { name: "Pokaż główne okno" })).toContainText(
    "Brak pomiaru",
  );
  await widget.getByRole("button", { name: "Wznów ostatnie zadanie" }).click();
  await expect(
    page.getByRole("dialog", { name: "Szybkie przełączanie" }),
  ).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);

  await widget.getByRole("button", { name: "Schowaj widget" }).click();
  await expect(page.locator("#widget")).toHaveCount(0);
  await expect(page.getByText("Mini-widget schowany do końca sesji.")).toBeVisible();
  // X nie zmienia ustawienia.
  expect(
    await page.evaluate((k) => localStorage.getItem(k), settingsKey),
  ).toBeNull();
  await page.getByRole("button", { name: "Pokaż widget" }).click();
  await expect(page.locator("#widget")).toBeVisible();
});

test("header switch enables the widget and hides it for the session only", async ({
  page,
}) => {
  await page.goto("/");
  const toggle = page.getByRole("button", { name: "Mini-widget", exact: true });
  await expect(toggle).toHaveAttribute("aria-pressed", "false");
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-pressed", "true");
  const settings = () =>
    page.evaluate((k) => JSON.parse(localStorage.getItem(k) ?? "{}"), settingsKey);
  expect(await settings()).toEqual({ widgetEnabled: true });

  // Ponowne kliknięcie chowa widget do końca sesji, ustawienie zostaje włączone.
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-pressed", "false");
  expect(await settings()).toEqual({ widgetEnabled: true });

  // Ustawienia pokazują włączony widget; przełącznik go przywraca.
  await openSettings(page, "Okna");
  await expect(
    page.getByLabel("Pokazuj mini-widget zawsze na wierzchu"),
  ).toBeChecked();
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-pressed", "true");

  // Wyłączenie w ustawieniach gasi przełącznik.
  await page.getByLabel("Pokazuj mini-widget zawsze na wierzchu").uncheck();
  await expect(toggle).toHaveAttribute("aria-pressed", "false");
  expect(await settings()).toEqual({ widgetEnabled: false });
});

test("re-enabling the widget in settings clears the session dismissal", async ({
  page,
}) => {
  await page.goto("/");
  const toggle = page.getByRole("button", { name: "Mini-widget", exact: true });
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-pressed", "true");
  // Schowanie do końca sesji (jak X na widgecie).
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-pressed", "false");

  await openSettings(page, "Okna");
  const checkbox = page.getByLabel("Pokazuj mini-widget zawsze na wierzchu");
  await expect(checkbox).toBeChecked();
  await checkbox.uncheck();
  await expect(toggle).toHaveAttribute("aria-pressed", "false");
  await checkbox.check();
  await expect(toggle).toHaveAttribute("aria-pressed", "true");

  // Po przeładowaniu (ta sama sesja karty) widget nadal nie jest schowany.
  await page.reload();
  await expect(
    page.getByRole("button", { name: "Mini-widget", exact: true }),
  ).toHaveAttribute("aria-pressed", "true");
});

test("pinned tasks: pin from the task card, switch with one click in the widget", async ({
  page,
}) => {
  await seed(page);
  // Dwa kolejne zadania: ze zgłoszeniem Jiry (przycisk pokazuje klucz) i czwarte.
  await page.evaluate((key) => {
    const data = JSON.parse(localStorage.getItem(key)!);
    data.tasks.unshift(
      {
        id: 3,
        title: "Poprawka logowania",
        done: false,
        createdAt: 0,
        jiraKey: "WMS-12",
      },
      { id: 4, title: "Czwarte zadanie", done: false, createdAt: 0 },
    );
    localStorage.setItem(key, JSON.stringify(data));
  }, dataKey);
  await page.goto("/");
  for (const title of [
    "Przegląd kodu",
    "Poprawka logowania",
    "Raport miesięczny dla działu sprzedaży i marketingu",
    "Czwarte zadanie",
  ])
    await page
      .getByRole("button", { name: `Przedziały: ${title}`, exact: true })
      .click();
  for (const title of [
    "Przegląd kodu",
    "Poprawka logowania",
    "Raport miesięczny dla działu sprzedaży i marketingu",
  ]) {
    await page
      .getByRole("button", { name: `Przypnij do widgetu: ${title}` })
      .click();
    await expect(
      page.getByRole("button", { name: `Odepnij od widgetu: ${title}` }),
    ).toHaveAttribute("aria-pressed", "true");
    await page.clock.runFor(1000);
  }
  // Czwarte przekracza limit trzech.
  await page
    .getByRole("button", { name: "Przypnij do widgetu: Czwarte zadanie" })
    .click();
  await expect(
    page.getByText("Przypiąć można najwyżej 3 zadania. Odepnij jedno z nich."),
  ).toBeVisible();

  await page.setViewportSize({ width: 320, height: 96 });
  // Zmiana samego hasha nie przeładowuje strony: widget startuje od nowa.
  await page.goto("/#widget");
  await page.reload();
  const widget = page.locator("#widget");
  const pins = widget
    .getByRole("group", { name: "Przypięte zadania" })
    .getByRole("button");
  // Kolejność przypięcia; klucz Jiry zamiast tytułu; mierzone zadanie wyróżnione.
  await expect(pins).toHaveText([
    "Przegląd kodu",
    "WMS-12",
    "Raport miesięczny dla działu sprzedaży i marketingu",
  ]);
  await expect(pins.nth(2)).toHaveAttribute("aria-pressed", "true");
  const scroll = await page.evaluate(() => ({
    width: document.documentElement.scrollWidth,
    height: document.documentElement.scrollHeight,
  }));
  expect(scroll).toEqual({ width: 320, height: 96 });
  await page.screenshot({ path: "test-results/widget-pins.png" });

  // Jedno kliknięcie: poprzedni pomiar staje, startuje przypięte zadanie.
  await widget
    .getByRole("button", { name: "Przełącz na: Poprawka logowania" })
    .click();
  await expect(widget.getByRole("status")).toHaveText(
    "Start: Poprawka logowania",
  );
  await expect(
    widget.getByRole("button", { name: "Pokaż główne okno" }),
  ).toContainText("Poprawka logowania");
  await expect(pins.nth(1)).toHaveAttribute("aria-pressed", "true");
  expect((await active(page)).map((s: { taskId: number }) => s.taskId)).toEqual(
    [3],
  );
  await widget
    .getByRole("button", { name: "Przełącz na: Przegląd kodu" })
    .click();
  expect((await active(page)).map((s: { taskId: number }) => s.taskId)).toEqual(
    [1],
  );

  // Ukończone zadanie znika z rzędu; bez przypiętych rząd znika.
  await page.evaluate((key) => {
    const data = JSON.parse(localStorage.getItem(key)!);
    for (const t of data.tasks)
      if (t.id !== 2) t.details = { ...t.details, pinnedAt: 0 };
    data.tasks.find((t: { id: number }) => t.id === 2).done = true;
    localStorage.setItem(key, JSON.stringify(data));
  }, dataKey);
  await page.reload();
  await expect(
    page.locator("#widget").getByRole("group", { name: "Przypięte zadania" }),
  ).toHaveCount(0);
});

test("podlinia widgetu pokazuje sumę dnia jak tray: 2:00 / 8:00 (B11)", async ({
  page,
}) => {
  await page.setViewportSize({ width: 320, height: 64 });
  await seed(page);
  const widget = page.locator("#widget");
  // Środa 14.10: 8:00–9:00 zamknięte + trwający od 9:00, cel dnia 8:00.
  await expect(widget.getByRole("status")).toHaveText(
    "2:00 / 8:00 · Biuro",
  );
  await expect(widget.getByRole("status")).toHaveAttribute(
    "title",
    "Dziś: 2:00 z 8:00",
  );
  await page.clock.runFor(60_000);
  await expect(widget.getByRole("status")).toHaveText(
    "2:01 / 8:00 · Biuro",
  );
  // Inny cel dnia; dzień wolny (środa poza dniami roboczymi) bez celu.
  await page.evaluate(
    (key) => localStorage.setItem(key, JSON.stringify({ targetDayMinutes: 450 })),
    settingsKey,
  );
  await page.reload();
  await expect(widget.getByRole("status")).toHaveText(
    "2:01 / 7:30 · Biuro",
  );
  await page.evaluate(
    (key) => localStorage.setItem(key, JSON.stringify({ workDays: [1, 2] })),
    settingsKey,
  );
  await page.reload();
  await expect(widget.getByRole("status")).toHaveText("2:01 · Biuro");
});
