import { test, expect, type Page } from "@playwright/test";
import { TODAY, agentSessions, at, ids, openPreview } from "./fixtures/seed";

/**
 * W1: czas agentów jako osobny tor. Dane z `fixtures/seed.ts` (`agentSessions()`):
 * dziś Claude Code nad „Analizą importu” 10:00–11:30 (Twój pomiar 08:30–10:30,
 * więc 30 min równolegle) i Codex bez zadania 11:00–11:40; w środę Claude nad
 * „Wdrożeniem etykiet”, we wtorek Codex nad „Code review” (bez Twojego czasu).
 */
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });

const nav = (page: Page, name: string) =>
  page.getByRole("button", { name, exact: true }).click();

test("czas agentów: suma bez podwójnego liczenia i część równoległa", async ({
  page,
}) => {
  await page.goto("/");
  const result = await page.evaluate(async () => {
    const m = await import("/src/agenttime.ts");
    const M = 60_000;
    const p = (id: number, taskId: number | null, a: number, b: number) => ({
      id,
      agent: "claude",
      folder: "C:\\repo",
      taskId,
      start: a * M,
      end: b * M,
    });
    const s = (id: number, taskId: number, a: number, b: number | null) => ({
      id,
      taskId,
      startedAt: a * M,
      endedAt: b === null ? null : b * M,
    });
    const all = m.agentTotals(
      [p(1, 1, 600, 660), p(2, 1, 630, 720), p(3, 2, 540, 600), p(4, null, 480, 500)],
      [s(1, 1, 690, 750), s(2, 2, 600, 660), s(3, 2, 585, null)],
      0,
      1440 * M,
      615 * M,
    );
    const clipped = m.agentTotals([p(1, 1, 600, 720)], [s(1, 1, 690, 750)], 705 * M, 1440 * M, 0);
    return {
      totals: all.totals.map((t) => [t.taskId, t.agentMs / M, t.parallelMs / M]),
      unmatched: all.unmatchedMs / M,
      clipped: clipped.totals.map((t) => [t.agentMs / M, t.parallelMs / M]),
    };
  });
  // Te same przypadki co `agent_time::tests::parallel_time_is_overlap_with_own_measurement_of_same_task`.
  expect(result).toEqual({
    totals: [
      [1, 120, 30],
      [2, 60, 15],
    ],
    unmatched: 20,
    clipped: [[15, 15]],
  });
});

test("oś dnia: tor Agenci pod Rzeczywistością, kolor zadania albo szary", async ({
  page,
}) => {
  await openPreview(page, { agentSessions: agentSessions() });
  await nav(page, "Plan i historia");
  await nav(page, "Dzień");
  const chart = page.getByRole("group", { name: "Oś planu i rzeczywistości" });
  await expect(page.locator(".dtl-labels")).toHaveText(
    /^\s*Plan\s*Rzeczywistość\s*Agenci\s*$/,
  );
  const matched = chart.getByRole("img", {
    name: "Agent: Claude Code · Analiza importu · 10:00–11:30 · wms",
  });
  await expect(matched).toBeVisible();
  await expect(matched).not.toHaveClass(/unmatched/);
  await expect(matched).toHaveAttribute("style", /--tl-/);
  const unmatched = chart.getByRole("img", {
    name: "Agent: Codex · bez zadania · 11:00–11:40 · narzedzia",
  });
  await expect(unmatched).toHaveClass(/unmatched/);
  // Tylko dzisiejsze okresy; środowy i wtorkowy są w innych dniach.
  await expect(chart.locator(".dtl-agent")).toHaveCount(2);
  // Tor leży pod Rzeczywistością.
  const work = chart.locator(".dtl-work").first();
  expect(Number(await matched.getAttribute("y"))).toBeGreaterThan(
    Number(await work.getAttribute("y")),
  );
  await expect(
    page.getByRole("list", { name: "Legenda osi dnia" }),
  ).toContainText("Agent bez zadania");
  // Tabela planu i rzeczywistości liczy tylko Twój pomiar.
  await expect(
    page
      .getByRole("table", { name: "Plan i rzeczywistość" })
      .locator("tbody tr", { hasText: "Analiza importu" }),
  ).toContainText("02:00");

  // Dzień bez pracy agentów: bez toru.
  await page.getByLabel("Dzień planu").fill("2026-10-12");
  await expect(page.locator(".dtl-labels")).toHaveText(
    /^\s*Plan\s*Rzeczywistość\s*$/,
  );
  await expect(chart.locator(".dtl-agent")).toHaveCount(0);

  // Bez danych agentów toru nie ma.
  await openPreview(page);
  await nav(page, "Plan i historia");
  await nav(page, "Dzień");
  await expect(page.locator(".dtl-labels")).toHaveText(
    /^\s*Plan\s*Rzeczywistość\s*$/,
  );
});

