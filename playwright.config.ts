import { defineConfig } from "@playwright/test";

// Testy uruchamiają własny serwer Vite na osobnym porcie i nigdy nie
// podłączają się do serwera, który już działa. Wcześniej port 1420 był
// współdzielony z `npm run dev` / `tauri dev` i z równoległymi przebiegami
// testów z innych katalogów roboczych: Playwright po cichu używał cudzego
// serwera, a gdy ten znikał lub był przeciążony w trakcie przebiegu, losowe
// testy kończyły się przekroczeniem czasu. Port można zmienić przez
// PLAYWRIGHT_PORT, np. przy kilku równoległych przebiegach.
const port = Number(process.env.PLAYWRIGHT_PORT ?? 1430);
const baseURL = `http://127.0.0.1:${port}`;

export default defineConfig({
  testDir: "./tests",
  use: {
    baseURL,
    viewport: { width: 1180, height: 800 },
  },
  webServer: {
    command: `npx vite --host 127.0.0.1 --port ${port} --strictPort`,
    url: baseURL,
    reuseExistingServer: false,
  },
  expect: {
    // Zrzuty wzorcowe (tests/visual.spec.ts): animacje i przejścia zatrzymane,
    // kursor tekstowy ukryty, do 1% pikseli może się różnić (wygładzanie).
    toHaveScreenshot: {
      maxDiffPixelRatio: 0.01,
      animations: "disabled",
      caret: "hide",
    },
  },
  // W CI dodatkowo raport HTML z różnicami zrzutów (artefakt playwright-report).
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
});
