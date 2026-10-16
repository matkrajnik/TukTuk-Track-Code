// Czyste funkcje rozszerzenia Todo Track (bez API przeglądarki), testowane
// w Playwright jako moduł (tests/browser-extension.spec.ts). Reguły domen są
// takie same jak w aplikacji (`browser::host_matches` w src-tauri/src/browser.rs).

/** Nazwa hosta native messaging (`native_msg::HOST_NAME`). */
export const HOST = "pl.todotrack.bridge";
/** Domyślna lista dozwolonych domen. */
export const DEFAULT_DOMAINS = Object.freeze(["*.atlassian.net"]);
/** Co ile rozszerzenie odświeża stan (aplikacja uznaje dane za ważne 30 s). */
export const REFRESH_MS = 10_000;

const label = /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/;

/**
 * Jeden wzorzec domeny z tekstu użytkownika: `*.firma.pl` (poddomeny) albo
 * `jira.firma.pl` (dokładnie ta nazwa). Akceptuje też wklejony adres
 * (`https://jira.firma.pl/browse/X`) i bierze z niego nazwę hosta. `null`, gdy
 * tekst nie jest domeną (np. sama gwiazdka, `*.pl`, adres IP bez kropki).
 * @param {string} raw
 * @returns {string | null}
 */
export function normalizePattern(raw) {
  let text = String(raw ?? "")
    .trim()
    .toLowerCase();
  if (!text) return null;
  text = text.replace(/^[a-z][a-z0-9+.-]*:\/\//, "");
  text = text.split(/[/?#]/)[0];
  text = text.replace(/:\d+$/, "").replace(/\.$/, "");
  const wildcard = text.startsWith("*.");
  const host = wildcard ? text.slice(2) : text;
  const labels = host.split(".");
  if (labels.length < 2 || !labels.every((l) => label.test(l))) return null;
  return wildcard ? `*.${host}` : host;
}

/**
 * Lista domen z pola opcji: wiersze, przecinki albo spacje; bez powtórzeń.
 * @param {string} text
 * @returns {{ domains: string[], invalid: string[] }}
 */
export function parseDomains(text) {
  const domains = [];
  const invalid = [];
  for (const part of String(text ?? "").split(/[\s,;]+/)) {
    if (!part) continue;
    const pattern = normalizePattern(part);
    if (!pattern) invalid.push(part);
    else if (!domains.includes(pattern)) domains.push(pattern);
  }
  return { domains, invalid };
}

/**
 * Czy host pasuje do wzorca: `*.domena` = poddomeny (bez samej domeny),
 * inaczej dokładnie ta nazwa. Wielkość liter bez znaczenia.
 * @param {string} host
 * @param {string} pattern
 */
export function hostMatches(host, pattern) {
  const h = String(host ?? "")
    .toLowerCase()
    .replace(/\.$/, "");
  const p = String(pattern ?? "")
    .trim()
    .toLowerCase();
  if (!h || !p) return false;
  if (p.startsWith("*.")) {
    const domain = p.slice(2);
    return domain.length > 0 && h.endsWith(`.${domain}`);
  }
  return h === p;
}

/**
 * Czy adres jest na liście dozwolonych (tylko http i https).
 * @param {string} url
 * @param {readonly string[]} domains
 */
export function urlAllowed(url, domains) {
  let parsed;
  try {
    parsed = new URL(url);
  } catch {
    return false;
  }
  if (parsed.protocol !== "http:" && parsed.protocol !== "https:") return false;
  return domains.some((d) => hostMatches(parsed.hostname, d));
}

/**
 * Wiadomość dla aplikacji z aktywnej karty. Dla domen z listy:
 * `{ type: "page", url, title, at }`. Dla każdej innej strony, karty incognito,
 * braku karty albo przeglądarki w tle: `{ type: "clear", at }` bez adresu
 * i tytułu, więc inne strony nie wychodzą z przeglądarki.
 * @param {{ url?: string, pendingUrl?: string, title?: string, incognito?: boolean } | null | undefined} tab
 * @param {readonly string[]} domains
 * @param {number} now
 * @param {boolean} [focused] przeglądarka na pierwszym planie
 */
export function pageMessage(tab, domains, now, focused = true) {
  const url = tab?.url || tab?.pendingUrl || "";
  if (!focused || !tab || tab.incognito || !urlAllowed(url, domains))
    return { type: "clear", at: now };
  return {
    type: "page",
    url,
    title: String(tab.title ?? "").slice(0, 2048),
    at: now,
  };
}

/** Klucz porównania wiadomości (bez czasu): ta sama treść nie idzie dwa razy z rzędu. */
export function messageKey(msg) {
  return msg.type === "page" ? `page ${msg.url} ${msg.title}` : msg.type;
}

/**
 * Stan połączenia dla opcji rozszerzenia z odpowiedzi hosta albo błędu portu.
 * @param {any} reply odpowiedź `todo-track-bridge` (`null`, gdy port się zamknął)
 * @param {string} [error] `chrome.runtime.lastError.message`
 * @returns {{ state: "connected" | "app-off" | "disabled" | "no-host" | "error", text: string, key: string | null }}
 */
export function describeReply(reply, error = "") {
  if (!reply) {
    if (/not found|nie znaleziono|forbidden|not allowed/i.test(error))
      return {
        state: "no-host",
        text: "Brak połączenia z Todo Track. Włącz „Rozszerzenie przeglądarki” w Ustawieniach → Czas pracy w aplikacji.",
        key: null,
      };
    return {
      state: "error",
      text: error
        ? `Połączenie przerwane: ${error}`
        : "Połączenie z Todo Track przerwane.",
      key: null,
    };
  }
  if (reply.connected === false)
    return {
      state: "app-off",
      text: "Todo Track nie działa (niepołączone). Uruchom aplikację.",
      key: null,
    };
  if (reply.enabled === false)
    return {
      state: "disabled",
      text: "Rozszerzenie jest wyłączone w Ustawieniach Todo Track.",
      key: null,
    };
  if (reply.ok === false)
    return {
      state: "error",
      text: reply.error || "Todo Track odrzucił wiadomość.",
      key: null,
    };
  const key = typeof reply.key === "string" ? reply.key : null;
  return {
    state: "connected",
    text: key
      ? `Połączono z Todo Track · rozpoznano ${key}`
      : "Połączono z Todo Track",
    key,
  };
}
