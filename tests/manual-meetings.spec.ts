import { test, expect, type Page } from "@playwright/test";
import {
  TODAY,
  at,
  calendarFixture,
  ids,
  manualMeetings,
  openPreview,
  stored,
} from "./fixtures/seed";

/**
 * B12: ręczne spotkania spoza Outlooka. Dane z `fixtures/seed.ts`: dziś
 * (czw. 15.10, teraz 12:00) pomiar 8:00–10:30, plan „Analiza importu” 10 h od
 * 8:00; `manualMeetings()` = telefon 11:00–11:30 z zadaniem „Code review”
 * i spotkanie u klienta 15:00–16:00. Kalendarz Microsoft domyślnie niepołączony.
 */
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });

const nav = (page: Page, name: string) =>
  page.getByRole("button", { name, exact: true }).click();
const openDay = async (page: Page) => {
  await nav(page, "Plan i historia");
  await nav(page, "Dzień");
};
const chart = (page: Page) =>
  page.getByRole("group", { name: "Oś planu i rzeczywistości" });
const dialog = (page: Page, name: string) =>
  page.getByRole("dialog", { name, exact: true });

test("oś dnia bez kalendarza: dodanie, edycja i usunięcie ręcznego spotkania", async ({
  page,
}) => {
  await openPreview(page);
  await openDay(page);
  await expect(page.locator(".dtl-labels")).toHaveText(
    /^\s*Plan\s*Rzeczywistość\s*$/,
  );

  await page
    .getByRole("region", { name: "Plan kontra rzeczywistość" })
    .getByRole("button", { name: "Dodaj spotkanie" })
    .click();
  const add = dialog(page, "Dodaj spotkanie");
  // Domyślnie najbliższa pełna godzina (teraz 12:00), 30 min, dziś.
  await expect(add.getByLabel("Dzień", { exact: true })).toHaveValue(TODAY);
  await expect(add.getByLabel("Od", { exact: true })).toHaveValue("12:00");
  await expect(add.getByLabel("Do", { exact: true })).toHaveValue("12:30");
  await expect(add.getByLabel("Typ", { exact: true })).toHaveValue("busy");
  // Walidacja: pusty temat blokuje formularz, koniec przed początkiem daje błąd.
  await add.getByLabel("Temat", { exact: true }).fill("Telefon z klientem");
  await add.getByLabel("Od", { exact: true }).fill("15:00");
  await add.getByLabel("Do", { exact: true }).fill("14:00");
  await add.getByRole("button", { name: "Zapisz" }).click();
  await expect(add.getByRole("alert")).toHaveText(
    "Error: Koniec spotkania musi być po początku.",
  );
  await add.getByLabel("Do", { exact: true }).fill("16:00");
  await add.getByRole("button", { name: "Zapisz" }).click();
  await expect(add).toHaveCount(0);

  // Tor „Spotkania” pojawia się bez połączonego kalendarza.
  await expect(page.locator(".dtl-labels")).toHaveText(
    /Spotkania\s*Plan\s*Rzeczywistość/,
  );
  const meeting = chart(page).getByRole("button", {
    name: "Spotkanie: Telefon z klientem · 15:00–16:00 · ręczne",
  });
  await expect(meeting).toHaveClass(/manual/);
  // Plan omija spotkanie.
  const plan = chart(page).locator(".dtl-plan");
  await expect(plan).toHaveCount(2);
  await expect(plan.nth(0)).toHaveAttribute(
    "aria-label",
    "Plan: Analiza importu · 08:00–15:00 · część 1 z 2",
  );
  await expect(plan.nth(1)).toHaveAttribute(
    "aria-label",
    "Plan: Analiza importu · 16:00–19:00 · część 2 z 2",
  );

  // Edycja: inne godziny, typ „Wstępnie” i zadanie.
  await meeting.click();
  const edit = dialog(page, "Edytuj spotkanie");
  await expect(edit.getByLabel("Temat", { exact: true })).toHaveValue("Telefon z klientem");
  await edit.getByLabel("Od", { exact: true }).fill("15:30");
  await edit.getByLabel("Do", { exact: true }).fill("16:30");
  await edit.getByLabel("Typ", { exact: true }).selectOption("tentative");
  await edit.getByRole("combobox", { name: "Zadanie spotkania" }).click();
  await edit.getByRole("option", { name: "Code review" }).click();
  await edit.getByRole("button", { name: "Zapisz" }).click();
  await expect(edit).toHaveCount(0);
  const changed = chart(page).getByRole("button", {
    name: "Spotkanie: Telefon z klientem · 15:30–16:30 · wstępnie · ręczne",
  });
  await expect(changed).toHaveClass(/tentative/);
  const saved = await page.evaluate(() =>
    JSON.parse(localStorage.getItem("todo-track-preview-meetings-v1")!),
  );
  expect(saved).toHaveLength(1);
  expect(saved[0]).toMatchObject({
    subject: "Telefon z klientem",
    showAs: "tentative",
    taskId: ids.review,
    start: at(TODAY, "15:30"),
    end: at(TODAY, "16:30"),
  });

  // Usunięcie z potwierdzeniem; „Anuluj” wraca do edycji bez zmian.
  await changed.click();
  await dialog(page, "Edytuj spotkanie")
    .getByRole("button", { name: "Usuń" })
    .click();
  const confirm = dialog(page, "Usunąć spotkanie?");
  await confirm.getByRole("button", { name: "Anuluj" }).click();
  await expect(confirm).toHaveCount(0);
  await dialog(page, "Edytuj spotkanie")
    .getByRole("button", { name: "Usuń" })
    .click();
  await dialog(page, "Usunąć spotkanie?")
    .getByRole("button", { name: "Usuń" })
    .click();
  await expect(chart(page).locator(".dtl-meeting")).toHaveCount(0);
  await expect(page.locator(".dtl-labels")).toHaveText(
    /^\s*Plan\s*Rzeczywistość\s*$/,
  );
});

