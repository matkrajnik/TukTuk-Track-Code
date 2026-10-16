// Service worker rozszerzenia Todo Track (Manifest V3).
//
// Śledzi aktywną kartę (tabs.onActivated, tabs.onUpdated, windows.onFocusChanged)
// i co 10 s odświeża stan. Adres i tytuł wysyła tylko dla domen z listy
// dozwolonych (opcje, domyślnie *.atlassian.net); dla innych stron wysyła samo
// „clear”. Wiadomości idą przez native messaging do todo-track-bridge.exe,
// a ten przekazuje je do działającej aplikacji. Otwarty port native messaging
// utrzymuje service worker przy życiu; alarm co 30 s budzi go po uśpieniu.
import {
  DEFAULT_DOMAINS,
  HOST,
  REFRESH_MS,
  browserName,
  describeReply,
  messageKey,
  pageMessage,
  withSource,
} from "./lib.js";

// Chrome i Edge (albo dwa profile) z rozszerzeniem działają naraz: aplikacja
// trzyma stan osobno dla każdej przeglądarki, a „clear” z tła go nie gasi.
const BROWSER = browserName(navigator.userAgentData, navigator.userAgent);

let port = null;
let domains = [...DEFAULT_DOMAINS];
let focused = true;
let lastKey = "";
let lastSent = 0;

async function loadDomains() {
  try {
    const stored = await chrome.storage.sync.get({ domains: DEFAULT_DOMAINS });
    domains = Array.isArray(stored.domains)
      ? stored.domains
      : [...DEFAULT_DOMAINS];
  } catch {
    domains = [...DEFAULT_DOMAINS];
  }
}

function setStatus(status) {
  chrome.storage.session
    .set({ status: { ...status, at: Date.now() } })
    .catch(() => {});
}

function connect() {
  if (port) return port;
  try {
    port = chrome.runtime.connectNative(HOST);
  } catch (e) {
    port = null;
    setStatus(describeReply(null, String(e?.message ?? e)));
    return null;
  }
  port.onMessage.addListener((reply) => setStatus(describeReply(reply)));
  port.onDisconnect.addListener(() => {
    const error = chrome.runtime.lastError?.message ?? "";
    port = null;
    lastKey = "";
    setStatus(describeReply(null, error));
  });
  return port;
}

async function current() {
  if (!focused)
    return withSource(
      pageMessage(null, domains, Date.now(), false),
      BROWSER,
      false,
    );
  try {
    const [tab] = await chrome.tabs.query({
      active: true,
      lastFocusedWindow: true,
    });
    return withSource(
      pageMessage(tab, domains, Date.now(), true),
      BROWSER,
      true,
    );
  } catch {
    return withSource(
      pageMessage(null, domains, Date.now(), false),
      BROWSER,
      true,
    );
  }
}

/** Wysyła stan; ten sam stan najwyżej raz na 5 s (odświeżenie co 10 s). */
async function send(force = false) {
  const msg = await current();
  const key = messageKey(msg);
  const now = Date.now();
  if (!force && key === lastKey && now - lastSent < 5_000) return;
  const p = connect();
  if (!p) return;
  try {
    p.postMessage(msg);
    lastKey = key;
    lastSent = now;
  } catch {
    port = null;
  }
}

chrome.tabs.onActivated.addListener(() => void send());
chrome.tabs.onUpdated.addListener((_id, change, tab) => {
  if (
    tab.active &&
    (change.url || change.title || change.status === "complete")
  )
    void send();
});
chrome.windows.onFocusChanged.addListener((id) => {
  focused = id !== chrome.windows.WINDOW_ID_NONE;
  void send(true);
});
chrome.storage.onChanged.addListener((changes, area) => {
  if (area === "sync" && changes.domains)
    void loadDomains().then(() => send(true));
});
chrome.runtime.onMessage.addListener((msg, _sender, reply) => {
  if (msg?.type === "reconnect") {
    lastKey = "";
    void send(true).then(() => reply({ ok: true }));
    return true;
  }
  return false;
});
chrome.alarms.onAlarm.addListener(() => void send(true));

async function init() {
  await loadDomains();
  try {
    const win = await chrome.windows.getLastFocused();
    focused = !!win?.focused;
  } catch {
    focused = true;
  }
  chrome.alarms.create("todo-track-refresh", { periodInMinutes: 0.5 });
  await send(true);
}

setInterval(() => void send(true), REFRESH_MS);
void init();
