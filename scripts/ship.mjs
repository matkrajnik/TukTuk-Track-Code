// `npm run ship -- <wersja>` (albo dwuklik na `wydanie.cmd`): całe wydanie jednym
// poleceniem, na gałęzi master w czystym katalogu roboczym (RELEASE.md):
//   1. wersja w tauri.conf.json, package.json i Cargo.toml oraz w Cargo.lock
//      (`cargo update --workspace`) i package-lock.json (`npm install --package-lock-only`);
//   2. `npm run check` (build, testy Playwright, cargo fmt, cargo test);
//   3. `npm run release -- --notes-file notes.md` (build instalatora, podpis, latest.json,
//      release w GitHubie, kopia w releases/);
//   4. commit, tag v<wersja> i push master z tagami do zdalnego repozytorium GitHub.
// Przed uruchomieniem: nadpisz `notes.md` notatkami wydania i dopisz „Nowości” w README.
// Niezacommitowane zmiany są dozwolone tylko w README.md i notes.md.
//
// Opcje:
//   --notes-file plik   notatki (domyślnie notes.md)
//   --skip-check        bez `npm run check`
//   --skip-build        przekazane do release.mjs (gotowy instalator w target/)
//   --no-push           commit i tag bez pusha
//   --dry-run           bez publikacji, commitu, tagu i pusha; na końcu cofa zmiany wersji
//   --branch nazwa      wydanie z innej gałęzi niż master (np. gałąź poprawek)
//   --help
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { delimiter, join } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const windows = process.platform === "win32";
const env = { ...process.env };

function fail(message) {
  console.error(`\nnpm run ship: ${message}`);
  process.exit(1);
}
function step(name) {
  console.log(`\n=== ${name} ===`);
}
/** Polecenie z widocznym wyjściem; przerywa wydanie przy błędzie. */
function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: root,
    env,
    stdio: "inherit",
    shell: options.shell ?? false,
  });
  if (result.status !== 0)
    fail(
      `„${[command, ...args].join(" ")}” zakończyło się błędem (kod ${
        result.status ?? result.signal
      }).${options.retry ? ` ${options.retry}` : ""}`,
    );
}
/** Polecenie z przechwyconym wyjściem (git, gh). */
function read(command, args) {
  const result = spawnSync(command, args, { cwd: root, env, encoding: "utf8" });
  // Bez przycinania z lewej: linie `git status --porcelain` zaczynają się od spacji.
  return {
    ok: result.status === 0,
    out: (result.stdout ?? "").trimEnd(),
    err: result.error
      ? `nie znaleziono programu „${command}” (${result.error.message})`
      : (result.stderr ?? "").trim(),
  };
}
const ask = (question) =>
  new Promise((resolve) => {
    const rl = createInterface({ input: process.stdin, output: process.stdout });
    rl.question(question, (answer) => {
      rl.close();
      resolve(answer.trim());
    });
  });
/** Pytanie bez echa (hasło klucza). */
const askHidden = (question) =>
  new Promise((resolve) => {
    const rl = createInterface({ input: process.stdin, output: process.stdout, terminal: true });
    rl._writeToOutput = () => {};
    process.stdout.write(question);
    rl.question("", (answer) => {
      rl.close();
      process.stdout.write("\n");
      resolve(answer);
    });
  });

// --- argumenty ---------------------------------------------------------------
const args = process.argv.slice(2);
const options = {
  version: "",
  notesFile: "notes.md",
  branch: "master",
  skipCheck: false,
  skipBuild: false,
  push: true,
  dryRun: false,
};
for (let i = 0; i < args.length; i++) {
  const arg = args[i];
  if (arg === "--help" || arg === "-h") {
    console.log(
      [
        "Użycie: npm run ship -- <wersja> [--notes-file notes.md] [--skip-check] [--skip-build] [--no-push] [--dry-run]",
        "",
        "Podnosi wersję, sprawdza projekt, publikuje wydanie (npm run release), commituje, taguje i wypycha.",
      ].join("\n"),
    );
    process.exit(0);
  } else if (arg === "--notes-file") options.notesFile = args[++i] ?? "";
  else if (arg === "--branch") options.branch = args[++i] ?? "";
  else if (arg === "--skip-check") options.skipCheck = true;
  else if (arg === "--skip-build") options.skipBuild = true;
  else if (arg === "--no-push") options.push = false;
  else if (arg === "--dry-run") options.dryRun = true;
  else if (/^\d+\.\d+\.\d+$/.test(arg)) options.version = arg;
  else fail(`Nieznana opcja albo zła wersja: ${arg}. Wersja ma postać X.Y.Z, zobacz --help.`);
}