test("oś dnia: klik w pusty tor Spotkania podstawia godzinę, „Cały dzień”", async ({
  page,
}) => {
  await openPreview(page, { meetings: manualMeetings() });
  await openDay(page);
  const track = page.locator(".dtl-meeting-track");
  const box = (await track.boundingBox())!;
  // Pusty fragment toru przy prawym końcu osi.
  await track.click({ position: { x: box.width - 4, y: box.height / 2 } });
  const add = dialog(page, "Dodaj spotkanie");
  await expect(add.getByLabel("Dzień", { exact: true })).toHaveValue(TODAY);
  const from = await add.getByLabel("Od", { exact: true }).inputValue();
  expect(from).toMatch(/^(1[6-9]|2[0-3]):(00|15|30|45)$/);
  // „Cały dzień” chowa godziny i zapisuje dzień od północy do północy.
  await add.getByLabel("Temat", { exact: true }).fill("Szkolenie BHP");
  await add.getByLabel("Cały dzień").check();
  await expect(add.getByLabel("Od", { exact: true })).toHaveCount(0);
  await add.getByLabel("Typ", { exact: true }).selectOption("workingElsewhere");
  await add.getByRole("button", { name: "Zapisz" }).click();
  await expect(add).toHaveCount(0);
  await expect(
    chart(page).getByRole("button", {
      name: "Spotkanie: Szkolenie BHP · cały dzień · ręczne",
    }),
  ).toHaveClass(/allday/);
  const saved = await page.evaluate(() =>
    JSON.parse(localStorage.getItem("todo-track-preview-meetings-v1")!),
  );
  expect(saved[2]).toMatchObject({
    allDay: true,
    start: at(TODAY, "00:00"),
    end: at("2026-10-16", "00:00"),
  });
});

