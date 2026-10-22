import { test, expect, type Page } from "./fixtures/test";
import {
  REMINDERS_KEY,
  SETTINGS_KEY,
  TODAY,
  calendarFixture,
  ids,
  manualMeetings,
  openPreview,
  remindersFixture,
} from "./fixtures/seed";
import { openSettings } from "./fixtures/settings";
import {
  defaultReminderAt,
  firstOccurrence,
  nextOccurrence,
  whenLabel,
} from "../src/api/reminders";

/**
 * C3: przypomnienia. Dane z `fixtures/seed.ts`: teraz czw. 15.10 12:00,
 * `remindersFixture()` = jednorazowe dziś 15:30 z zadaniem „Code review”,
 * codzienne i tygodniowe od jutra oraz dwa wyłączone. Podgląd nie wysyła
 * powiadomień; wyzwalanie (`NotifyClock`) pokrywają testy Rust.
 */
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });

const nav = (page: Page, name: string) =>
  page.getByRole("button", { name, exact: true }).click();
const dialog = (page: Page, name: string) =>
  page.getByRole("dialog", { name, exact: true });
const remindersSection = (page: Page) =>
  page.getByRole("region", { name: "Moje przypomnienia" });
const stored = (page: Page) =>
  page.evaluate(
    (k) => JSON.parse(localStorage.getItem(k) ?? "{}"),
    REMINDERS_KEY,
  );
const openNotifications = async (page: Page) => {
  await openSettings(page, "Powiadomienia");
  await expect(remindersSection(page)).toBeVisible();
};

test("reguły: następne wystąpienie codziennie, w dni robocze i co tydzień", () => {
  // Codziennie.
  expect(
    nextOccurrence("daily", [], "2026-10-15T09:00", "2026-10-15T09:00"),
  ).toBe("2026-10-16T09:00");
  expect(
    nextOccurrence("daily", [], "2026-10-10T09:00", "2026-10-15T09:30"),
  ).toBe("2026-10-16T09:00");
  // W dni robocze: piątek → poniedziałek (weekend) i święto 11.11 (środa).
  expect(
    nextOccurrence("workdays", [], "2026-10-16T09:00", "2026-10-16T09:00"),
  ).toBe("2026-10-19T09:00");
  expect(
    nextOccurrence("workdays", [], "2026-11-10T09:00", "2026-11-10T09:00"),
  ).toBe("2026-11-12T09:00");
  // Co tydzień: dni z listy.
  expect(
    nextOccurrence("weekly", [1, 3, 5], "2026-10-14T10:00", "2026-10-14T10:00"),
  ).toBe("2026-10-16T10:00");
  expect(
    nextOccurrence("weekly", [1, 3, 5], "2026-10-16T10:00", "2026-10-16T10:00"),
  ).toBe("2026-10-19T10:00");
  // Jednorazowe nie ma następnego; pierwsze wystąpienie nie cofa się w czasie.
  expect(
    nextOccurrence("none", [], "2026-10-16T10:00", "2026-10-16T10:00"),
  ).toBeNull();
  expect(
    firstOccurrence("daily", [], "2026-10-15T09:00", "2026-10-15T09:30"),
  ).toBe("2026-10-16T09:00");
  expect(
    firstOccurrence("workdays", [], "2026-10-17T08:00", "2026-10-17T08:00"),
  ).toBe("2026-10-19T08:00");
});

test("domyślny czas za godzinę zaokrąglony do 5 minut i etykiety", () => {
  const at = (iso: string) => +new Date(iso);
  expect(defaultReminderAt(at("2026-10-15T12:00:00"))).toBe("2026-10-15T13:00");
  expect(defaultReminderAt(at("2026-10-15T12:01:00"))).toBe("2026-10-15T13:05");
  expect(defaultReminderAt(at("2026-10-15T12:56:00"))).toBe("2026-10-15T14:00");
  expect(defaultReminderAt(at("2026-10-15T23:30:00"))).toBe("2026-10-16T00:30");
  expect(whenLabel("2026-10-15T15:30", "2026-10-15")).toBe("15:30");
  expect(whenLabel("2026-10-16T08:45", "2026-10-15")).toBe("jutro 08:45");
});