test("zakładka Miesiąc: kolumny Agent i Równolegle bez wpływu na sumy", async ({
  page,
}) => {
  const openMonth = async () => {
    await nav(page, "Raporty dzienne");
    await nav(page, "Miesiąc");
  };
  await openPreview(page);
  await openMonth();
  const summary = page.getByRole("region", { name: "Podsumowanie miesiąca" });
  const byTask = page.getByRole("region", { name: "Według zadania" });
  const total = await summary.locator(".report-total").textContent();
  const ownFooter = await byTask.locator("tfoot td").allTextContents();
  const projects = page
    .getByRole("region", { name: "Według projektu" })
    .locator("tbody");
  const projectText = await projects.textContent();
  await expect(byTask.locator("thead th")).toHaveText([
    "Zadanie",
    "Klucz Jiry",
    "Projekt",
    "Czas",
    "Godziny",
  ]);

  await openPreview(page, { agentSessions: agentSessions() });
  await openMonth();
  await expect(byTask.locator("thead th")).toHaveText([
    "Zadanie",
    "Klucz Jiry",
    "Projekt",
    "Czas",
    "Godziny",
    "Agent",
    "Równolegle",
  ]);
  const row = (title: string) =>
    byTask.locator("tbody tr", { hasText: title }).locator("td");
  await expect(row("Analiza importu").nth(5)).toHaveText("01:30");
  await expect(row("Analiza importu").nth(6)).toHaveText("00:30");
  await expect(row("Wdrożenie etykiet").nth(5)).toHaveText("01:00");
  await expect(row("Wdrożenie etykiet").nth(6)).toHaveText("00:00");
  // Zadanie tylko z czasem agentów: Twój czas 0, na końcu tabeli.
  await expect(row("Code review").nth(3)).toHaveText("00:00");
  await expect(row("Code review").nth(5)).toHaveText("00:45");
  await expect(byTask.locator("tbody tr").last()).toContainText("Code review");
  // Okres bez zadania nie trafia do tabeli zadań.
  await expect(byTask.locator("tfoot td").nth(4)).toHaveText("03:15");
  await expect(byTask.locator("tfoot td").nth(5)).toHaveText("00:30");
  // Suma, cel i projekty bez zmian.
  await expect(summary.locator(".report-total")).toHaveText(total!);
  expect((await byTask.locator("tfoot td").allTextContents()).slice(0, 4)).toEqual(
    ownFooter,
  );
  await expect(projects).toHaveText(projectText!);
  await expect(byTask).toContainText("nie wlicza się do celu");
});

test("rozwinięty kafelek: Ty, Agent i Równolegle; przypisanie folderu uzupełnia okresy", async ({
  page,
}) => {
  await openPreview(page, { agentSessions: agentSessions() });
  await page
    .getByRole("button", { name: "Przedziały: Analiza importu", exact: true })
    .click();
  const section = page.getByRole("region", {
    name: "Czas agentów: Analiza importu",
  });
  await expect(section.locator("tbody td")).toHaveText([
    "11:00",
    "01:30",
    "00:30",
  ]);
  // Zadanie bez pracy agentów nie ma sekcji.
  await page
    .getByRole("button", { name: "Przedziały: Analiza importu", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Przedziały: Spotkanie zespołu", exact: true })
    .click();
  await expect(
    page.getByRole("region", { name: "Czas agentów: Spotkanie zespołu" }),
  ).toHaveCount(0);

  // Przypisanie folderu uzupełnia okres bez zadania z ostatnich 7 dni.
  const after = await page.evaluate(
    async ([task, from, to]) => {
      const store = await import("/src/store.ts");
      await store.bindAgentFolder("c:/projekty/NARZEDZIA/", task);
      const time = await store.agentTime(from, to);
      return {
        periods: time.periods.map((p) => [p.id, p.taskId]),
        totals: time.totals.map((t) => [t.taskId, t.agentMs / 60_000]),
        unmatched: time.unmatchedMs,
      };
    },
    [ids.standup, at(TODAY, "00:00"), at("2026-10-16", "00:00")] as const,
  );
  expect(after).toEqual({
    periods: [
      [3, ids.analysis],
      [4, ids.standup],
    ],
    totals: [
      [ids.analysis, 90],
      [ids.standup, 40],
    ],
    unmatched: 0,
  });
  await expect(
    page.getByRole("region", { name: "Czas agentów: Spotkanie zespołu" }),
  ).toContainText("00:40");
});
