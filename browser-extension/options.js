// Opcje rozszerzenia: lista dozwolonych domen i stan połączenia z TukTuk Track.
import { DEFAULT_DOMAINS, parseDomains } from "./lib.js";

const field = document.getElementById("domains");
const status = document.getElementById("status");
const message = document.getElementById("message");

function showStatus(s) {
  if (!s) {
    status.textContent = "Jeszcze bez odpowiedzi z TukTuk Track.";
    status.dataset.state = "";
    return;
  }
  const at = s.at ? ` (${new Date(s.at).toLocaleTimeString("pl-PL")})` : "";
  status.textContent = s.text + at;
  status.dataset.state = s.state;
}

async function load() {
  const stored = await chrome.storage.sync.get({ domains: DEFAULT_DOMAINS });
  field.value = (
    Array.isArray(stored.domains) ? stored.domains : DEFAULT_DOMAINS
  ).join("\n");
  const session = await chrome.storage.session.get("status").catch(() => ({}));
  showStatus(session.status);
}

document.getElementById("save").addEventListener("click", async () => {
  const { domains, invalid } = parseDomains(field.value);
  if (invalid.length) {
    message.textContent = `To nie są domeny: ${invalid.join(", ")}.`;
    return;
  }
  if (!domains.length) {
    message.textContent = "Podaj co najmniej jedną domenę.";
    return;
  }
  await chrome.storage.sync.set({ domains });
  field.value = domains.join("\n");
  message.textContent = "Zapisano.";
});

document.getElementById("reset").addEventListener("click", async () => {
  await chrome.storage.sync.set({ domains: [...DEFAULT_DOMAINS] });
  field.value = DEFAULT_DOMAINS.join("\n");
  message.textContent = "Przywrócono *.atlassian.net.";
});

document.getElementById("reconnect").addEventListener("click", async () => {
  status.textContent = "Sprawdzam połączenie…";
  await chrome.runtime.sendMessage({ type: "reconnect" }).catch(() => {});
});

chrome.storage.onChanged.addListener((changes, area) => {
  if (area === "session" && changes.status) showStatus(changes.status.newValue);
});

void load();
