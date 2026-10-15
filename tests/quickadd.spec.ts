import { test, expect } from "@playwright/test";
import {
  blockingError,
  complete,
  parse,
  removeToken,
  renderTitle,
} from "../src/quickadd";

// Testy jednostkowe parsera składni szybkiego dodawania (bez przeglądarki).
// „Dziś” to czwartek 15.10.2026, jak w tests/fixtures/seed.ts.
const TODAY = "2026-10-15";
const settings = { projects: ["WMS", "Foundation", "Zespół", "Moje projekty"] };

test("parser: every token at once", () => {
  const r = parse(
    "WMS-123 Eksport etykiet ~1h30 @pt do:20.10 ! #wms /review",
    TODAY,
    settings,
  );
  expect(r.errors).toEqual([]);
  expect(r.title).toBe("Eksport etykiet");
  expect(r.fields).toEqual({
    jiraKey: "WMS-123",
    estimateMinutes: 90,
    plannedDay: "2026-10-16",
    dueDay: "2026-10-20",
    priority: 1,
    // Pisownia z istniejącego projektu.
    project: "WMS",
    template: "review",
  });
  expect(r.tokens.map((t) => t.kind)).toEqual([
    "jira",
    "estimate",
    "planned",
    "due",
    "priority",
    "project",
    "template",
  ]);
  expect(r.tokens.map((t) => t.label)).toEqual([
    "Jira: WMS-123",
    "Szacunek: 1 h 30 min",
    "Plan: jutro 16.10",
    "Termin: wt 20.10",
    "Priorytet: wysoki",
    "Projekt: WMS",
    "Szablon: review",
  ]);
});

test("parser: tokens in any order give the same fields", () => {
  const variants = [
    "Raport ~45m @jutro do:pon !niski #Foundation",
    "#foundation !niski Raport do:pon @jutro ~45m",
    "@jutro ~45m Raport #Foundation do:pon !niski",
  ];
  for (const text of variants) {
    const r = parse(text, TODAY, settings);
    expect(r.errors, text).toEqual([]);
    expect(r.title, text).toBe("Raport");
    expect(r.fields, text).toEqual({
      estimateMinutes: 45,
      plannedDay: "2026-10-16",
      dueDay: "2026-10-19",
      priority: -1,
      project: "Foundation",
    });
  }
  // Klucz Jiry po innych zapisach, ale przed tytułem, też się liczy.
  expect(parse("~2h WMS-9 Poprawka", TODAY).fields).toEqual({
    estimateMinutes: 120,
    jiraKey: "WMS-9",
  });
  expect(parse("~2h WMS-9 Poprawka", TODAY).title).toBe("Poprawka");
});

test("parser: text without prefixes stays exactly as typed", () => {
  for (const text of [
    "Spotkanie 1h",
    "Spotkanie  z  zespołem",
    "Napisać do Janka @ 15",
    "Napisać do @Janka",
    "Zgłosić WMS-12 do testów",
    "~ # / @ \\",
    "e-mail a@b.pl, ~ok, C#",
  ]) {
    const r = parse(`  ${text} `, TODAY, settings);
    expect(r.tokens, text).toEqual([]);
    expect(r.fields, text).toEqual({});
    expect(r.title, text).toBe(text.trim());
  }
});

test("parser: backslash makes the next character literal", () => {
  const r = parse("Nauka C\\# \\#wms \\@pt \\~30m \\! \\WMS-1 a\\\\b", TODAY, settings);
  expect(r.tokens).toEqual([]);
  expect(r.fields).toEqual({});
  expect(r.title).toBe("Nauka C# #wms @pt ~30m ! WMS-1 a\\b");
  // Zwykły ukośnik wsteczny (ścieżka) zostaje.
  expect(parse("Log C:\\Users\\x ~1h", TODAY).title).toBe("Log C:\\Users\\x");
});

