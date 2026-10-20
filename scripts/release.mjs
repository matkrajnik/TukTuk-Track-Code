// `npm run release -- --notes "..."`: wydanie Todo Track do publicznego repozytorium
// wydań (RELEASE.md). Skrypt sprawdza zgodność wersji w tauri.conf.json,
// package.json i Cargo.toml, buduje instalator NSIS z artefaktami updatera
// (`createUpdaterArtifacts`), składa latest.json i tworzy release w GitHubie przez
// `gh release create` z trzema plikami: instalator .exe, podpis .sig i latest.json.
// Nie commituje i nie taguje prywatnego repozytorium; to robi użytkownik.
//
// Opcje:
//   --notes "tekst"      notatki z wydania (treść release i pole `notes` w latest.json)
//   --notes-file plik    notatki z pliku (zamiast --notes)
//   --skip-build         bez `npm run tauri build` (gotowy instalator w target/)
//   --dry-run            wszystko poza `gh release create`; wypisuje polecenie
//   --help
import { spawnSync } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { homedir } from "node:os";
import { delimiter, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const windows = process.platform === "win32";

function fail(message) {
  console.error(`\nnpm run release: ${message}`);
  process.exit(1);
}

// --- argumenty ---------------------------------------------------------------
const args = process.argv.slice(2);
const options = { notes: "", skipBuild: false, dryRun: false };
for (let i = 0; i < args.length; i++) {
  const arg = args[i];
  if (arg === "--help" || arg === "-h") {
    console.log(
      [
        "Użycie: npm run release -- --notes \"...\" [--notes-file plik] [--skip-build] [--dry-run]",
        "",
        "  --notes       notatki z wydania (release w GitHubie i latest.json)",
        "  --notes-file  notatki z pliku",
        "  --skip-build  pomiń `npm run tauri build` (użyj gotowego instalatora)",
        "  --dry-run     bez publikacji; buduje, składa latest.json i wypisuje polecenie gh",
      ].join("\n"),
    );
    process.exit(0);
  } else if (arg === "--notes") options.notes = args[++i] ?? "";
  else if (arg.startsWith("--notes=")) options.notes = arg.slice(8);
  else if (arg === "--notes-file") {
    const file = args[++i];
    if (!file) fail("--notes-file wymaga ścieżki do pliku.");
    options.notes = readFileSync(join(process.cwd(), file), "utf8");
  } else if (arg === "--skip-build") options.skipBuild = true;
  else if (arg === "--dry-run") options.dryRun = true;
  else fail(`Nieznana opcja: ${arg}. Zobacz --help.`);
}
options.notes = options.notes.replace(/\r\n/g, "\n").trim();
if (!options.notes)
  fail('Podaj notatki z wydania: --notes "..." albo --notes-file plik.');

// --- wersje ------------------------------------------------------------------
const tauriConf = JSON.parse(
  readFileSync(join(root, "src-tauri", "tauri.conf.json"), "utf8"),
);
const pkg = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
const cargo = readFileSync(join(root, "src-tauri", "Cargo.toml"), "utf8");
const cargoVersion = /^\[package\][^[]*?^version\s*=\s*"([^"]+)"/ms.exec(cargo)?.[1];
const versions = {
  "src-tauri/tauri.conf.json": tauriConf.version,
  "package.json": pkg.version,
  "src-tauri/Cargo.toml": cargoVersion,
};
const version = tauriConf.version;
if (!/^\d+\.\d+\.\d+$/.test(version ?? ""))
  fail(`Wersja w tauri.conf.json („${version}”) nie ma postaci X.Y.Z.`);
const mismatched = Object.entries(versions).filter(([, v]) => v !== version);
if (mismatched.length)
  fail(
    "wersje się różnią:\n" +
      Object.entries(versions)
        .map(([file, v]) => `  ${file}: ${v ?? "brak"}`)
        .join("\n") +
      "\nUjednolić wersję w trzech plikach (i Cargo.lock: `cargo check` w src-tauri).",
  );