test("pojemność: tablica tygodnia, cel w Zadaniach i Dzisiaj bez kalendarza", async ({
  page,
}) => {
  await openPreview(page, { meetings: manualMeetings() });
  // Pomiar 2:30; telefon 0:30 i klient 1:00 niepokryte pomiarem: cel 6:30.
  await expect(page.locator(".daily-target .target-text")).toHaveText(
    "02:30 / 06:30",
  );
  await page.getByLabel("Dzisiaj: plan i praca").check();
  const today = page.getByRole("region", { name: "Spotkania dziś" });
  // Telefon już minął (teraz 12:00): zostaje klient.
  await expect(today.locator("li")).toHaveCount(1);
  await expect(today.locator("li").first()).toHaveText(
    /15:00–16:00\s*Spotkanie u klienta\s*ręczne/,
  );
  // Edycja z listy i dodanie z „Dodaj spotkanie” obok listy.
  await today
    .getByRole("button", { name: "Edytuj spotkanie: Spotkanie u klienta" })
    .click();
  const edit = dialog(page, "Edytuj spotkanie");
  await edit.getByLabel("Temat", { exact: true }).fill("Klient: odbiór etykiet");
  await edit.getByRole("button", { name: "Zapisz" }).click();
  await expect(today.locator("li").first()).toContainText(
    "Klient: odbiór etykiet",
  );
  await today.getByRole("button", { name: "Dodaj spotkanie" }).click();
  const add = dialog(page, "Dodaj spotkanie");
  await add.getByLabel("Temat", { exact: true }).fill("Przegląd z zespołem");
  await add.getByLabel("Od", { exact: true }).fill("17:00");
  await add.getByLabel("Do", { exact: true }).fill("17:30");
  await add.getByRole("button", { name: "Zapisz" }).click();
  await expect(today.locator("li")).toHaveCount(2);
  await expect(page.locator(".daily-target .target-text")).toHaveText(
    "02:30 / 06:00",
  );

  // Tablica tygodnia: czwartek 8:00 − 2:00 spotkań = 6:00.
  await nav(page, "Plan i historia");
  await nav(page, "Tydzień");
  await nav(page, "Tablica");
  const thursday = page.getByRole("region", {
    name: "Czwartek 2026-10-15",
    exact: true,
  });
  await expect(thursday.locator(".week-estimate")).toHaveText("10:00 / 06:00");
  await expect(
    page.getByRole("progressbar", { name: "Pojemność: Czwartek 2026-10-15" }),
  ).toHaveAttribute("title", "Cel 8:00 · spotkania 2:00 · zostaje 6:00");

  // „Dodaj spotkanie” w nagłówku dnia: piątek 9:00–11:00.
  await page
    .getByRole("button", { name: "Dodaj spotkanie: Piątek 2026-10-16" })
    .click();
  const friday = dialog(page, "Dodaj spotkanie");
  await expect(friday.getByLabel("Dzień", { exact: true })).toHaveValue("2026-10-16");
  await friday.getByLabel("Temat", { exact: true }).fill("Wyjazd do klienta");
  await friday.getByLabel("Od", { exact: true }).fill("09:00");
  await friday.getByLabel("Do", { exact: true }).fill("11:00");
  await friday.getByRole("button", { name: "Zapisz" }).click();
  await expect(friday).toHaveCount(0);
  await expect(
    page
      .getByRole("region", { name: "Piątek 2026-10-16", exact: true })
      .locator(".week-estimate"),
  ).toHaveText("00:00 / 06:00");
});

test("Rano i Zamknięcie dnia: ręczne spotkanie z „Dodaj jako czas” z zadaniem", async ({
  page,
}) => {
  await openPreview(page, { meetings: manualMeetings() });
  await nav(page, "Plan i historia");
  await nav(page, "Rano");
  const morning = page.getByRole("list", { name: "Spotkania dziś" });
  await expect(morning.locator("li")).toHaveCount(2);
  await expect(morning.locator("li").nth(0)).toHaveText(
    /11:00–11:30\s*Telefon z magazynem\s*ręczne/,
  );
  await expect(
    page.getByText(/Cel 8:00 · spotkania 1:30 · zostaje 6:30/),
  ).toBeVisible();

  const before = (await stored(page)).sessions.length;
  await nav(page, "Zamknięcie dnia");
  const list = page.getByRole("list", { name: "Spotkania dnia" });
  await expect(list.locator("li")).toHaveCount(2);
  await expect(list.locator("li").nth(0)).toContainText("ręczne");
  // Klient (15:00) jeszcze się nie skończył.
  await expect(list.locator("li").nth(1)).toContainText(
    "Jeszcze się nie skończyło",
  );
  await list
    .getByRole("button", {
      name: "Dodaj jako czas: Telefon z magazynem 11:00–11:30",
    })
    .click();
  await expect(page.getByLabel("Początek ręcznego czasu")).toHaveValue(
    `${TODAY}T11:00`,
  );
  await expect(page.getByLabel("Koniec ręcznego czasu")).toHaveValue(
    `${TODAY}T11:30`,
  );
  // Zadanie ręcznego spotkania wypełnia się samo.
  await expect(
    page.getByRole("combobox", { name: "Zadanie do uzupełnienia" }),
  ).toHaveValue("Code review");
  await page.getByRole("button", { name: "Zapisz ręczny czas" }).click();
  await expect
    .poll(async () => (await stored(page)).sessions.length)
    .toBe(before + 1);
  const added = (await stored(page)).sessions.find(
    (s: { startedAt: number }) => s.startedAt === at(TODAY, "11:00"),
  );
  expect(added).toMatchObject({ taskId: ids.review, endedAt: at(TODAY, "11:30") });
});