test("parser: week boundary and @mon on a Monday means next week", () => {
  // Niedziela 18.10 → poniedziałek 19.10.
  expect(parse("A @pon", "2026-10-18").fields.plannedDay).toBe("2026-10-19");
  // Piątek → poniedziałek następnego tygodnia; piątek → następny piątek.
  expect(parse("A @pon", "2026-10-16").fields.plannedDay).toBe("2026-10-19");
  expect(parse("A @pt", "2026-10-16").fields.plannedDay).toBe("2026-10-23");
  // Poniedziałek: @pon to za tydzień, @dziś to dziś.
  expect(parse("A @pon", "2026-10-12").fields.plannedDay).toBe("2026-10-19");
  expect(parse("A @dziś", "2026-10-12").fields.plannedDay).toBe("2026-10-12");
  expect(parse("A @nd", "2026-10-12").fields.plannedDay).toBe("2026-10-18");
  expect(parse("A @Środa", "2026-10-12").fields.plannedDay).toBe("2026-10-14");
});

test("parser: year boundary picks the nearest such day", () => {
  const eve = "2026-12-30";
  expect(parse("A @pt", eve).fields.plannedDay).toBe("2027-01-01");
  expect(parse("A @jutro", "2026-12-31").fields.plannedDay).toBe("2027-01-01");
  expect(parse("A @2.01", eve).fields.plannedDay).toBe("2027-01-02");
  expect(parse("A do:31.12", eve).fields.dueDay).toBe("2026-12-31");
  // Dzień już miniony w tym roku → przyszły rok; dziś → dziś.
  expect(parse("A @10.10", TODAY).fields.plannedDay).toBe("2027-10-10");
  expect(parse("A @15.10", TODAY).fields.plannedDay).toBe("2026-10-15");
  expect(parse("A @20.10.2027", TODAY).fields.plannedDay).toBe("2027-10-20");
  const label = parse("A @2.01", eve).tokens[0].label;
  expect(label).toBe("Plan: sob 02.01.2027");
});

test("parser: impossible dates and bad values are errors shown in a chip", () => {
  const r = parse("Raport @31.02", TODAY, settings);
  expect(r.fields).toEqual({});
  expect(r.title).toBe("Raport");
  expect(r.tokens).toEqual([
    expect.objectContaining({ kind: "error", text: "@31.02" }),
  ]);
  expect(r.errors).toEqual(["Nie ma dnia 31.02.2027."]);
  expect(parse("A @29.02", TODAY).errors).toHaveLength(1);
  expect(parse("A @29.02.2028", TODAY).fields.plannedDay).toBe("2028-02-29");
  expect(parse("A do:13.13", TODAY).errors).toHaveLength(1);
  expect(parse("A do:kiedyś", TODAY).errors).toHaveLength(1);
  expect(parse("A ~1h75", TODAY).errors).toHaveLength(1);
  expect(parse("A ~0m", TODAY).errors).toHaveLength(1);
  // Ten sam rodzaj dwa razy: błąd zamiast cichego nadpisania.
  const twice = parse("A ~1h ~2h", TODAY);
  expect(twice.fields.estimateMinutes).toBe(60);
  expect(twice.errors).toEqual(["Podano dwa razy: szacunek."]);
});

test("parser: estimates and priorities", () => {
  const est = (s: string) => parse(`A ~${s}`, TODAY).fields.estimateMinutes;
  expect(est("30m")).toBe(30);
  expect(est("2h")).toBe(120);
  expect(est("1h30")).toBe(90);
  expect(est("1h05m")).toBe(65);
  expect(parse("A !", TODAY).fields.priority).toBe(1);
  expect(parse("A !wysoki", TODAY).fields.priority).toBe(1);
  expect(parse("A !niski", TODAY).fields.priority).toBe(-1);
  expect(parse("Uwaga!!", TODAY).fields).toEqual({});
});

test("parser: projects, multi-word projects and Jira key alone", () => {
  const multi = parse("Plan #moje projekty ~1h", TODAY, settings);
  expect(multi.fields).toEqual({ project: "Moje projekty", estimateMinutes: 60 });
  expect(multi.title).toBe("Plan");
  // Nowy projekt: nazwa tak, jak wpisana.
  expect(parse("Plan #Nowy", TODAY, settings).fields.project).toBe("Nowy");
  const key = parse("WMS-123", TODAY);
  expect(key.title).toBe("");
  expect(key.fields).toEqual({ jiraKey: "WMS-123" });
  // Klucz w środku tytułu to zwykły tekst.
  expect(parse("Raport ~1h WMS-1", TODAY).fields.jiraKey).toBeUndefined();
});

