import { test, expect, type Page } from "@playwright/test";
import { matchingSections, sectionInMessage } from "../src/navigation";
import {
  redmineCertError,
  redmineKeyRejected,
  redmineNormalizeSite,
  redmineWriteDisabled,
} from "../src/store";
import {
  openPreview,
  REDMINE_KEY,
  REDMINE_SITE,
  SETTINGS_KEY,
} from "./fixtures/seed";
import { openSettings } from "./fixtures/settings";

/**
 * Ustawienia → Redmine (PLAN_1.9.md, R5) na symulowanym Redmine podglądu:
 * połączenie kluczem API (błąd złego klucza, certyfikat, `http`), test
 * połączenia, rozłączenie, śledzone projekty, przełączniki, listy statusów,
 * „Wstaw statusy z Redmine” i wyszukiwarka ustawień. Żaden test nie łączy się
 * z Redmine; adresy są przykładowe.
 */
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });

const OPENED_KEY = "todo-track-preview-redmine-opened-v1";

const panel = (page: Page) => page.getByRole("tabpanel");
const siteInput = (page: Page) => page.getByLabel("Adres Redmine", { exact: true });
const keyInput = (page: Page) => page.getByLabel("Klucz API", { exact: true });
const connectButton = (page: Page) =>
  panel(page).getByRole("button", { name: "Połącz", exact: true });
const account = (page: Page) => panel(page).locator(".redmine-account");
const tracked = (page: Page) =>
  panel(page).getByRole("list", { name: "Śledzone projekty" }).getByRole("listitem");

async function setRedmine(page: Page, state: Record<string, unknown>) {
  await page.evaluate(
    ([key, value]) => localStorage.setItem(key, value),
    [REDMINE_KEY, JSON.stringify(state)] as const,
  );
  await page.reload();
}
const redmineState = (page: Page) =>
  page.evaluate((k) => JSON.parse(localStorage.getItem(k) ?? "{}"), REDMINE_KEY);
const storedSettings = (page: Page) =>
  page.evaluate((k) => JSON.parse(localStorage.getItem(k) ?? "{}"), SETTINGS_KEY);

async function openRedmineSettings(page: Page) {
  await openSettings(page, "Redmine");
  await expect(
    panel(page).getByRole("heading", { name: "Redmine", exact: true }),
  ).toBeVisible();
}

test("sekcja Redmine: miejsce na liście, wyszukiwarka i komunikaty (moduł)", () => {
  const ids = matchingSections("").map((s) => s.id);
  expect(ids.indexOf("redmine")).toBe(ids.indexOf("jira") + 1);
  expect(ids.indexOf("notifications")).toBe(ids.indexOf("redmine") + 1);
  for (const q of ["Redmine", "klucz API", "sledzone projekty", "certyfikat"])
    expect(matchingSections(q).map((s) => s.id)).toEqual(["redmine"]);
  // Wspólne słowa z Jirą znajdują obie sekcje.
  expect(matchingSections("przypisane do mnie").map((s) => s.id)).toEqual([
    "jira",
    "redmine",
  ]);
  expect(sectionInMessage(redmineWriteDisabled)).toBe("redmine");
  expect(
    sectionInMessage("Połącz konto w Ustawieniach → Redmine albo poprzedź # znakiem \\."),
  ).toBe("redmine");
  // Normalizacja adresu jak `redmine::normalize_site`.
  expect(redmineNormalizeSite(" https://redmine.example.pl/ ")).toBe(REDMINE_SITE);
  expect(redmineNormalizeSite("https://redmine.example.pl/redmine/")).toBe(
    "https://redmine.example.pl/redmine",
  );
  expect(redmineNormalizeSite("http://127.0.0.1:3000")).toBe("http://127.0.0.1:3000");
  for (const bad of [
    "",
    "redmine.example.pl",
    "ftp://redmine.example.pl",
    "https://redmine.example.pl/?key=1",
    "https://redmine.example.pl/#x",
    "https://user:pass@redmine.example.pl",
  ])
    expect(() => redmineNormalizeSite(bad), bad).toThrow();
});

