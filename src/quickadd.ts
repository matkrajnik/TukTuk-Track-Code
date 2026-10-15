/**
 * Składnia szybkiego dodawania (PLAN_1.3.md, etap 3). Czysta funkcja `parse`
 * bez zależności od Vue i Tauri, więc testy jednostkowe uruchamiają ją wprost.
 *
 * Zapisy (każdy jako osobne słowo, w dowolnej kolejności):
 * - klucz Jiry (`WMS-123`) jako pierwsze słowo, które nie jest innym zapisem;
 * - `~30m`, `~2h`, `~1h30` — szacunek;
 * - `@dziś`, `@jutro`, `@pon`…`@nd`, `@20.10` — dzień w planie (najbliższy taki dzień);
 * - `do:pt`, `do:20.10` — termin;
 * - `!` / `!wysoki` i `!niski` — priorytet;
 * - `#nazwa` — projekt (także wielowyrazowa nazwa istniejącego projektu);
 * - `/szablon` — szablon z Ustawień → Szablony (nieznana nazwa to błąd);
 * - `\` przed znakiem specjalnym — zapis dosłowny (`C\#` → „C#”, `\#wms` → „#wms”).
 *
 * Tekst bez zapisów i bez `\` daje tytuł identyczny z wpisanym, więc zwykłe
 * dodawanie działa dokładnie jak wcześniej.
 */

export type TokenKind =
  | "jira"
  | "estimate"
  | "planned"
  | "due"
  | "priority"
  | "project"
  | "template"
  | "error";

export interface Token {
  kind: TokenKind;
  /** Zapis z pola, np. `@pt`. */
  text: string;
  /** Położenie zapisu w tekście (do usuwania kliknięciem chipa). */
  start: number;
  end: number;
  /** Tekst chipa po polsku. */
  label: string;
  /** Opis błędu (tylko `kind: "error"`). */
  error?: string;
}

export interface QuickFields {
  jiraKey?: string;
  estimateMinutes?: number;
  plannedDay?: string;
  dueDay?: string;
  /** 1 = wysoki, -1 = niski. */
  priority?: number;
  project?: string;
  template?: string;
}

export interface Parsed {
  title: string;
  fields: QuickFields;
  tokens: Token[];
  /** Opisy błędów; niepusta lista blokuje dodanie. */
  errors: string[];
}

export interface QuickSettings {
  /** Istniejące projekty: podpowiedzi i zachowanie pisowni. */
  projects?: string[];
  /** Nazwy szablonów: podpowiedzi Tab, pisownia i błąd przy nieznanej nazwie. */
  templates?: string[];
  /**
   * Czy Jira jest połączona. `false`: słowo wyglądające jak klucz (np.
   * „COVID-19”) zostaje w tytule. Domyślnie (`undefined`) klucz jest rozpoznawany.
   */
  jiraEnabled?: boolean;
}

const SPECIAL = "~@#!/\\";
const JIRA_KEY = /^[A-Z][A-Z0-9_]*-\d+$/;
const TEMPLATE = /^[\p{L}\p{N}_-]{1,60}$/u;
const MAX_ESTIMATE = 600_000;

/** Dni tygodnia: 1 = poniedziałek … 7 = niedziela (jak `repeatDays`). */
const WEEKDAYS: Record<string, number> = {
  pon: 1,
  pn: 1,
  poniedziałek: 1,
  poniedzialek: 1,
  wt: 2,
  wto: 2,
  wtorek: 2,
  śr: 3,
  sr: 3,
  śro: 3,
  sro: 3,
  środa: 3,
  sroda: 3,
  czw: 4,
  cz: 4,
  czwartek: 4,
  pt: 5,
  pią: 5,
  pia: 5,
  piątek: 5,
  piatek: 5,
  sob: 6,
  so: 6,
  sobota: 6,
  nd: 7,
  nie: 7,
  ndz: 7,
  niedz: 7,
  niedziela: 7,
};
const RELATIVE: Record<string, number> = {
  dziś: 0,
  dzis: 0,
  dzisiaj: 0,
  jutro: 1,
  pojutrze: 2,
};
const SHORT_DAYS = ["", "pon", "wt", "śr", "czw", "pt", "sob", "nd"];

/** Dzień `YYYY-MM-DD` jako data UTC (bez wpływu zmiany czasu). */
function utc(day: string) {
  const [y, m, d] = day.split("-").map(Number);
  return new Date(Date.UTC(y, m - 1, d));
}
function key(date: Date) {
  return date.toISOString().slice(0, 10);
}
function addDays(day: string, n: number) {
  const date = utc(day);
  date.setUTCDate(date.getUTCDate() + n);
  return key(date);
}
function weekday(day: string) {
  return ((utc(day).getUTCDay() + 6) % 7) + 1;
}
function validDate(y: number, m: number, d: number) {
  if (m < 1 || m > 12 || d < 1 || y < 1970 || y > 9999) return false;
  const date = new Date(Date.UTC(y, m - 1, d));
  return date.getUTCMonth() === m - 1 && date.getUTCDate() === d;
}
const pad = (n: number) => String(n).padStart(2, "0");