test("Ustawienia → Powiadomienia: dodanie, edycja, wyłączenie i usunięcie przypomnienia", async ({
  page,
}) => {
  await openPreview(page);
  await openNotifications(page);
  const section = remindersSection(page);
  await expect(section.getByText("Brak przypomnień.")).toBeVisible();

  await section.getByRole("button", { name: "Dodaj przypomnienie" }).click();
  const add = dialog(page, "Dodaj przypomnienie");
  // Domyślnie za godzinę (teraz 12:00), jednorazowo, dziś.
  await expect(add.getByLabel("Dzień", { exact: true })).toHaveValue(TODAY);
  await expect(add.getByLabel("Godzina", { exact: true })).toHaveValue("13:00");
  await expect(add.getByLabel("Powtarzanie", { exact: true })).toHaveValue(
    "none",
  );
  // Walidacja: pusta treść blokuje formularz, przeszłość i brak dni dają błędy.
  await add.getByLabel("Treść", { exact: true }).fill("Zadzwonić do klienta");
  await add.getByLabel("Godzina", { exact: true }).fill("11:00");
  await add.getByRole("button", { name: "Zapisz" }).click();
  await expect(add.getByRole("alert")).toHaveText(
    "Przypomnienie jednorazowe musi być w przyszłości.",
  );
  await add.getByLabel("Godzina", { exact: true }).fill("13:00");
  await add.getByLabel("Powtarzanie", { exact: true }).selectOption("weekly");
  await add.getByRole("button", { name: "Zapisz" }).click();
  await expect(add.getByRole("alert")).toHaveText(
    "Zaznacz co najmniej jeden dzień tygodnia.",
  );
  await add.getByLabel("poniedziałek", { exact: true }).check();
  await add.getByLabel("piątek", { exact: true }).check();
  await add.getByRole("button", { name: "Zapisz" }).click();
  await expect(add).toHaveCount(0);

  const item = section.locator("li", { hasText: "Zadzwonić do klienta" });
  // Co tydzień w pn i pt: czwartek 15.10 13:00 przechodzi na piątek.
  await expect(item).toContainText("Co tydzień: Pn, Pt");
  await expect(item.locator(".reminder-when")).toHaveText("jutro 13:00");
  expect((await stored(page)).reminders[0]).toMatchObject({
    text: "Zadzwonić do klienta",
    repeat: "weekly",
    weekdays: [1, 5],
    at: "2026-10-16T13:00",
    enabled: true,
    taskId: null,
  });

  // Edycja: codziennie o 14:15.
  await section
    .getByRole("button", { name: "Edytuj przypomnienie: Zadzwonić do klienta" })
    .click();
  const edit = dialog(page, "Edytuj przypomnienie");
  await expect(edit.getByLabel("Treść", { exact: true })).toHaveValue(
    "Zadzwonić do klienta",
  );
  await edit.getByLabel("Powtarzanie", { exact: true }).selectOption("daily");
  await expect(edit.getByRole("group", { name: "Dni tygodnia" })).toHaveCount(0);
  await edit.getByLabel("Dzień", { exact: true }).fill(TODAY);
  await edit.getByLabel("Godzina", { exact: true }).fill("14:15");
  await edit.getByRole("button", { name: "Zapisz" }).click();
  await expect(edit).toHaveCount(0);
  await expect(item).toContainText("Codziennie");
  await expect(item.locator(".reminder-when")).toHaveText("14:15");

  // Wyłączenie i ponowne włączenie.
  const toggle = section.getByLabel("Włączone: Zadzwonić do klienta");
  await toggle.uncheck();
  await expect(item.locator(".reminder-when")).toHaveText("wyłączone");
  expect((await stored(page)).reminders[0].enabled).toBe(false);
  await toggle.check();
  await expect(item.locator(".reminder-when")).toHaveText("14:15");

  // Usunięcie z potwierdzeniem; „Anuluj” zostawia przypomnienie.
  const remove = section.getByRole("button", {
    name: "Usuń przypomnienie: Zadzwonić do klienta",
  });
  await remove.click();
  const confirm = dialog(page, "Usunąć przypomnienie?");
  await confirm.getByRole("button", { name: "Anuluj" }).click();
  await expect(confirm).toHaveCount(0);
  await expect(item).toHaveCount(1);
  await remove.click();
  await dialog(page, "Usunąć przypomnienie?")
    .getByRole("button", { name: "Usuń" })
    .click();
  await expect(item).toHaveCount(0);
  await expect(section.getByText("Brak przypomnień.")).toBeVisible();
  expect((await stored(page)).reminders).toEqual([]);
});