test("wyszukiwarka ustawień znajduje „Redmine” i „klucz API”", async ({ page }) => {
  await openPreview(page);
  await openSettings(page, "Czas pracy");
  const search = page.getByRole("searchbox", { name: "Szukaj ustawienia" });
  const tabs = page.getByRole("tablist", { name: "Sekcje ustawień" }).getByRole("tab");
  for (const q of ["Redmine", "klucz API", "Akceptuj niezaufany certyfikat"]) {
    await search.fill(q);
    await expect(tabs).toHaveText(["Redmine"]);
    await expect(
      panel(page).getByRole("heading", { name: "Redmine", exact: true }),
    ).toBeVisible();
  }
  await expect(keyInput(page)).toBeVisible();
  await search.fill("Czekam na");
  await expect(tabs).toHaveText(["Jira", "Redmine", "Powiadomienia"]);
});

test("połączone konto: stan, pusty klucz, test połączenia i jego błędy", async ({
  page,
}) => {
  await openPreview(page);
  await openRedmineSettings(page);
  await expect(account(page)).toHaveText("Połączono jako Jan Kowalski (jkowalski)");
  await expect(siteInput(page)).toHaveValue(REDMINE_SITE);
  // Klucz API nigdy nie wraca: pole puste, z podpisem o magazynie poświadczeń.
  await expect(keyInput(page)).toHaveValue("");
  await expect(keyInput(page)).toHaveAttribute("type", "password");
  await expect(keyInput(page)).toHaveAttribute("placeholder", "zapisany");
  await expect(panel(page)).toContainText(
    "Klucz API jest zapisany w magazynie poświadczeń Windows.",
  );
  await expect(connectButton(page)).toBeDisabled();

  await panel(page).getByRole("button", { name: "Testuj połączenie" }).click();
  await expect(panel(page).getByRole("status")).toHaveText(
    "Połączono jako Jan Kowalski (jkowalski), Redmine odpowiada.",
  );

  await setRedmine(page, { failTest: "Redmine zwrócił błąd HTTP 502. Spróbuj ponownie." });
  await openRedmineSettings(page);
  await panel(page).getByRole("button", { name: "Testuj połączenie" }).click();
  await expect(panel(page).getByRole("alert")).toHaveText(
    "Redmine zwrócił błąd HTTP 502. Spróbuj ponownie.",
  );

  // Osobny komunikat dla certyfikatu spoza magazynu Windows.
  await setRedmine(page, { certError: true });
  await openRedmineSettings(page);
  await panel(page).getByRole("button", { name: "Testuj połączenie" }).click();
  await expect(panel(page).getByRole("alert")).toHaveText(redmineCertError);
});

test("Połącz: zły klucz daje błąd, poprawny łączy z adresem z prefiksem ścieżki", async ({
  page,
}) => {
  await openPreview(page);
  await setRedmine(page, { connected: false, site: "" });
  await openRedmineSettings(page);
  await expect(account(page)).toHaveText("Nie połączono");
  await expect(
    panel(page).getByRole("button", { name: "Testuj połączenie" }),
  ).toHaveCount(0);
  await expect(siteInput(page)).toHaveValue("");
  await expect(siteInput(page)).toHaveAttribute(
    "placeholder",
    "https://redmine.firma.pl",
  );
  await expect(
    panel(page).getByText("Lista projektów pojawi się po połączeniu konta."),
  ).toBeVisible();
  await expect(panel(page).getByRole("button", { name: "Dodaj projekt…" })).toBeDisabled();
  // Bez połączenia nie ma „Wstaw statusy z Redmine”.
  await expect(
    panel(page).getByRole("button", { name: "Wstaw statusy z Redmine" }),
  ).toHaveCount(0);

  await siteInput(page).fill("https://redmine.example.pl/redmine/");
  await expect(connectButton(page)).toBeDisabled();
  await keyInput(page).fill("zly");
  await connectButton(page).click();
  await expect(panel(page).getByRole("alert")).toHaveText(redmineKeyRejected);
  await expect(keyInput(page)).toHaveValue("");
  await expect(account(page)).toHaveText("Nie połączono");

  await keyInput(page).fill("klucz z spacja");
  await connectButton(page).click();
  await expect(panel(page).getByRole("alert")).toHaveText(
    "Podaj klucz API Redmine (1–128 znaków, bez spacji).",
  );

  await keyInput(page).fill("0123456789abcdef");
  await keyInput(page).press("Enter");
  await expect(panel(page).getByRole("status")).toHaveText(
    "Połączono jako Jan Kowalski (jkowalski).",
  );
  await expect(account(page)).toHaveText("Połączono jako Jan Kowalski (jkowalski)");
  await expect(keyInput(page)).toHaveValue("");
  await expect(siteInput(page)).toHaveValue("https://redmine.example.pl/redmine");
  const state = await redmineState(page);
  expect(state.connected).toBe(true);
  expect(state.site).toBe("https://redmine.example.pl/redmine");
  // Klucz API nie trafia do localStorage.
  expect(
    await page.evaluate(() => JSON.stringify({ ...localStorage })),
  ).not.toContain("0123456789abcdef");
  // Nowy adres: śledzone projekty wybiera się od nowa.
  await expect(tracked(page)).toHaveCount(0);
});