/** Czytelny dzień do chipa: „pt 16.10” (rok, gdy inny niż dziś). */
export function dayLabel(day: string, today: string) {
  const [y, m, d] = day.split("-");
  const year = y !== today.slice(0, 4) ? `.${y}` : "";
  const name =
    day === today ? "dziś" : day === addDays(today, 1) ? "jutro" : SHORT_DAYS[weekday(day)];
  return `${name} ${d}.${m}${year}`;
}

/** Czas szacunku do chipa: „45 min”, „2 h”, „1 h 30 min”. */
export function minutesLabel(minutes: number) {
  const h = Math.floor(minutes / 60),
    m = minutes % 60;
  return [h ? `${h} h` : "", m ? `${m} min` : ""].filter(Boolean).join(" ");
}

/**
 * Wartość dnia (`dziś`, `pt`, `20.10`, `20.10.2027`) względem `today`.
 * Dzień tygodnia to najbliższy następny (`pon` w poniedziałek = za tydzień),
 * data bez roku to najbliższe takie dd.mm od dziś (po dacie w tym roku — przyszły rok).
 * `null` = to nie jest zapis dnia; `{ error }` = wygląda na dzień, ale jest błędny.
 */
export function parseDay(
  value: string,
  today: string,
): { day: string } | { error: string } | null {
  const word = value.toLocaleLowerCase("pl");
  if (word in RELATIVE) return { day: addDays(today, RELATIVE[word]) };
  if (word in WEEKDAYS) {
    const ahead = (WEEKDAYS[word] - weekday(today) + 7) % 7 || 7;
    return { day: addDays(today, ahead) };
  }
  const match = /^(\d{1,2})\.(\d{1,2})(?:\.(\d{4}))?\.?$/.exec(word);
  if (!match) return /^\d/.test(word) ? { error: `Nie rozumiem daty „${value}”.` } : null;
  const d = Number(match[1]),
    m = Number(match[2]);
  const [ty, tm, td] = today.split("-").map(Number);
  const y = match[3]
    ? Number(match[3])
    : m > tm || (m === tm && d >= td)
      ? ty
      : ty + 1;
  if (!validDate(y, m, d))
    return { error: `Nie ma dnia ${pad(d)}.${pad(m)}.${y}.` };
  return { day: `${y}-${pad(m)}-${pad(d)}` };
}

/** `30m`, `2h`, `1h30`, `1h30m`; `null` = to nie jest szacunek. */
export function parseEstimate(
  value: string,
): { minutes: number } | { error: string } | null {
  const match = /^(?:(\d+)h(?:(\d+)m?)?|(\d+)m(?:in)?)$/i.exec(value);
  if (!match) return /^\d/.test(value) ? { error: `Nie rozumiem szacunku „~${value}”.` } : null;
  const minutes = match[3]
    ? Number(match[3])
    : Number(match[1]) * 60 + Number(match[2] ?? 0);
  if (match[2] && Number(match[2]) >= 60)
    return { error: `Minuty w „~${value}” muszą być mniejsze niż 60.` };
  if (minutes <= 0 || minutes > MAX_ESTIMATE)
    return { error: `Szacunek „~${value}” jest poza zakresem.` };
  return { minutes };
}

