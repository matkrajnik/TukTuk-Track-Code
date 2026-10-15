// `npm run check`: lokalnie to samo co CI (.github/workflows/check.yml) bez buildu
// Tauri — build frontendu, testy Playwright, cargo fmt --check i cargo test --locked.
// Kroki idą po kolei i zatrzymują się na pierwszym błędzie. Gdy `cargo` nie ma
// w PATH (częste w Windows po instalacji rustup), używa %USERPROFILE%\.cargo\bin.
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { delimiter, join } from "node:path";

const windows = process.platform === "win32";
const env = { ...process.env };
const pathKey =
  Object.keys(env).find((k) => k.toUpperCase() === "PATH") ?? "PATH";

function found(command) {
  const probe = spawnSync(`${windows ? "where" : "which"} ${command}`, {
    env,
    stdio: "ignore",
    shell: true,
  });
  return probe.status === 0;
}
if (!found("cargo")) {
  const cargoBin = join(env.USERPROFILE ?? homedir(), ".cargo", "bin");
  if (existsSync(join(cargoBin, windows ? "cargo.exe" : "cargo")))
    env[pathKey] = cargoBin + delimiter + (env[pathKey] ?? "");
  else {
    console.error(
      `Nie znaleziono cargo ani w PATH, ani w ${cargoBin}. Zainstaluj Rust (rustup).`,
    );
    process.exit(1);
  }
}

const manifest = ["--manifest-path", "src-tauri/Cargo.toml"];
const steps = [
  ["Build (vue-tsc + vite)", "npm", ["run", "build"]],
  ["Testy Playwright", "npm", ["test"]],
  ["cargo fmt --check", "cargo", ["fmt", ...manifest, "--check"]],
  ["cargo test --locked", "cargo", ["test", ...manifest, "--locked"]],
];

const started = Date.now();
for (const [name, command, args] of steps) {
  console.log(`\n=== ${name} ===`);
  // Argumenty są stałe z listy wyżej, więc jedno polecenie dla powłoki jest bezpieczne.
  const result = spawnSync([command, ...args].join(" "), {
    env,
    stdio: "inherit",
    // npm w Windows to plik .cmd, który wymaga powłoki.
    shell: true,
  });
  if (result.status !== 0) {
    console.error(
      `\nnpm run check: krok „${name}” zakończył się błędem (kod ${result.status ?? result.signal}).`,
    );
    process.exit(result.status || 1);
  }
}
console.log(
  `\nnpm run check: wszystko zielone (${Math.round((Date.now() - started) / 1000)} s).`,
);