test("tytuły: „Pokazuj tytuły spotkań” dotyczy tylko Outlooka; rozłączenie nie kasuje ręcznych", async ({
  page,
}) => {
  await openPreview(page, {
    calendar: calendarFixture(),
    meetings: manualMeetings(),
    settings: { calendarShowTitles: false },
  });
  await openDay(page);
  await expect(
    chart(page).getByRole("img", { name: "Spotkanie: Spotkanie · 09:15–09:30" }),
  ).toBeVisible();
  await expect(
    chart(page).getByRole("button", {
      name: "Spotkanie: Telefon z magazynem · 11:00–11:30 · ręczne",
    }),
  ).toBeVisible();
  // Spotkanie z Outlooka jest tylko do odczytu: klik nie otwiera edycji.
  await chart(page)
    .getByRole("img", { name: "Spotkanie: Spotkanie · 09:15–09:30" })
    .click();
  await expect(page.getByRole("dialog")).toHaveCount(0);

  // „Rozłącz” kasuje spotkania z Outlooka, ręczne zostają.
  await page.evaluate(async () => {
    const store = await import("/src/store.ts");
    await store.calendarDisconnect();
  });
  await expect(chart(page).getByRole("img")).toHaveCount(0);
  await expect(chart(page).locator(".dtl-meeting.manual")).toHaveCount(2);
});

test("podgląd: walidacja komend ręcznych spotkań jak w meetings.rs", async ({
  page,
}) => {
  await openPreview(page, { calendar: calendarFixture() });
  const errors = await page.evaluate(
    async ([start, end, nextDay]) => {
      const store = await import("/src/store.ts");
      const base = {
        start,
        end,
        subject: "Telefon",
        showAs: "busy",
        allDay: false,
        taskId: null as number | null,
      };
      const error = async (p: Promise<unknown>) =>
        p.then(
          () => "ok",
          (e: Error) => e.message,
        );
      return [
        await error(store.meetingAdd({ ...base, subject: "  " })),
        await error(store.meetingAdd({ ...base, subject: "x".repeat(201) })),
        await error(store.meetingAdd({ ...base, subject: "x".repeat(200) })),
        await error(store.meetingAdd({ ...base, showAs: "free" })),
        await error(store.meetingAdd({ ...base, end: start })),
        await error(store.meetingAdd({ ...base, end: nextDay })),
        await error(store.meetingAdd({ ...base, taskId: 999 })),
        await error(store.meetingAdd({ ...base, taskId: 7 })),
        await error(store.meetingUpdate("m-daily", base)),
        await error(store.meetingDelete("m-daily")),
        await error(store.meetingDelete("manual:404")),
      ];
    },
    [at(TODAY, "10:00"), at(TODAY, "10:30"), at("2026-10-16", "01:00")],
  );
  expect(errors).toEqual([
    "Podaj temat spotkania.",
    "Temat spotkania może mieć najwyżej 200 znaków.",
    "ok",
    "Niepoprawny typ spotkania.",
    "Koniec spotkania musi być po początku.",
    "Początek i koniec spotkania muszą być tego samego dnia. Dla dłuższego zaznacz „Cały dzień”.",
    "Wybrane zadanie nie istnieje.",
    "Wybrane zadanie nie istnieje.",
    "Spotkania z Outlooka są tylko do odczytu.",
    "Spotkania z Outlooka są tylko do odczytu.",
    "Nie ma takiego spotkania.",
  ]);
  // Spotkania z obu źródeł według początku, z polem `source`.
  const sources = await page.evaluate(async ([from, to]) => {
    const store = await import("/src/store.ts");
    return (await store.calendarEvents(from, to)).map(
      (e) => `${e.source}:${e.subject}`,
    );
  }, [at(TODAY, "09:00"), at(TODAY, "11:00")]);
  expect(sources).toEqual(["outlook:Daily WMS", `manual:${"x".repeat(200)}`]);
});