// --- stan repozytorium -------------------------------------------------------
const files = {
  conf: join(root, "src-tauri", "tauri.conf.json"),
  pkg: join(root, "package.json"),
  cargo: join(root, "src-tauri", "Cargo.toml"),
};
const conf = JSON.parse(readFileSync(files.conf, "utf8"));
const current = conf.version;
const cmp = (a, b) => {
  const x = a.split(".").map(Number), y = b.split(".").map(Number);
  for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return x[i] - y[i];
  return 0;
};
if (!options.version) {
  options.version = await ask(`Numer nowej wersji (obecna ${current}): `);
  if (!/^\d+\.\d+\.\d+$/.test(options.version)) fail("wersja ma postać X.Y.Z.");
}
const version = options.version;
const tag = `v${version}`;
const sameVersion = cmp(version, current) === 0;
if (cmp(version, current) < 0) fail(`wersja ${version} jest niższa od obecnej ${current}.`);

const branch = read("git", ["branch", "--show-current"]).out;
if (branch !== options.branch)
  fail(
    `wydanie robi się z gałęzi ${options.branch} (jesteś na „${branch || "?"}”); ` +
      `inną gałąź wskazuje --branch.`,
  );
const dirty = read("git", ["status", "--porcelain"]).out.split("\n").filter(Boolean);
const allowed = new Set(["README.md", options.notesFile.replace(/\\/g, "/")]);
const versionFiles = ["package.json", "package-lock.json", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock", "src-tauri/tauri.conf.json"];
const blocking = dirty.filter((line) => {
  const path = line.slice(3).replace(/\\/g, "/").replace(/^"|"$/g, "");
  if (allowed.has(path)) return false;
  // Powtórka po błędzie: pliki wersji już podniesione do tej samej wersji.
  if (sameVersion && versionFiles.includes(path)) return false;
  return !line.startsWith("??");
});
if (blocking.length)
  fail(
    "katalog roboczy ma niezacommitowane zmiany poza README.md i notatkami:\n  " +
      blocking.join("\n  ") +
      "\nZacommituj je albo wycofaj.",
  );
if (read("git", ["tag", "-l", tag]).out) fail(`tag ${tag} już istnieje.`);

const notesPath = join(root, options.notesFile);
if (!existsSync(notesPath)) fail(`brak pliku notatek ${options.notesFile}.`);
const notes = readFileSync(notesPath, "utf8").replace(/\r\n/g, "\n").trim();
if (!notes) fail(`${options.notesFile} jest pusty: wpisz notatki wydania.`);
if (notes.includes(`TukTuk Track ${current}`) && !sameVersion)
  fail(`${options.notesFile} wygląda na notatki poprzedniej wersji (${current}); nadpisz je.`);
const readme = readFileSync(join(root, "README.md"), "utf8");
if (!readme.includes(`Nowości ${version}`))
  console.warn(`Uwaga: README.md nie ma sekcji „Nowości ${version}”.`);

// GitHub CLI i zdalne repozytorium przed jakąkolwiek zmianą plików.
const remotes = read("git", ["remote", "-v"]).out.split("\n");
const github =
  remotes.find((l) => /github\.com/.test(l) && /\(push\)$/.test(l))?.split(/\s+/)[0] ?? null;
if (options.push && !options.dryRun && !github)
  fail("brak zdalnego repozytorium GitHub (git remote -v). Dodaj je albo użyj --no-push.");
if (!options.dryRun) {
  const auth = read("gh", ["auth", "status"]);
  if (!auth.ok) fail(`gh nie jest zalogowany (\`gh auth login\`):\n${auth.err || auth.out}`);
}

// Klucz podpisu: ścieżka jak w release.mjs, hasło z pytania bez echa.
const pathKey = Object.keys(env).find((k) => k.toUpperCase() === "PATH") ?? "PATH";
const cargoBin = join(env.USERPROFILE ?? homedir(), ".cargo", "bin");
if (existsSync(join(cargoBin, windows ? "cargo.exe" : "cargo")))
  env[pathKey] = cargoBin + delimiter + (env[pathKey] ?? "");
if (!options.skipBuild) {
  const keyFile = env.TAURI_SIGNING_PRIVATE_KEY || join(env.USERPROFILE ?? homedir(), ".tauri", "todotrack.key");
  if (!env.TAURI_SIGNING_PRIVATE_KEY && !existsSync(keyFile))
    fail(`brak klucza podpisu ${keyFile} (RELEASE.md).`);
  if (env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD === undefined)
    env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD = await askHidden(
      "Hasło klucza podpisu (Enter, gdy klucz nie ma hasła): ",
    );
}

// --- 1. wersja ---------------------------------------------------------------
step(`Wersja ${current} → ${version}`);
if (sameVersion) console.log("Pliki mają już tę wersję; pomijam.");
else {
  const bump = (path, from, to) => {
    const text = readFileSync(path, "utf8");
    if (!text.includes(from)) fail(`w ${path} nie ma „${from.trim()}”.`);
    writeFileSync(path, text.replace(from, to));
  };
  bump(files.conf, `"version": "${current}"`, `"version": "${version}"`);
  bump(files.pkg, `"version": "${current}"`, `"version": "${version}"`);
  bump(files.cargo, `\nversion = "${current}"`, `\nversion = "${version}"`);
  run("cargo", ["update", "--workspace", "--manifest-path", "src-tauri/Cargo.toml", "--offline"]);
  run("npm install --package-lock-only --ignore-scripts --no-audit --no-fund", [], { shell: true });
}

// --- 2. sprawdzenie ----------------------------------------------------------
if (options.skipCheck) console.log("\n(npm run check pominięte)");
else {
  step("npm run check");
  run("npm run check", [], { shell: true, retry: "Po poprawce uruchom ponownie: pliki wersji zostają." });
}

// --- 3. wydanie --------------------------------------------------------------
step("npm run release");
const releaseArgs = ["scripts/release.mjs", "--notes-file", options.notesFile];
if (options.skipBuild) releaseArgs.push("--skip-build");
if (options.dryRun) releaseArgs.push("--dry-run");
run(process.execPath, releaseArgs, {
  retry: `Po poprawce: npm run ship -- ${version} --skip-check${options.skipBuild ? " --skip-build" : ""} (gotowy instalator: dodaj --skip-build).`,
});

if (options.dryRun) {
  step("--dry-run: cofam zmiany wersji");
  if (!sameVersion) run("git", ["checkout", "--", ...versionFiles]);
  console.log("Bez commitu, tagu i pusha.");
  process.exit(0);
}

// --- 4. commit, tag, push ----------------------------------------------------
step("git commit, tag, push");
run("git", ["add", "-A", "--", ...versionFiles, "README.md", options.notesFile, "releases"]);
const title = notes.split("\n")[0].replace(/^[-*#\s]+/, "").replace(/\.$/, "");
run("git", ["-c", "core.safecrlf=false", "commit", "-q", "-m", `${version}: ${title}`]);
run("git", ["tag", tag]);
if (options.push) run("git", ["push", github, options.branch, "--tags"]);
else console.log(`Bez pusha (--no-push): git push ${github ?? "<remote>"} ${options.branch} --tags`);

const slugMatch = /github\.com\/([^/]+\/[^/]+)\/releases/.exec(conf.plugins?.updater?.endpoints?.[0] ?? "");
console.log(
  [
    "",
    `Wydanie ${tag} gotowe.`,
    slugMatch ? `Release: https://github.com/${slugMatch[1]}/releases/tag/${tag}` : "",
    "Zainstalowane aplikacje zobaczą baner przy następnym starcie albo po „Sprawdź teraz”.",
  ]
    .filter(Boolean)
    .join("\n"),
);