// --- repozytorium wydań z adresu updatera -------------------------------------
const endpoint = tauriConf.plugins?.updater?.endpoints?.[0] ?? "";
const repoMatch =
  /^https:\/\/github\.com\/([^/<>]+)\/([^/<>]+)\/releases\/latest\/download\/latest\.json$/.exec(
    endpoint,
  );
if (!repoMatch)
  fail(
    `plugins.updater.endpoints[0] w tauri.conf.json musi mieć postać ` +
      `https://github.com/<OWNER>/<REPO>/releases/latest/download/latest.json ` +
      `(jest: „${endpoint}”). Uzupełnij właściciela repozytorium wydań (RELEASE.md).`,
  );
const [, owner, repo] = repoMatch;
const slug = `${owner}/${repo}`;
const tag = `v${version}`;
if (!tauriConf.bundle?.createUpdaterArtifacts)
  fail("bundle.createUpdaterArtifacts w tauri.conf.json musi być true (podpis .sig).");

// --- środowisko: cargo, klucz podpisu, gh -------------------------------------
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
if (!options.skipBuild && !found("cargo")) {
  const cargoBin = join(env.USERPROFILE ?? homedir(), ".cargo", "bin");
  if (existsSync(join(cargoBin, windows ? "cargo.exe" : "cargo")))
    env[pathKey] = cargoBin + delimiter + (env[pathKey] ?? "");
  else fail(`nie znaleziono cargo ani w PATH, ani w ${cargoBin}.`);
}
if (!options.skipBuild && !env.TAURI_SIGNING_PRIVATE_KEY) {
  // Domyślna lokalizacja klucza z RELEASE.md; zmienna może też zawierać sam klucz.
  const keyFile = join(env.USERPROFILE ?? homedir(), ".tauri", "todotrack.key");
  if (existsSync(keyFile)) {
    env.TAURI_SIGNING_PRIVATE_KEY = keyFile;
    console.log(`Klucz podpisu: ${keyFile}`);
  } else
    fail(
      `brak klucza podpisu: ustaw TAURI_SIGNING_PRIVATE_KEY (ścieżka albo treść klucza) ` +
        `albo zapisz klucz w ${keyFile}. Generowanie: RELEASE.md.`,
    );
}
if (!options.dryRun) {
  if (!found("gh"))
    fail("nie znaleziono GitHub CLI (gh). Instalacja i logowanie: RELEASE.md.");
  const auth = spawnSync("gh", ["auth", "status"], { env, encoding: "utf8" });
  if (auth.status !== 0)
    fail(`gh nie jest zalogowany (\`gh auth login\`):\n${auth.stderr || auth.stdout}`);
  const existing = spawnSync(
    "gh",
    ["release", "view", tag, "--repo", slug, "--json", "tagName"],
    { env, encoding: "utf8" },
  );
  if (existing.status === 0)
    fail(
      `release ${tag} już istnieje w ${slug}. Podnieś wersję albo usuń go: ` +
        `gh release delete ${tag} --repo ${slug} --cleanup-tag`,
    );
}

// --- build -------------------------------------------------------------------
const productName = tauriConf.productName;
const bundleDir = join(root, "src-tauri", "target", "release", "bundle", "nsis");
const installer = join(bundleDir, `${productName}_${version}_x64-setup.exe`);
const signature = `${installer}.sig`;
if (!options.skipBuild) {
  console.log(`\n=== npm run tauri build -- -- --locked (${version}) ===`);
  const build = spawnSync("npm run tauri build -- -- --locked", {
    cwd: root,
    env,
    stdio: "inherit",
    shell: true,
  });
  if (build.status !== 0)
    fail(`build zakończył się błędem (kod ${build.status ?? build.signal}).`);
}
if (!existsSync(installer)) fail(`brak instalatora: ${installer}`);
if (!existsSync(signature))
  fail(
    `brak podpisu ${signature}. Build musi mieć klucz podpisu ` +
      `(TAURI_SIGNING_PRIVATE_KEY) i bundle.createUpdaterArtifacts = true.`,
  );