test("Zadania → Dzisiaj bez kalendarza i bez spotkań: „Dodaj spotkanie”, koniec o północy", async ({
  page,
}) => {
  await openPreview(page);
  await page.getByLabel("Dzisiaj: plan i praca").check();
  // Sekcja widoczna także bez połączonego kalendarza i bez żadnego spotkania.
  const today = page.getByRole("region", { name: "Spotkania dziś" });
  await expect(today.locator("li")).toHaveCount(0);
  await expect(today.locator(".hint")).toHaveText(
    "Brak nadchodzących spotkań dziś.",
  );
  await today.getByRole("button", { name: "Dodaj spotkanie" }).click();
  const add = dialog(page, "Dodaj spotkanie");
  await add.getByLabel("Temat", { exact: true }).fill("Wdrożenie nocne");
  await add.getByLabel("Od", { exact: true }).fill("23:30");
  // „Do” = 00:00 to północ kończąca dzień, nie początek dnia.
  await add.getByLabel("Do", { exact: true }).fill("00:00");
  await add.getByRole("button", { name: "Zapisz" }).click();
  await expect(add).toHaveCount(0);
  await expect(today.locator("li")).toHaveCount(1);
  await expect(today.locator("li").first()).toHaveText(
    /23:30–00:00\s*Wdrożenie nocne\s*ręczne/,
  );
  const saved = async () =>
    page.evaluate(() =>
      JSON.parse(localStorage.getItem("todo-track-preview-meetings-v1")!),
    );
  expect((await saved())[0]).toMatchObject({
    start: at(TODAY, "23:30"),
    end: at("2026-10-16", "00:00"),
  });

  // Ponowny zapis spotkania kończącego się o północy (Do = 00:00) przechodzi.
  await today
    .getByRole("button", { name: "Edytuj spotkanie: Wdrożenie nocne" })
    .click();
  const edit = dialog(page, "Edytuj spotkanie");
  await expect(edit.getByLabel("Do", { exact: true })).toHaveValue("00:00");
  await edit.getByLabel("Temat", { exact: true }).fill("Wdrożenie produkcyjne");
  await edit.getByRole("button", { name: "Zapisz" }).click();
  await expect(edit).toHaveCount(0);
  expect((await saved())[0]).toMatchObject({
    subject: "Wdrożenie produkcyjne",
    end: at("2026-10-16", "00:00"),
  });
});

test("edycja spotkania z zadaniem w koszu: „Bez zadania” z podpowiedzią i zapis", async ({
  page,
}) => {
  const [, client] = manualMeetings();
  await openPreview(page, {
    meetings: [{ ...client, taskId: ids.trashed }],
  });
  await openDay(page);
  await chart(page)
    .getByRole("button", {
      name: "Spotkanie: Spotkanie u klienta · 15:00–16:00 · ręczne",
    })
    .click();
  const edit = dialog(page, "Edytuj spotkanie");
  await expect(edit.locator(".meeting-task-gone")).toHaveText(
    "Zadanie tego spotkania jest w koszu. Wybierz inne albo zapisz bez zadania.",
  );
  await expect(
    edit.getByRole("combobox", { name: "Zadanie spotkania" }),
  ).toHaveValue("Bez zadania");
  await edit.getByLabel("Do", { exact: true }).fill("16:30");
  await edit.getByRole("button", { name: "Zapisz" }).click();
  await expect(edit).toHaveCount(0);
  const saved = await page.evaluate(() =>
    JSON.parse(localStorage.getItem("todo-track-preview-meetings-v1")!),
  );
  expect(saved[0]).toMatchObject({
    taskId: null,
    end: at(TODAY, "16:30"),
  });
});
