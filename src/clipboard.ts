/**
 * Kopiuje tekst raportu: najpierw API schowka, potem zaznaczenie pola
 * i `execCommand("copy")`, na końcu zostawia zaznaczenie do Ctrl+C.
 * Zwraca komunikat dla użytkownika.
 */
export async function copyReport(
  text: string,
  area: HTMLTextAreaElement | null,
): Promise<string> {
  try {
    await navigator.clipboard.writeText(text);
    return "Skopiowano raport.";
  } catch {
    area?.focus();
    area?.select();
    try {
      if (document.execCommand("copy")) return "Skopiowano raport.";
    } catch {}
    return "Zaznaczono raport. Naciśnij Ctrl+C, aby skopiować.";
  }
}
