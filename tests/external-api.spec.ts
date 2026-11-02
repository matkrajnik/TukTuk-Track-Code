import { test, expect } from "./fixtures/test";
import { openPreview } from "./fixtures/seed";
import { backend, openDesktop } from "./fixtures/tauri";
import { openSettings } from "./fixtures/settings";

/**
 * Zewnętrzne sterowanie pomiarem: wyłącznik w Ustawieniach → Agenci. Potok,
 * żądania add/stop/status, wznowienie dnia i program tuktuk-track-ctl testują
 * testy Rust (`api.rs`, `api_wire.rs`, `tests/ctl_exe.rs`).
 */
const toggleName = "Pozwól innym programom sterować pomiarem";
const ctlStatus = "Stan tuktuk-track-ctl.exe";

test("podgląd: wyłącznik domyślnie wyłączony i zapamiętany", async ({
  page,
}) => {
  await openPreview(page);
  await openSettings(page, "Agenci");
  const toggle = page.getByLabel(toggleName);
  await expect(toggle).not.toBeChecked();
  await expect(page.getByRole("status", { name: ctlStatus })).toHaveText(
    "Program tuktuk-track-ctl.exe jest dostępny w zainstalowanej aplikacji.",
  );
  await toggle.check();
  await page.reload();
  await openSettings(page, "Agenci");
  await expect(page.getByLabel(toggleName)).toBeChecked();
});

test("aplikacja: ścieżka programu i zapis ustawienia externalApi", async ({
  page,
}) => {
  await openDesktop(page);
  await openSettings(page, "Agenci");
  await expect(page.getByRole("status", { name: ctlStatus })).toHaveText(
    "Program: C:\\Program Files\\TukTuk Track\\tuktuk-track-ctl.exe",
  );
  await page.getByLabel(toggleName).check();
  await expect
    .poll(async () =>
      (await backend<{ cmd: string; args: any }[]>(page, "calls")).some(
        (c) => c.cmd === "set_settings" && c.args?.patch?.externalApi === true,
      ),
    )
    .toBe(true);
});