test("ostrzeżenie przy http i link „Gdzie znaleźć klucz API”", async ({ page }) => {
  await openPreview(page);
  await setRedmine(page, { connected: false, site: "" });
  await openRedmineSettings(page);
  const warning = panel(page).getByText(
    "klucz API jest wysyłany bez szyfrowania",
  );
  const where = panel(page).getByRole("button", { name: "Gdzie znaleźć klucz API" });
  await expect(where).toBeDisabled();
  await expect(warning).toHaveCount(0);

  await siteInput(page).fill("http://redmine.example.pl/redmine");
  await expect(warning).toBeVisible();
  await expect(where).toBeEnabled();
  await where.click();
  expect(
    await page.evaluate((k) => JSON.parse(localStorage.getItem(k) ?? "[]"), OPENED_KEY),
  ).toEqual(["http://redmine.example.pl/redmine/my/account"]);

  await siteInput(page).fill("https://redmine.example.pl");
  await expect(warning).toHaveCount(0);
  await siteInput(page).fill("redmine.example.pl");
  await expect(where).toBeDisabled();
});

test("opcja niezaufanego certyfikatu pozwala połączyć się i jest zapamiętana", async ({
  page,
}) => {
  await openPreview(page);
  await setRedmine(page, { connected: false, site: "", certError: true });
  await openRedmineSettings(page);
  const option = page.getByLabel(
    "Akceptuj niezaufany certyfikat (tylko sieć wewnętrzna)",
  );
  // Zwinięte „Zaawansowane”, opcja domyślnie wyłączona.
  await expect(option).toBeHidden();
  await siteInput(page).fill("https://redmine.example.pl");
  await keyInput(page).fill("0123456789abcdef");
  await connectButton(page).click();
  await expect(panel(page).getByRole("alert")).toHaveText(redmineCertError);
  await expect(account(page)).toHaveText("Nie połączono");

  await panel(page).getByText("Zaawansowane", { exact: true }).click();
  await expect(option).not.toBeChecked();
  await expect(panel(page)).toContainText("ufa certyfikatom z magazynu Windows");
  await option.check();
  await expect(
    panel(page).getByText("Połączenie nie sprawdza tożsamości serwera."),
  ).toBeVisible();
  await keyInput(page).fill("0123456789abcdef");
  await connectButton(page).click();
  await expect(account(page)).toHaveText("Połączono jako Jan Kowalski (jkowalski)");
  expect((await redmineState(page)).acceptInvalidCert).toBe(true);

  // Po przeładowaniu opcja zostaje włączona (z konfiguracji), sekcja rozwinięta.
  await page.reload();
  await openRedmineSettings(page);
  await expect(option).toBeVisible();
  await expect(option).toBeChecked();
  await panel(page).getByRole("button", { name: "Testuj połączenie" }).click();
  await expect(panel(page).getByRole("status")).toContainText("Redmine odpowiada.");
});