test("parser: without connected Jira a key-like word stays in the title", () => {
  const off = { ...settings, jiraEnabled: false };
  const covid = parse("COVID-19 raport", TODAY, off);
  expect(covid.title).toBe("COVID-19 raport");
  expect(covid.fields).toEqual({});
  expect(covid.tokens).toEqual([]);
  const iso = parse("ISO-9001 audyt ~1h", TODAY, off);
  expect(iso.title).toBe("ISO-9001 audyt");
  expect(iso.fields).toEqual({ estimateMinutes: 60 });
  // Z połączoną Jirą ten sam zapis jest kluczem.
  expect(
    parse("ISO-9001 audyt", TODAY, { ...settings, jiraEnabled: true }).fields,
  ).toEqual({ jiraKey: "ISO-9001" });
});

test("parser: /template uses known names and blocks unknown ones", () => {
  const known = { ...settings, templates: ["review", "Daily"] };
  const r = parse("/Review Popraw raport", TODAY, known);
  // Pisownia z listy szablonów, bez błędu.
  expect(r.fields.template).toBe("review");
  expect(r.title).toBe("Popraw raport");
  expect(r.tokens.map((t) => t.label)).toEqual(["Szablon: review"]);
  expect(blockingError(r)).toBeUndefined();
  // Nieznana nazwa blokuje dodanie zamiast znikać z tytułu.
  const unknown = parse("/brak Popraw raport", TODAY, known);
  expect(unknown.fields.template).toBeUndefined();
  expect(blockingError(unknown)).toContain("Nie ma szablonu „brak”");
  // Bez listy (`templates` nie podano) nazwa jest przyjmowana jak wpisana.
  expect(parse("/review", TODAY, settings).fields.template).toBe("review");
  // Poprzedzone `\` zostaje w tytule i nie blokuje.
  const literal = parse("\\/brak Popraw raport", TODAY, known);
  expect(literal.title).toBe("/brak Popraw raport");
  expect(blockingError(literal)).toBeUndefined();
  // Błąd składni ma pierwszeństwo.
  expect(blockingError(parse("Raport @31.02 /brak", TODAY, known))).toBe(
    "Nie ma dnia 31.02.2027.",
  );
  // Tab uzupełnia nazwę szablonu.
  expect(complete("/re", 3, known)).toEqual({ text: "/review ", caret: 8 });
  expect(complete("WMS-1 /d", 8, known)).toEqual({
    text: "WMS-1 /Daily ",
    caret: 13,
  });
});

test("template title: {klucz}, {tytuł}, line title and Jira summary", () => {
  const jira = (key: string) =>
    key === "WMS-123" ? "Eksport etykiet" : undefined;
  const t = "Code review {klucz}: {tytuł}";
  expect(renderTitle(t, "WMS-123", "", jira)).toBe(
    "Code review WMS-123: Eksport etykiet",
  );
  // Tytuł z linii wygrywa z tytułem z Jiry.
  expect(renderTitle(t, "WMS-123", " Poprawki ", jira)).toBe(
    "Code review WMS-123: Poprawki",
  );
  expect(renderTitle(t, null, "", jira)).toBe("Code review");
  expect(renderTitle(t, null, "Raport", jira)).toBe("Code review: Raport");
  expect(() => renderTitle(t, "WMS-9", "", jira)).toThrow("WMS-9");
  expect(renderTitle("Daily", null, "z zespołem", jira)).toBe(
    "Daily z zespołem",
  );
  expect(renderTitle("", null, "Raport", jira)).toBe("Raport");
  expect(renderTitle("{klucz} – {tytul}", "WMS-123", "", jira)).toBe(
    "WMS-123 – Eksport etykiet",
  );
});

test("parser helpers: removing a chip token and Tab completion", () => {
  const text = "Raport ~1h @pt #wms";
  const r = parse(text, TODAY, settings);
  const est = r.tokens.find((t) => t.kind === "estimate")!;
  expect(removeToken(text, est)).toBe("Raport @pt #wms");
  const project = r.tokens.find((t) => t.kind === "project")!;
  expect(removeToken(text, project)).toBe("Raport ~1h @pt");

  expect(complete("Raport #ze", 10, settings)).toEqual({
    text: "Raport #Zespół ",
    caret: 15,
  });
  expect(complete("Raport #f ~1h", 9, settings)).toEqual({
    text: "Raport #Foundation ~1h",
    caret: 19,
  });
  expect(complete("Raport #x", 9, settings)).toBeNull();
  expect(complete("Raport", 6, settings)).toBeNull();
});
