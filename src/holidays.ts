/**
 * Polskie święta ustawowe: stałe i ruchome liczone od Wielkanocy.
 * Odpowiednik `src-tauri/src/holidays.rs`; oba porównuje się z tą samą listą
 * (tests/fixtures/holidays-pl-2024-2030.json).
 */
export interface Holiday {
  /** Klucz dnia RRRR-MM-DD. */
  date: string;
  name: string;
}

const key = (y: number, m: number, d: number) =>
  `${y}-${String(m).padStart(2, "0")}-${String(d).padStart(2, "0")}`;

/** Niedziela Wielkanocna (algorytm Meeusa/Jonesa/Butchera) jako [miesiąc, dzień]. */
export function easter(year: number): [number, number] {
  const a = year % 19;
  const b = Math.floor(year / 100);
  const c = year % 100;
  const d = Math.floor(b / 4);
  const e = b % 4;
  const f = Math.floor((b + 8) / 25);
  const g = Math.floor((b - f + 1) / 3);
  const h = (19 * a + b - d - g + 15) % 30;
  const i = Math.floor(c / 4);
  const k = c % 4;
  const l = (32 + 2 * e + 2 * i - h - k) % 7;
  const m = Math.floor((a + 11 * h + 22 * l) / 451);
  const n = h + l - 7 * m + 114;
  return [Math.floor(n / 31), (n % 31) + 1];
}

const cache = new Map<number, Holiday[]>();

/** Święta ustawowe danego roku, rosnąco po dacie. */
export function holidays(year: number): Holiday[] {
  const cached = cache.get(year);
  if (cached) return cached;
  const [em, ed] = easter(year);
  // Daty liczone w UTC, żeby zmiana czasu nie przesuwała dni.
  const fromEaster = (offset: number) => {
    const date = new Date(Date.UTC(year, em - 1, ed + offset));
    return key(date.getUTCFullYear(), date.getUTCMonth() + 1, date.getUTCDate());
  };
  const list: Holiday[] = [
    { date: key(year, 1, 1), name: "Nowy Rok" },
    { date: key(year, 1, 6), name: "Trzech Króli" },
    { date: fromEaster(0), name: "Wielkanoc" },
    { date: fromEaster(1), name: "Poniedziałek Wielkanocny" },
    { date: key(year, 5, 1), name: "Święto Pracy" },
    { date: key(year, 5, 3), name: "Święto Konstytucji 3 Maja" },
    { date: fromEaster(49), name: "Zielone Świątki" },
    { date: fromEaster(60), name: "Boże Ciało" },
    { date: key(year, 8, 15), name: "Wniebowzięcie NMP" },
    { date: key(year, 11, 1), name: "Wszystkich Świętych" },
    { date: key(year, 11, 11), name: "Święto Niepodległości" },
    { date: key(year, 12, 25), name: "Boże Narodzenie" },
    { date: key(year, 12, 26), name: "Drugi dzień Bożego Narodzenia" },
  ];
  // Wigilia jest dniem wolnym od 2025 r.
  if (year >= 2025) list.push({ date: key(year, 12, 24), name: "Wigilia" });
  list.sort((a, b) => a.date.localeCompare(b.date));
  cache.set(year, list);
  return list;
}

/** Nazwa święta w dniu `day` (RRRR-MM-DD) albo null. */
export function holidayName(day: string): string | null {
  const year = Number(day.slice(0, 4));
  if (!Number.isInteger(year)) return null;
  return holidays(year).find((h) => h.date === day)?.name ?? null;
}
