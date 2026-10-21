import { test, expect } from "./fixtures/test";
import { clock, dayKey, hhmm, hm, localInput, plural, stamp } from "../src/time";
import { hm as targetHm, hhmm as targetHhmm } from "../src/target";
import { tsPlural } from "../src/timesheet";

// Testy jednostkowe wspólnego formatowania czasu i odmiany (B10), bez przeglądarki.

test("clock: godzina lokalna HH:MM, z sekundami HH:MM:SS", () => {
  const at = new Date(2026, 9, 21, 9, 5, 3).getTime();
  expect(clock(at)).toBe("09:05");
  expect(clock(at, true)).toBe("09:05:03");
  expect(clock(new Date(2026, 9, 21, 0, 0, 0).getTime())).toBe("00:00");
  expect(clock(new Date(2026, 9, 21, 23, 59, 59).getTime(), true)).toBe(
    "23:59:59",
  );
  // To samo co dotychczasowe formatowanie Intl w interfejsie.
  const intl = new Intl.DateTimeFormat("pl-PL", {
    hour: "2-digit",
    minute: "2-digit",
  }).format(at);
  expect(clock(at)).toBe(intl);
  expect(clock(at, true)).toBe(new Date(at).toLocaleTimeString("pl-PL"));
});

test("dayKey, localInput i stamp korzystają z tych samych pól", () => {
  const at = new Date(2026, 0, 2, 7, 8, 9).getTime();
  expect(dayKey(at)).toBe("2026-01-02");
  expect(localInput(at)).toBe("2026-01-02T07:08:09");
  expect(stamp(at).endsWith(", 07:08:09")).toBe(true);
});

test("hm i hhmm: czas trwania G:MM, minuty w dół, ujemny jako zero", () => {
  expect(hm(0)).toBe("0:00");
  expect(hm(8 * 3_600_000)).toBe("8:00");
  expect(hm(12 * 3_600_000 + 5 * 60_000 + 59_999)).toBe("12:05");
  expect(hm(-60_000)).toBe("0:00");
  expect(hhmm(3 * 3_600_000 + 12 * 60_000)).toBe("03:12");
  expect(hhmm(25 * 3_600_000)).toBe("25:00");
  // `target.ts` eksportuje te same funkcje.
  expect(targetHm).toBe(hm);
  expect(targetHhmm).toBe(hhmm);
});

test("plural: polska odmiana 1, 2–4 (bez 12–14) i reszta", () => {
  const word = (n: number) => plural(n, "zadanie", "zadania", "zadań");
  expect([0, 1, 2, 4, 5, 11, 12, 14, 21, 22, 24, 25, 112, 122].map(word)).toEqual([
    "zadań",
    "zadanie",
    "zadania",
    "zadania",
    "zadań",
    "zadań",
    "zadań",
    "zadań",
    "zadań",
    "zadania",
    "zadania",
    "zadań",
    "zadań",
    "zadania",
  ]);
  expect(plural(3, "dzień roboczy", "dni robocze", "dni roboczych")).toBe(
    "dni robocze",
  );
  expect(tsPlural(5, "wpis", "wpisy", "wpisów")).toBe("5 wpisów");
  expect(tsPlural(1, "wpis", "wpisy", "wpisów")).toBe("1 wpis");
});