test("lista przypomnień: nadchodzące według terminu, potem wyłączone i wykonane", async ({
  page,
}) => {
  await openPreview(page, { reminders: remindersFixture() });
  await openNotifications(page);
  const items = remindersSection(page).locator("li");
  await expect(items).toHaveCount(5);
  await expect(items.locator(".reminder-text")).toHaveText([
    "Zadzwonić do klienta w sprawie importu",
    "Uzupełnić czas w TS",
    "Przegląd tygodnia",
    "Sprawdzić dostawę",
    "Odpisać na zgłoszenie",
  ]);
  await expect(items.locator(".reminder-when")).toHaveText([
    "15:30",
    "jutro 08:45",
    "jutro 09:00",
    "wyłączone",
    "wykonane",
  ]);
  await expect(items.nth(0)).toContainText("Jednorazowo · Code review");
  await expect(items.nth(2)).toContainText("Co tydzień: Pn, Pt");
  // Wykonane jednorazowe można włączyć dopiero z nową godziną.
  await remindersSection(page)
    .getByLabel("Włączone: Odpisać na zgłoszenie")
    .click();
  await expect(remindersSection(page).getByRole("alert")).toHaveText(
    "Przypomnienie jednorazowe musi być w przyszłości.",
  );
  await expect(
    remindersSection(page).getByLabel("Włączone: Odpisać na zgłoszenie"),
  ).not.toBeChecked();
});

test("„Przypomnij…” z kafelka: formularz z zadaniem i znacznik 🔔 z godziną", async ({
  page,
}) => {
  await openPreview(page);
  const card = page.getByRole("button", {
    name: "Przedziały: Analiza importu",
    exact: true,
  });
  await expect(card.locator(".task-reminder")).toHaveCount(0);
  await card.click();
  await page
    .getByRole("group", { name: "Akcje zadania" })
    .getByRole("button", { name: "Przypomnij: Analiza importu" })
    .click();
  const form = dialog(page, "Dodaj przypomnienie");
  // Zadanie przypięte do przypomnienia.
  await expect(
    form.getByRole("combobox", { name: "Zadanie przypomnienia" }),
  ).toHaveValue("Analiza importu");
  await form.getByLabel("Treść", { exact: true }).fill("Wrócić do importu");
  await form.getByLabel("Godzina", { exact: true }).fill("15:00");
  await form.getByRole("button", { name: "Zapisz" }).click();
  await expect(form).toHaveCount(0);
  await expect(card.locator(".task-reminder")).toHaveText("🔔 15:00");
  await expect(card.locator(".task-reminder")).toHaveAttribute(
    "title",
    "Przypomnienie: Wrócić do importu · 15:00 · Jednorazowo",
  );
  expect((await stored(page)).reminders[0]).toMatchObject({
    text: "Wrócić do importu",
    taskId: ids.analysis,
    at: `${TODAY}T15:00`,
  });
  // Wyłączone przypomnienie zdejmuje znacznik.
  await openSettings(page, "Powiadomienia");
  await remindersSection(page)
    .getByLabel("Włączone: Wrócić do importu")
    .uncheck();
  await nav(page, "Zadania");
  await expect(
    page.getByRole("button", { name: "Przedziały: Analiza importu", exact: true })
      .locator(".task-reminder"),
  ).toHaveCount(0);
});

