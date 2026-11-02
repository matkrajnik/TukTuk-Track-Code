import { test, expect, type Page } from "./fixtures/test";
import { NOW, TODAY, at, ids, openPreview, seed, stored } from "./fixtures/seed";
import { backend, openDesktop } from "./fixtures/tauri";
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });

const input = (p: Page) => p.getByLabel("Nowe zadanie", { exact: true });
const card = (p: Page, title: string) =>
  p
    .locator(".task-card")
    .filter({ has: p.locator(".task-title", { hasText: title }) });
const openSessions = async (p: Page) =>
  (await stored(p)).sessions.filter((s: any) => s.endedAt === null);
const running = { active: { task: ids.analysis, from: "11:00" } };

test("„Dodaj i rozpocznij” (Enter) dodaje zadanie i uruchamia jego pomiar; poprzedni staje", async ({
  page,
}) => {
  await openPreview(page, running);
  await expect(input(page)).toHaveAttribute(
    "aria-keyshortcuts",
    "Enter Shift+Enter",
  );
  await input(page).fill("Wdrożenie poprawki ~30m");
  await input(page).press("Enter");
  await expect(input(page)).toHaveValue("");
  await expect(input(page)).toBeFocused();
  await expect(
    page.getByRole("button", { name: "Stop: Wdrożenie poprawki", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Start: Analiza importu", exact: true }),
  ).toBeVisible();

  const data = await stored(page);
  const created = data.tasks.find((t: any) => t.title === "Wdrożenie poprawki");
  expect(data.sessions.filter((s: any) => s.endedAt === null)).toEqual([
    expect.objectContaining({ taskId: created.id, startedAt: Date.parse(NOW) }),
  ]);
  const previous = data.sessions.filter(
    (s: any) =>
      s.taskId === ids.analysis &&
      s.startedAt === at(TODAY, "11:00"),
  );
  expect(previous.map((s: any) => s.endedAt)).toEqual([Date.parse(NOW)]);
  expect(created.details.estimateMinutes).toBe(30);

  // Tekst bez składni też startuje (dawna ścieżka „mutate add”).
  await input(page).fill("Telefon do klienta");
  await page
    .getByRole("button", { name: "Dodaj i rozpocznij", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Stop: Telefon do klienta", exact: true }),
  ).toBeVisible();
  const phone = (await stored(page)).tasks.find(
    (t: any) => t.title === "Telefon do klienta",
  );
  expect(phone.details).toBeUndefined();
});

test("„Zaplanuj” (Shift+Enter) dodaje bez pomiaru i rozwija plan nowego zadania", async ({
  page,
}) => {
  await openPreview(page, running);
  const plan = page.getByRole("button", { name: "Zaplanuj", exact: true });
  await input(page).fill("Przegląd umowy @pt");
  await plan.click();
  await expect(input(page)).toHaveValue("");
  await expect(
    page.getByRole("button", { name: "Stop: Analiza importu", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Przedziały: Przegląd umowy", exact: true }),
  ).toHaveAttribute("aria-expanded", "true");
  const day = card(page, "Przegląd umowy").getByLabel("Planowany dzień");
  await expect(day).toBeFocused();
  await expect(day).toHaveValue("2026-10-16");
  expect((await openSessions(page)).map((s: any) => s.taskId)).toEqual([
    ids.analysis,
  ]);

  // Filtr ukrywający nowe zadanie wraca do „Do zrobienia”.
  const today = page.getByLabel("Dzisiaj: plan i praca");
  await today.check();
  await input(page).fill("Notatki ze spotkania");
  await input(page).press("Shift+Enter");
  await expect(today).not.toBeChecked();
  const day2 = card(page, "Notatki ze spotkania").getByLabel("Planowany dzień");
  await expect(day2).toBeFocused();
  await expect(day2).toHaveValue("");
  expect((await openSessions(page)).map((s: any) => s.taskId)).toEqual([
    ids.analysis,
  ]);

  await input(page).fill("Raport @31.02");
  await expect(plan).toBeDisabled();
  await expect(
    page.getByRole("button", { name: "Dodaj i rozpocznij", exact: true }),
  ).toBeDisabled();
  await input(page).press("Shift+Enter");
  expect(
    (await stored(page)).tasks.some((t: any) => t.title.startsWith("Raport")),
  ).toBe(false);
});

test("po zakończonym dniu „Dodaj i rozpocznij” pyta „Wznowić dzień?”", async ({
  page,
}) => {
  await openPreview(page, {
    data: {
      ...seed(),
      dayMarks: [
        {
          day: TODAY,
          startedAt: at(TODAY, "08:00"),
          endedAt: at(TODAY, "11:30"),
          resumed: 0,
        },
      ],
    },
  });
  const dialog = page.getByRole("dialog", { name: "Wznowić dzień?" });
  await input(page).fill("Pilna poprawka");
  await input(page).press("Enter");
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Anuluj" }).click();
  await expect(dialog).toHaveCount(0);
  const data = await stored(page);
  expect(data.tasks.some((t: any) => t.title === "Pilna poprawka")).toBe(true);
  expect(await openSessions(page)).toHaveLength(0);
  expect(data.dayMarks[0].endedAt).not.toBeNull();
  await expect(
    page.locator(".add-notice"),
  ).toHaveText("Dodano „Pilna poprawka” bez startu pomiaru: dzień pozostał zakończony.");

  await input(page).fill("Druga poprawka");
  await page
    .getByRole("button", { name: "Dodaj i rozpocznij", exact: true })
    .click();
  await dialog.getByRole("button", { name: "Wznów i start" }).click();
  await expect(
    page.getByRole("button", { name: "Stop: Druga poprawka", exact: true }),
  ).toBeVisible();
  await expect(dialog).toHaveCount(0);
  expect((await stored(page)).dayMarks[0]).toMatchObject({
    endedAt: null,
    resumed: 1,
  });
  expect(await openSessions(page)).toHaveLength(1);
});

test("aplikacja: Enter i Shift+Enter wołają quick_add ze start true/false", async ({
  page,
}) => {
  await openDesktop(page);
  const lastAdd = async () => {
    const calls = await backend<any[]>(page, "calls");
    return calls.filter((c) => c.cmd === "quick_add").at(-1);
  };
  await input(page).fill("Zadanie w aplikacji");
  await input(page).press("Enter");
  await expect(
    page.getByRole("button", { name: "Stop: Zadanie w aplikacji", exact: true }),
  ).toBeVisible();
  expect((await lastAdd()).args.task.start).toBe(true);

  await input(page).fill("Plan w aplikacji");
  await input(page).press("Shift+Enter");
  await expect(
    page.getByRole("button", { name: "Przedziały: Plan w aplikacji", exact: true }),
  ).toHaveAttribute("aria-expanded", "true");
  expect((await lastAdd()).args.task.start).toBe(false);
  await expect(
    page.getByRole("button", { name: "Stop: Zadanie w aplikacji", exact: true }),
  ).toBeVisible();
});

test("aplikacja: zadanie dodane z backendu podczas „Wznowić dzień?” nie myli nowego zadania", async ({
  page,
}) => {
  await openDesktop(page, {
    data: {
      ...seed(),
      dayMarks: [
        {
          day: TODAY,
          startedAt: at(TODAY, "08:00"),
          endedAt: at(TODAY, "11:30"),
          resumed: 0,
        },
      ],
    },
  });
  const dialog = page.getByRole("dialog", { name: "Wznowić dzień?" });
  await input(page).fill("Pilna poprawka");
  await input(page).press("Enter");
  await expect(dialog).toBeVisible();
  // Jak wątek cykli: zadanie z niższym id niż nowe, zanim użytkownik odpowie.
  await page.evaluate(async () => {
    const w = window as any;
    await w.__TAURI_INTERNALS__.invoke("quick_add", {
      task: { title: "Wystąpienie cyklu", start: false },
    });
    w.__tauriMock.emit("data-changed");
  });
  await expect(card(page, "Wystąpienie cyklu")).toBeVisible();
  await dialog.getByRole("button", { name: "Anuluj" }).click();
  await expect(page.locator(".add-notice")).toHaveText(
    "Dodano „Pilna poprawka” bez startu pomiaru: dzień pozostał zakończony.",
  );
});