test("Rozłącz pyta o potwierdzenie i zostawia adres", async ({ page }) => {
  await openPreview(page);
  await openRedmineSettings(page);
  await panel(page).getByRole("button", { name: "Rozłącz", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Rozłączyć konto Redmine?" });
  await expect(dialog).toContainText(
    "Klucz API zostanie usunięty z magazynu poświadczeń Windows.",
  );
  await dialog.getByRole("button", { name: "Rozłącz", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(account(page)).toHaveText("Nie połączono");
  await expect(siteInput(page)).toHaveValue(REDMINE_SITE);
  await expect(
    panel(page).getByRole("button", { name: "Testuj połączenie" }),
  ).toHaveCount(0);
  expect((await redmineState(page)).connected).toBe(false);
  // Widok Redmine prowadzi z powrotem do Ustawień → Redmine.
  await page.getByRole("button", { name: "Redmine", exact: true }).click();
  await page
    .getByRole("button", { name: "Połącz konto Redmine w Ustawieniach" })
    .click();
  await expect(page.getByRole("tab", { name: "Redmine", exact: true })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(account(page)).toHaveText("Nie połączono");
});

test("śledzone projekty: dodanie z listy z podprojektami i usunięcie", async ({
  page,
}) => {
  await openPreview(page);
  await openRedmineSettings(page);
  await expect(panel(page)).toContainText(
    "Widok Redmine pokazuje zgłoszenia tych projektów; nowe przypisania są sprawdzane we wszystkich projektach.",
  );
  await expect(tracked(page)).toHaveCount(1);
  await expect(tracked(page).first()).toContainText("Magazyn WMS");

  await panel(page).getByRole("button", { name: "Dodaj projekt…" }).click();
  const picker = page.getByRole("region", { name: "Wybór projektów Redmine" });
  const options = picker.locator(".redmine-project-options label");
  // Podprojekt z wcięciem pod rodzicem.
  await expect(options).toHaveText([
    /Magazyn WMS/,
    /WMS – integracje/,
    /Portal klienta/,
  ]);
  const indent = (i: number) =>
    options.nth(i).evaluate((el) => parseFloat(getComputedStyle(el).paddingLeft));
  expect(await indent(1)).toBeGreaterThan(await indent(0));
  expect(await indent(2)).toBe(await indent(0));
  await expect(picker.getByLabel("Magazyn WMS")).toBeChecked();
  await expect(picker.getByText("Wybrano 1 / 50")).toBeVisible();

  await picker.getByLabel("Szukaj projektu").fill("portal");
  await expect(options).toHaveText([/Portal klienta/]);
  await picker.getByLabel("Portal klienta").check();
  await picker.getByLabel("Szukaj projektu").fill("integr");
  await picker.getByLabel("WMS – integracje").check();
  await expect(picker.getByText("Wybrano 3 / 50")).toBeVisible();
  await picker.getByRole("button", { name: "Zapisz projekty" }).click();
  await expect(picker).toHaveCount(0);
  await expect(tracked(page)).toHaveCount(3);
  expect(
    (await redmineState(page)).selectedProjects.map((p: { id: number }) => p.id),
  ).toEqual([7, 9, 8]);

  await panel(page).getByRole("button", { name: "Usuń projekt Magazyn WMS" }).click();
  await expect(tracked(page)).toHaveCount(2);
  await panel(page).getByRole("button", { name: "Usuń projekt WMS – integracje" }).click();
  await expect(tracked(page)).toHaveText([/Portal klienta/]);
  expect(
    (await redmineState(page)).selectedProjects.map((p: { id: number }) => p.id),
  ).toEqual([9]);

  // Widok Redmine pokazuje teraz tylko Portal klienta.
  await page.getByRole("button", { name: "Redmine", exact: true }).click();
  await expect(
    page.getByRole("group", { name: "Projekt Redmine" }).getByRole("button"),
  ).toHaveText(["Portal klienta"]);
  // „Ustawienia → Redmine” w nagłówku widoku otwiera tę sekcję.
  await page.getByRole("button", { name: "Ustawienia → Redmine" }).click();
  await expect(tracked(page)).toHaveText([/Portal klienta/]);
});

test("przełączniki, statusy i prefiks gałęzi zapisują się z walidacją", async ({
  page,
}) => {
  await openPreview(page);
  await openRedmineSettings(page);
  const write = page.getByLabel("Pozwól aplikacji zmieniać zgłoszenia w Redmine");
  const ask = page.getByLabel("Pytaj o przesunięcie zgłoszenia przy starcie pracy");
  const move = page.getByLabel("Docelowy status przy starcie pracy");
  const initial = page.getByLabel("Statusy początkowe (jeden w wierszu)");
  const waiting = page.getByLabel("Statusy oznaczające „Czekam na” (jeden w wierszu)");
  const autoImport = page.getByLabel("Dodawaj nowe zgłoszenia przypisane do mnie");
  const prefix = page.getByLabel("Prefiks nazwy gałęzi");

  // Wartości domyślne.
  await expect(write).toBeChecked();
  await expect(ask).toBeChecked();
  await expect(move).toHaveValue("In Progress");
  await expect(initial).toHaveValue("New\nNowy");
  await expect(waiting).toHaveValue("Feedback");
  await expect(autoImport).toBeChecked();
  await expect(prefix).toHaveValue("feature/");

  await write.uncheck();
  await expect(ask).toBeDisabled();
  await expect(move).toBeDisabled();
  expect((await storedSettings(page)).redmineWrite).toBe(false);
  await write.check();
  await expect(ask).toBeEnabled();
  await ask.uncheck();
  await expect(ask).not.toBeChecked();
  await autoImport.uncheck();
  await expect(autoImport).not.toBeChecked();

  await move.fill("  W toku ");
  await move.blur();
  await expect(move).toHaveValue("W toku");
  await initial.fill("Nowy\n\nnowy\nOtwarte");
  await initial.blur();
  await expect(initial).toHaveValue("Nowy\nOtwarte");
  await waiting.fill("Feedback\nOczekuje na klienta");
  await waiting.blur();
  await prefix.fill("bugfix/");
  await prefix.blur();
  await expect(panel(page)).toContainText("bugfix/1234-krotki-tytul");

  const saved = await storedSettings(page);
  expect(saved).toMatchObject({
    redmineWrite: true,
    redmineAskMove: false,
    redmineAutoImport: false,
    redmineMoveStatus: "W toku",
    redmineNewStatuses: ["Nowy", "Otwarte"],
    waitingRedmineStatuses: ["Feedback", "Oczekuje na klienta"],
    redmineBranchPrefix: "bugfix/",
  });
  // Ustawienia Jiry bez zmian.
  expect(saved.jiraMoveStatus ?? "In Progress").toBe("In Progress");
  expect(saved.waitingJiraStatuses ?? ["Waiting", "Blocked"]).not.toContain(
    "Oczekuje na klienta",
  );

  // Pusty status docelowy jest odrzucany, pole wraca do zapisanej wartości.
  await move.fill(" ");
  await move.blur();
  await expect(page.getByRole("alert")).toHaveText(
    "Nazwa statusu w Redmine musi mieć od 1 do 80 znaków.",
  );
  await expect(move).toHaveValue("W toku");
  await prefix.fill("../x");
  await prefix.blur();
  await expect(prefix).toHaveValue("bugfix/");

  // Po przeładowaniu wartości zostają.
  await page.reload();
  await openRedmineSettings(page);
  await expect(ask).not.toBeChecked();
  await expect(autoImport).not.toBeChecked();
  await expect(move).toHaveValue("W toku");
  await expect(waiting).toHaveValue("Feedback\nOczekuje na klienta");
});

test("„Wstaw statusy z Redmine” dopisuje kliknięte statusy do wybranego pola", async ({
  page,
}) => {
  await openPreview(page);
  await openRedmineSettings(page);
  await panel(page).getByRole("button", { name: "Wstaw statusy z Redmine" }).click();
  const picker = page.getByRole("group", { name: "Statusy z Redmine" });
  await expect(picker.locator(".redmine-status-chips button")).toHaveText([
    "New",
    "In Progress",
    "Feedback",
    "Resolved (zamknięty)",
    "Closed (zamknięty)",
  ]);
  const initial = page.getByLabel("Statusy początkowe (jeden w wierszu)");
  const waiting = page.getByLabel("Statusy oznaczające „Czekam na” (jeden w wierszu)");
  const move = page.getByLabel("Docelowy status przy starcie pracy");

  // Domyślnie do statusów początkowych; powtórka nie dubluje.
  await picker.getByRole("button", { name: "Wstaw status Feedback" }).click();
  await expect(initial).toHaveValue("New\nNowy\nFeedback");
  await expect(panel(page).getByRole("status")).toHaveText(
    "Dodano „Feedback” do statusów początkowych.",
  );
  await picker.getByRole("button", { name: "Wstaw status New" }).click();
  await expect(panel(page).getByRole("status")).toHaveText(
    "„New” już jest na liście statusów początkowych.",
  );
  await expect(initial).toHaveValue("New\nNowy\nFeedback");

  await picker.getByLabel("Wstaw do").selectOption({ label: "statusów „Czekam na”" });
  await picker.getByRole("button", { name: "Wstaw status Resolved" }).click();
  await expect(waiting).toHaveValue("Feedback\nResolved");

  await picker
    .getByLabel("Wstaw do")
    .selectOption({ label: "statusu docelowego" });
  await picker.getByRole("button", { name: "Wstaw status In Progress" }).click();
  await expect(move).toHaveValue("In Progress");
  await move.fill("W toku");
  await move.blur();
  await picker.getByRole("button", { name: "Wstaw status In Progress" }).click();
  await expect(move).toHaveValue("In Progress");

  expect(await storedSettings(page)).toMatchObject({
    redmineNewStatuses: ["New", "Nowy", "Feedback"],
    waitingRedmineStatuses: ["Feedback", "Resolved"],
    redmineMoveStatus: "In Progress",
  });
  await panel(page).getByRole("button", { name: "Zwiń statusy z Redmine" }).click();
  await expect(picker).toHaveCount(0);
});