/** Usuwa `\` przed znakami specjalnymi (zapis dosłowny). */
function unescape(word: string) {
  return word.replace(/\\([~@#!/\\])/g, "$1");
}

const FIELD_LABELS: Record<string, string> = {
  jira: "klucz Jiry",
  estimate: "szacunek",
  planned: "dzień w planie",
  due: "termin",
  priority: "priorytet",
  project: "projekt",
  template: "szablon",
};

export function parse(
  text: string,
  today: string,
  settings: QuickSettings = {},
): Parsed {
  const tokens: Token[] = [];
  const fields: QuickFields = {};
  const errors: string[] = [];
  const titleParts: { start: number; end: number; text: string }[] = [];
  const projects = [...(settings.projects ?? [])]
    .filter(Boolean)
    .sort((a, b) => b.length - a.length);
  const words = [...text.matchAll(/\S+/g)].map((m) => ({
    text: m[0],
    start: m.index!,
    end: m.index! + m[0].length,
  }));
  let escaped = false;
  let titleStarted = false;

  const set = (
    kind: Exclude<TokenKind, "error">,
    word: { text: string; start: number; end: number },
    label: string,
    apply: () => void,
    has: boolean,
  ) => {
    if (has) {
      const error = `Podano dwa razy: ${FIELD_LABELS[kind]}.`;
      tokens.push({ kind: "error", text: word.text, start: word.start, end: word.end, label: `${word.text}: podano dwa razy`, error });
      errors.push(error);
      return;
    }
    apply();
    tokens.push({ kind, text: word.text, start: word.start, end: word.end, label });
  };
  const fail = (word: { text: string; start: number; end: number }, error: string) => {
    tokens.push({ kind: "error", text: word.text, start: word.start, end: word.end, label: `${word.text}: błąd`, error });
    errors.push(error);
  };

  for (let i = 0; i < words.length; i++) {
    const word = words[i];
    const w = word.text;
    const lower = w.toLocaleLowerCase("pl");
    const literal = () => {
      if (w.includes("\\")) escaped = true;
      // Słowo zaczynające się od `\` przed innym znakiem zostaje dosłownie
      // (np. `\WMS-1` jako zwykły tekst) bez ukośnika.
      const shown =
        w.startsWith("\\") && w.length > 1 && !SPECIAL.includes(w[1])
          ? w.slice(1)
          : unescape(w);
      titleParts.push({ start: word.start, end: word.end, text: shown });
      titleStarted = true;
    };

    if (w.startsWith("\\")) {
      literal();
      continue;
    }
    if (
      settings.jiraEnabled !== false &&
      !titleStarted &&
      JIRA_KEY.test(w) &&
      !fields.jiraKey
    ) {
      set("jira", word, `Jira: ${w}`, () => (fields.jiraKey = w), false);
      continue;
    }
    if (w.length > 1 && w[0] === "~") {
      const result = parseEstimate(w.slice(1));
      if (result && "error" in result) fail(word, result.error);
      else if (result)
        set("estimate", word, `Szacunek: ${minutesLabel(result.minutes)}`, () => (fields.estimateMinutes = result.minutes), fields.estimateMinutes != null);
      else literal();
      continue;
    }
    if (w.length > 1 && w[0] === "@") {
      const result = parseDay(w.slice(1), today);
      if (result && "error" in result) fail(word, result.error);
      else if (result)
        set("planned", word, `Plan: ${dayLabel(result.day, today)}`, () => (fields.plannedDay = result.day), fields.plannedDay != null);
      else literal();
      continue;
    }
    if (lower.startsWith("do:") && w.length > 3) {
      const result = parseDay(w.slice(3), today);
      if (result && "error" in result) fail(word, result.error);
      else if (result)
        set("due", word, `Termin: ${dayLabel(result.day, today)}`, () => (fields.dueDay = result.day), fields.dueDay != null);
      else fail(word, `Nie rozumiem terminu „${w}”.`);
      continue;
    }
    if (w[0] === "!") {
      const rest = lower.slice(1);
      const priority =
        rest === "" || rest === "wysoki" || rest === "ważne" || rest === "wazne"
          ? 1
          : rest === "niski"
            ? -1
            : null;
      if (priority === null) literal();
      else
        set("priority", word, `Priorytet: ${priority > 0 ? "wysoki" : "niski"}`, () => (fields.priority = priority), fields.priority != null);
      continue;
    }
    if (w.length > 1 && w[0] === "#") {
      // Najdłuższa pasująca nazwa istniejącego projektu (także z odstępami).
      const after = text.slice(word.start + 1);
      const known = projects.find((p) => {
        const head = after.slice(0, p.length);
        return (
          head.toLocaleLowerCase("pl") === p.toLocaleLowerCase("pl") &&
          (after.length === p.length || /\s/.test(after[p.length]))
        );
      });
      let token = word;
      let name = unescape(w.slice(1));
      if (known) {
        const end = word.start + 1 + known.length;
        token = { text: text.slice(word.start, end), start: word.start, end };
        name = known;
        while (i + 1 < words.length && words[i + 1].end <= end) i++;
      }
      if ([...name].length > 80) fail(token, "Nazwa projektu może mieć najwyżej 80 znaków.");
      else
        set("project", token, `Projekt: ${name}`, () => (fields.project = name), fields.project != null);
      continue;
    }
    if (w.length > 1 && w[0] === "/" && TEMPLATE.test(w.slice(1))) {
      // Znana lista szablonów: pisownia z listy, nieznana nazwa to błąd.
      // Bez listy (`templates` nie podano) nazwa jest przyjmowana, jak wpisana.
      const known = settings.templates?.find(
        (t) => t.toLocaleLowerCase("pl") === w.slice(1).toLocaleLowerCase("pl"),
      );
      if (settings.templates && !known)
        fail(
          word,
          `Nie ma szablonu „${w.slice(1)}”. Dodaj go w Ustawieniach → Szablony albo poprzedź / znakiem \\, by zostawić w tytule.`,
        );
      else {
        const name = known ?? w.slice(1);
        set("template", word, `Szablon: ${name}`, () => (fields.template = name), fields.template != null);
      }
      continue;
    }
    literal();
  }

  let title: string;
  if (!tokens.length && !escaped) title = text.trim();
  else {
    // Odstępy wewnątrz tytułu zostają, jak je wpisano; usunięte zapisy nie
    // zostawiają podwójnych spacji.
    title = "";
    let previous: { end: number } | null = null;
    for (const part of titleParts) {
      if (previous) {
        const gap = text.slice(previous.end, part.start);
        const removed = tokens.some((t) => t.start >= previous!.end && t.end <= part.start);
        title += removed ? " " : gap;
      }
      title += part.text;
      previous = part;
    }
  }
  return { title, fields, tokens, errors };
}

/** Tekst po usunięciu zapisu (kliknięcie chipa): bez podwójnych odstępów. */
export function removeToken(text: string, token: Pick<Token, "start" | "end">) {
  const before = text.slice(0, token.start).replace(/\s+$/, "");
  const after = text.slice(token.end).replace(/^\s+/, "");
  return before && after ? `${before} ${after}` : before + after;
}

/** Czy wynik ma pola do zapisania poza samym tytułem. */
export function hasFields(parsed: Parsed) {
  return Object.values(parsed.fields).some((v) => v !== undefined);
}

/**
 * Tab: uzupełnia `#projekt` (albo `/szablon`) pod kursorem pierwszą pasującą
 * nazwą. `null`, gdy nie ma czego uzupełnić (Tab działa wtedy zwyczajnie).
 */
export function complete(
  text: string,
  caret: number,
  settings: QuickSettings,
): { text: string; caret: number } | null {
  const start = text.slice(0, caret).search(/\S+$/);
  if (start < 0) return null;
  const word = text.slice(start, caret);
  const prefix = word[0];
  const list =
    prefix === "#" ? settings.projects ?? [] : prefix === "/" ? settings.templates ?? [] : [];
  const typed = word.slice(1).toLocaleLowerCase("pl");
  if (!list.length || (word.length > 1 && word[1] === "\\")) return null;
  const match = [...list]
    .sort((a, b) => a.localeCompare(b, "pl"))
    .find((p) => p.toLocaleLowerCase("pl").startsWith(typed));
  if (!match) return null;
  let rest = text.slice(caret);
  // Kursor w środku słowa: zastępujemy całe słowo.
  const tail = rest.match(/^\S*/)![0];
  rest = rest.slice(tail.length);
  const inserted = `${prefix}${match}`;
  const spacer = rest.startsWith(" ") ? "" : " ";
  return {
    text: text.slice(0, start) + inserted + spacer + rest,
    caret: start + inserted.length + 1,
  };
}

/** Błąd blokujący dodanie: pierwszy błąd składni (także nieznany szablon). */
export function blockingError(parsed: Parsed): string | undefined {
  return parsed.errors[0];
}

/**
 * Tytuł zadania z szablonu; ta sama reguła co `templates::render_title` w Rust.
 * - `{klucz}` → klucz Jiry (pusty, gdy go nie podano);
 * - `{tytuł}` (także `{tytul}`) → tytuł z linii, a bez niego tytuł zgłoszenia (`summary`);
 * - tytuł z linii bez pola `{tytuł}` jest dopisywany na końcu;
 * - pusty tytuł szablonu zostawia tytuł z linii.
 * Rzuca błąd, gdy potrzebny tytuł zgłoszenia jest nieznany.
 */
export function renderTitle(
  template: string,
  key: string | null | undefined,
  line: string,
  summary: (key: string) => string | null | undefined,
): string {
  line = line.trim();
  template = template.trim();
  if (!template) return line;
  const wantsTitle = template.includes("{tytuł}") || template.includes("{tytul}");
  let value = line;
  if (!line && wantsTitle && key) {
    value = summary(key)?.trim() ?? "";
    if (!value)
      throw new Error(
        `Nie znam tytułu ${key}. Wpisz tytuł albo odśwież listę zgłoszeń w widoku Jira.`,
      );
  }
  let title = template
    .replaceAll("{klucz}", key ?? "")
    .replaceAll("{tytuł}", value)
    .replaceAll("{tytul}", value);
  if (!wantsTitle && line) title = `${title} ${line}`;
  return tidyTitle(title);
}

/** Sprząta tytuł po pustych polach (jak `templates::tidy`). */
function tidyTitle(title: string) {
  return title
    .split(/\s+/)
    .filter(Boolean)
    .join(" ")
    .replace(/ ([:,.])/g, "$1")
    .replace(/^[:,\-–— ]+|[:,\-–— ]+$/g, "");
}