test("znacznik 🔔 pokazuje najbliższe przypomnienie zadania", async ({ page }) => {
  await openPreview(page, { reminders: remindersFixture() });
  await expect(
    page
      .getByRole("button", { name: "Przedziały: Code review", exact: true })
      .locator(".task-reminder"),
  ).toHaveText("🔔 15:30");
});

test("pole „Przypomnienie” w edytorze ręcznego spotkania", async ({ page }) => {
  await openPreview(page, { meetings: manualMeetings() });
  await nav(page, "Plan i historia");
  await nav(page, "Dzień");
  const chart = page.getByRole("group", { name: "Oś planu i rzeczywistości" });
  await chart
    .getByRole("button", {
      name: "Spotkanie: Telefon z magazynem · 11:00–11:30 · ręczne",
    })
    .click();
  const edit = dialog(page, "Edytuj spotkanie");
  const field = edit.getByLabel("Przypomnienie", { exact: true });
  await expect(field).toHaveValue("default");
  await expect(field.locator("option")).toHaveText([
    "Domyślne (5 min)",
    "1 min",
    "5 min",
    "10 min",
    "15 min",
    "30 min",
    "Bez przypomnienia",
  ]);
  await field.selectOption("10");
  await edit.getByRole("button", { name: "Zapisz" }).click();
  await expect(edit).toHaveCount(0);
  expect((await stored(page)).meetings).toEqual({ "manual:1": 10 });

  // Ponowne otwarcie pokazuje zapisaną wartość; „Bez przypomnienia” = null.
  await chart
    .getByRole("button", {
      name: "Spotkanie: Telefon z magazynem · 11:00–11:30 · ręczne",
    })
    .click();
  const again = dialog(page, "Edytuj spotkanie");
  await expect(again.getByLabel("Przypomnienie", { exact: true })).toHaveValue(
    "10",
  );
  await again.getByLabel("Przypomnienie", { exact: true }).selectOption("off");
  await again.getByRole("button", { name: "Zapisz" }).click();
  await expect(again).toHaveCount(0);
  expect((await stored(page)).meetings).toEqual({ "manual:1": null });

  // „Domyślne” usuwa nadpisanie.
  await chart
    .getByRole("button", {
      name: "Spotkanie: Telefon z magazynem · 11:00–11:30 · ręczne",
    })
    .click();
  const last = dialog(page, "Edytuj spotkanie");
  await expect(last.getByLabel("Przypomnienie", { exact: true })).toHaveValue(
    "off",
  );
  await last
    .getByLabel("Przypomnienie", { exact: true })
    .selectOption("default");
  await last.getByRole("button", { name: "Zapisz" }).click();
  expect((await stored(page)).meetings).toEqual({});
});

test("nowe ręczne spotkanie zapisuje przypomnienie razem ze spotkaniem", async ({
  page,
}) => {
  await openPreview(page);
  await nav(page, "Plan i historia");
  await nav(page, "Dzień");
  await page
    .getByRole("region", { name: "Plan kontra rzeczywistość" })
    .getByRole("button", { name: "Dodaj spotkanie" })
    .click();
  const add = dialog(page, "Dodaj spotkanie");
  await add.getByLabel("Temat", { exact: true }).fill("Telefon z klientem");
  await add.getByLabel("Przypomnienie", { exact: true }).selectOption("15");
  await add.getByRole("button", { name: "Zapisz" }).click();
  await expect(add).toHaveCount(0);
  const meetings = (await stored(page)).meetings as Record<string, number>;
  const [[id, minutes]] = Object.entries(meetings);
  expect(id).toMatch(/^manual:/);
  expect(minutes).toBe(15);
});

