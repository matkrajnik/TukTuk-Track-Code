import { test, expect, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import {
  DEFAULT_DOMAINS,
  HOST,
  describeReply,
  hostMatches,
  messageKey,
  normalizePattern,
  pageMessage,
  parseDomains,
  urlAllowed,
} from "../browser-extension/lib.js";
import { ids, NOW, openPreview } from "./fixtures/seed";
import { backend, openDesktop } from "./fixtures/tauri";
import { openSettings } from "./fixtures/settings";

/**
 * Rozszerzenie przeglądarki (PLAN_1.7.md, W4). Czyste funkcje rozszerzenia
 * (`browser-extension/lib.js`) testujemy jako moduł; rozpoznanie adresów, ramkę
 * native messaging, wygasanie sygnału i rejestrację sprawdzają testy Rust
 * (`browser.rs`, `foreground.rs`). Na końcu Ustawienia i baner ze źródłem
 * „Przeglądarka” przez podgląd i zastępcze IPC.
 */
test.use({ timezoneId: "Europe/Warsaw" });

const domains = [...DEFAULT_DOMAINS];

test("filtr domen: wzorce, lista z opcji i dopasowanie hosta", () => {
  expect(DEFAULT_DOMAINS).toEqual(["*.atlassian.net"]);
  expect(HOST).toBe("pl.todotrack.bridge");
  expect(normalizePattern(" *.Atlassian.NET ")).toBe("*.atlassian.net");
  expect(normalizePattern("https://jira.firma.pl:8443/browse/X-1")).toBe(
    "jira.firma.pl",
  );
  expect(normalizePattern("jira.firma.pl.")).toBe("jira.firma.pl");
  for (const bad of [
    "*",
    "*.",
    "*.pl",
    "localhost",
    "jira firma",
    "a..b",
    "-a.pl",
    "",
  ])
    expect(normalizePattern(bad), bad).toBeNull();

  expect(
    parseDomains(
      "*.atlassian.net\njira.firma.pl, JIRA.firma.pl ;  zła*domena\n\n",
    ),
  ).toEqual({
    domains: ["*.atlassian.net", "jira.firma.pl"],
    invalid: ["zła*domena"],
  });
  expect(parseDomains("")).toEqual({ domains: [], invalid: [] });

  expect(hostMatches("asistonwms.atlassian.net", "*.atlassian.net")).toBe(true);
  expect(hostMatches("A.B.ATLASSIAN.net.", "*.atlassian.net")).toBe(true);
  expect(hostMatches("atlassian.net", "*.atlassian.net")).toBe(false);
  expect(hostMatches("evilatlassian.net", "*.atlassian.net")).toBe(false);
  expect(hostMatches("jira.firma.pl", "jira.firma.pl")).toBe(true);
  expect(hostMatches("x.jira.firma.pl", "jira.firma.pl")).toBe(false);

  expect(urlAllowed("https://x.atlassian.net/browse/WMS-1", domains)).toBe(
    true,
  );
  expect(urlAllowed("http://x.atlassian.net/", domains)).toBe(true);
  for (const url of [
    "https://atlassian.net.evil.example/browse/WMS-1",
    "https://github.com/firma/wms/pull/1",
    "file:///C:/x.atlassian.net/browse/WMS-1",
    "chrome://extensions",
    "nie adres",
  ])
    expect(urlAllowed(url, domains), url).toBe(false);
});

test("dane z karty: tylko dozwolone domeny wychodzą z przeglądarki", () => {
  const at = 1_700_000_000_000;
  const tab = {
    url: "https://x.atlassian.net/jira/software/c/projects/WMS/boards/1?selectedIssue=WMS-45",
    title: "Tablica WMS - Jira",
  };
  expect(pageMessage(tab, domains, at)).toEqual({
    type: "page",
    url: tab.url,
    title: "Tablica WMS - Jira",
    at,
  });
  // Inna strona, karta incognito, brak karty, przeglądarka w tle: samo „clear”.
  const clear = { type: "clear", at };
  expect(
    pageMessage(
      { url: "https://mail.example/WMS-1", title: "WMS-1 poczta" },
      domains,
      at,
    ),
  ).toEqual(clear);
  expect(pageMessage({ ...tab, incognito: true }, domains, at)).toEqual(clear);
  expect(pageMessage(null, domains, at)).toEqual(clear);
  expect(pageMessage(tab, domains, at, false)).toEqual(clear);
  // Karta w trakcie ładowania (pendingUrl) i bardzo długi tytuł.
  expect(
    pageMessage(
      {
        url: "",
        pendingUrl: "https://x.atlassian.net/browse/WMS-2",
        title: "x".repeat(5000),
      },
      domains,
      at,
    ),
  ).toMatchObject({
    type: "page",
    url: "https://x.atlassian.net/browse/WMS-2",
  });
  expect(
    (pageMessage({ ...tab, title: "x".repeat(5000) }, domains, at) as any)
      .title,
  ).toHaveLength(2048);
  // Własna domena z opcji.
  expect(
    pageMessage(
      { url: "https://jira.firma.pl/browse/OPS-1", title: "" },
      ["jira.firma.pl"],
      at,
    ).type,
  ).toBe("page");
  // Ten sam stan nie idzie dwa razy z rzędu (klucz bez czasu).
  expect(messageKey(pageMessage(tab, domains, 1))).toBe(
    messageKey(pageMessage(tab, domains, 2)),
  );
  expect(messageKey(clear)).toBe("clear");
});

test("stan połączenia z odpowiedzi hosta", () => {
  expect(
    describeReply({ ok: true, connected: true, enabled: true, key: "WMS-45" }),
  ).toEqual({
    state: "connected",
    text: "Połączono z Todo Track · rozpoznano WMS-45",
    key: "WMS-45",
  });
  expect(describeReply({ ok: true, connected: true, enabled: true }).text).toBe(
    "Połączono z Todo Track",
  );
  expect(
    describeReply({ ok: false, connected: false, status: "niepołączone" })
      .state,
  ).toBe("app-off");
  expect(
    describeReply({ ok: false, connected: true, enabled: false }).state,
  ).toBe("disabled");
  expect(
    describeReply({ ok: false, error: "Za długi adres albo tytuł." }).text,
  ).toBe("Za długi adres albo tytuł.");
  expect(
    describeReply(null, "Specified native messaging host not found.").state,
  ).toBe("no-host");
  expect(describeReply(null, "").state).toBe("error");
});

test("manifest: Manifest V3, stały klucz i minimalne uprawnienia", () => {
  const manifest = JSON.parse(
    readFileSync(
      new URL("../browser-extension/manifest.json", import.meta.url),
      "utf8",
    ),
  );
  expect(manifest.manifest_version).toBe(3);
  expect(manifest.key).toMatch(/^MIIBIjAN[A-Za-z0-9+/=]+$/);
  expect(manifest.background).toEqual({
    service_worker: "background.js",
    type: "module",
  });
  expect([...manifest.permissions].sort()).toEqual([
    "alarms",
    "nativeMessaging",
    "storage",
    "tabs",
  ]);
  expect(manifest.host_permissions).toBeUndefined();
});

test("moduł działa też w przeglądarce (bez bundlera)", async ({ page }) => {
  await page.goto("/");
  const result = await page.evaluate(async () => {
    const lib = await import("/browser-extension/lib.js" as string);
    return lib.pageMessage(
      { url: "https://x.atlassian.net/browse/WMS-1", title: "t" },
      lib.DEFAULT_DOMAINS,
      5,
    );
  });
  expect(result).toEqual({
    type: "page",
    url: "https://x.atlassian.net/browse/WMS-1",
    title: "t",
    at: 5,
  });
});

const status = (page: Page) =>
  page.getByRole("status", { name: "Stan rozszerzenia przeglądarki" });

test("Ustawienia → Czas pracy: wyłącznik i stan połączenia", async ({
  page,
}) => {
  await openPreview(page);
  await openSettings(page, "Czas pracy");
  const toggle = page.getByLabel("Rozszerzenie przeglądarki (Chrome i Edge)");
  await expect(toggle).not.toBeChecked();
  await expect(status(page)).toHaveCount(0);
  await toggle.check();
  await expect(status(page)).toHaveText(
    "Niepołączone · jeszcze bez sygnału ze zgłoszenia.",
  );
  // Rozszerzenie odezwało się 5 s temu z kluczem z tablicy.
  const now = Date.parse(NOW);
  await page.evaluate(
    (data) =>
      localStorage.setItem(
        "todo-track-preview-browser-v1",
        JSON.stringify(data),
      ),
    { lastMessage: now - 5_000, lastKey: "WMS-45", lastKeyAt: now - 5_000 },
  );
  await page.reload();
  await openSettings(page, "Czas pracy");
  await expect(
    page.getByLabel("Rozszerzenie przeglądarki (Chrome i Edge)"),
  ).toBeChecked();
  await expect(status(page)).toContainText(
    "Połączone · ostatni sygnał: WMS-45",
  );
  // Instrukcja instalacji z identyfikatorem.
  await page
    .getByRole("button", { name: "Jak zainstalować rozszerzenie?" })
    .click();
  await expect(page.locator("#browser-extension-guide")).toContainText(
    "cecpcpkmmmkgfdklnkkkigofkcibnhbl",
  );
  // Wyszukiwarka ustawień znajduje sekcję po nazwie.
  await page
    .getByRole("searchbox", { name: "Szukaj ustawienia" })
    .fill("rozszerzenie przegladarki");
  await expect(
    page.getByRole("tablist", { name: "Sekcje ustawień" }).getByRole("tab"),
  ).toHaveText(["Czas pracy"]);
});

test("baner ze źródłem „Przeglądarka”: „Pracujesz teraz przy WMS-12?”", async ({
  page,
}) => {
  const now = Date.parse(NOW);
  await openDesktop(page, {
    settings: { browserExtension: true },
    suggestions: [
      {
        task: {
          taskId: ids.analysis,
          title: "Analiza importu",
          key: "WMS-12",
          source: "task",
          done: false,
        },
        agents: [
          {
            kind: "browser",
            name: "Google Chrome",
            cwd: "",
            since: now - 180_000,
          },
        ],
      },
    ],
  });
  const banner = page.getByRole("region", { name: "Propozycja przełączenia" });
  await expect(banner.getByRole("heading")).toHaveText(
    "Pracujesz teraz przy WMS-12?",
  );
  await expect(banner).toContainText(
    "Przeglądarka · Google Chrome pokazuje „Analiza importu”.",
  );
  await banner.getByRole("button", { name: "Przełącz" }).click();
  await expect(banner).toHaveCount(0);
  const calls = await backend<any[]>(page, "calls");
  expect(calls.find((c) => c.cmd === "agent_switch")?.args).toEqual({
    taskId: ids.analysis,
    key: "WMS-12",
  });
});