// --- artefakty wydania -------------------------------------------------------
// GitHub zamienia spacje w nazwach plików release na kropki, więc instalator
// i podpis idą pod nazwą bez spacji; ta sama nazwa jest w `url` w latest.json.
const assetName = `${productName.replace(/\s+/g, "")}_${version}_x64-setup.exe`;
const stage = join(root, "src-tauri", "target", "release", "bundle", "updater");
mkdirSync(stage, { recursive: true });
const assetExe = join(stage, assetName);
const assetSig = join(stage, `${assetName}.sig`);
const latestPath = join(stage, "latest.json");
const notesPath = join(stage, "notes.md");
copyFileSync(installer, assetExe);
copyFileSync(signature, assetSig);
const latest = {
  version,
  notes: options.notes,
  pub_date: new Date().toISOString().replace(/\.\d{3}Z$/, "Z"),
  platforms: {
    "windows-x86_64": {
      signature: readFileSync(signature, "utf8").trim(),
      url: `https://github.com/${slug}/releases/download/${tag}/${assetName}`,
    },
  },
};
writeFileSync(latestPath, JSON.stringify(latest, null, 2) + "\n");
writeFileSync(notesPath, options.notes + "\n");
// Kopia instalatora w releases/ jak w dotychczasowych wydaniach (do commitu);
// folder trzyma tylko najnowszy instalator, starsze są usuwane. --dry-run nie
// zmienia niczego poza target/.
const releasesDir = join(root, "releases");
const releasesCopy = join(releasesDir, `${productName}_${version}_x64-setup.exe`);
if (!options.dryRun) {
  mkdirSync(releasesDir, { recursive: true });
  for (const name of readdirSync(releasesDir))
    if (/_x64-setup\.exe$/.test(name) && join(releasesDir, name) !== releasesCopy)
      rmSync(join(releasesDir, name));
  copyFileSync(installer, releasesCopy);
}

const mb = (file) => (statSync(file).size / 1_048_576).toFixed(1);
console.log(`\nArtefakty wydania ${tag} (${stage}):`);
console.log(`  ${assetName} (${mb(assetExe)} MB)`);
console.log(`  ${assetName}.sig`);
console.log(`  latest.json → ${latest.platforms["windows-x86_64"].url}`);
console.log(
  options.dryRun
    ? `Kopia instalatora w releases/ dopiero przy publikacji: ${releasesCopy}`
    : `Kopia instalatora: ${releasesCopy}`,
);

// --- publikacja --------------------------------------------------------------
const ghArgs = [
  "release",
  "create",
  tag,
  "--repo",
  slug,
  "--title",
  `${productName} ${version}`,
  "--notes-file",
  notesPath,
  assetExe,
  assetSig,
  latestPath,
];
const shown = ["gh", ...ghArgs]
  .map((a) => (/[\s"]/.test(a) ? `"${a.replace(/"/g, '\\"')}"` : a))
  .join(" ");
if (options.dryRun) {
  console.log(`\n--dry-run: bez publikacji. Polecenie:\n  ${shown}`);
  process.exit(0);
}
console.log(`\n=== ${shown} ===`);
const release = spawnSync("gh", ghArgs, { cwd: root, env, stdio: "inherit" });
if (release.status !== 0)
  fail(
    `gh release create zakończył się błędem (kod ${release.status ?? release.signal}). ` +
      `Po poprawieniu powtórz z --skip-build.`,
  );
console.log(
  [
    "",
    `Wydanie ${tag} opublikowane: https://github.com/${slug}/releases/tag/${tag}`,
    "Aplikacje sprawdzą latest.json przy starcie albo po „Sprawdź teraz”.",
    "W prywatnym repozytorium zostaje do zrobienia: commit (releases/, README),",
    `tag ${tag} i push.`,
  ].join("\n"),
);