test("spotkanie z Outlooka: przypomnienie na osi dnia i w „Dzisiaj”", async ({
  page,
}) => {
  await openPreview(page, { calendar: calendarFixture() });
  await nav(page, "Plan i historia");
  await nav(page, "Dzień");
  const chart = page.getByRole("group", { name: "Oś planu i rzeczywistości" });
  // Spotkanie z Outlooka: zwykłe kliknięcie nic nie otwiera (tylko do odczytu),
  // prawy przycisk otwiera okno przypomnienia.
  const block = chart.getByRole("img", {
    name: "Spotkanie: Przegląd sprintu · 13:00–14:00 · wstępnie",
  });
  await block.click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await block.click({ button: "right" });
  const reminder = dialog(page, "Przypomnienie o spotkaniu");
  await expect(reminder).toContainText("Przegląd sprintu");
  await expect(reminder.getByLabel("Temat")).toHaveCount(0);
  const field = reminder.getByLabel("Przypomnienie", { exact: true });
  await expect(field).toHaveValue("default");
  await field.selectOption("30");
  await reminder.getByRole("button", { name: "Zapisz" }).click();
  await expect(reminder).toHaveCount(0);
  expect((await stored(page)).meetings).toEqual({ "m-review": 30 });

  // To samo w Zadania → Dzisiaj.
  await nav(page, "Zadania");
  await page.getByLabel("Dzisiaj: plan i praca").check();
  const today = page.getByRole("region", { name: "Spotkania dziś" });
  await today
    .getByRole("button", { name: "Przypomnienie o spotkaniu: Przegląd sprintu" })
    .click();
  const again = dialog(page, "Przypomnienie o spotkaniu");
  await expect(again.getByLabel("Przypomnienie", { exact: true })).toHaveValue(
    "30",
  );
  await again.getByLabel("Przypomnienie", { exact: true }).selectOption("off");
  await again.getByRole("button", { name: "Zapisz" }).click();
  await expect(again).toHaveCount(0);
  expect((await stored(page)).meetings).toEqual({ "m-review": null });
  // „Anuluj” nic nie zmienia.
  await today
    .getByRole("button", { name: "Przypomnienie o spotkaniu: Przegląd sprintu" })
    .click();
  await dialog(page, "Przypomnienie o spotkaniu")
    .getByRole("button", { name: "Anuluj" })
    .click();
  expect((await stored(page)).meetings).toEqual({ "m-review": null });
});

test("ustawienia domyślne przypomnień przed spotkaniem", async ({ page }) => {
  await openPreview(page, { meetings: manualMeetings() });
  await openSettings(page, "Powiadomienia");
  const enabled = page.getByLabel("Przypominaj przed spotkaniami", {
    exact: true,
  });
  const minutes = page.getByLabel("Ile minut przed spotkaniem", {
    exact: true,
  });
  await expect(enabled).toBeChecked();
  await expect(minutes).toHaveValue("5");
  await expect(minutes.locator("option")).toHaveText([
    "1 min",
    "5 min",
    "10 min",
    "15 min",
    "30 min",
  ]);
  await minutes.selectOption("15");
  await expect(minutes).toHaveValue("15");
  await enabled.uncheck();
  await expect(minutes).toBeDisabled();
  expect(
    await page.evaluate((k) => JSON.parse(localStorage.getItem(k)!), SETTINGS_KEY),
  ).toEqual({ meetingReminderMinutes: 15, meetingReminderEnabled: false });

  // Edytor spotkania podpowiada ustawienie domyślne.
  await nav(page, "Plan i historia");
  await nav(page, "Dzień");
  await page
    .getByRole("group", { name: "Oś planu i rzeczywistości" })
    .getByRole("button", {
      name: "Spotkanie: Telefon z magazynem · 11:00–11:30 · ręczne",
    })
    .click();
  const field = dialog(page, "Edytuj spotkanie").getByLabel("Przypomnienie", {
    exact: true,
  });
  await expect(field.locator("option").first()).toHaveText(
    "Domyślne (wyłączone)",
  );
});
