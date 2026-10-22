/** Podgląd szablonów zadań (`templates.rs`). */
import {
  templateEvent,
  type TaskTemplate,
  type TemplateBody,
} from "../api/task";
const templatesKey = "tuktuk-track-preview-templates-v1";
const TEMPLATE_NAME = /^[\p{L}\p{N}_-]{1,60}$/u;
export const sortTemplates = (list: TaskTemplate[]) =>
  [...list].sort((a, b) =>
    a.name
      .toLocaleLowerCase("pl")
      .localeCompare(b.name.toLocaleLowerCase("pl"), "pl"),
  );
export function previewTemplates(): TaskTemplate[] {
  try {
    return JSON.parse(localStorage.getItem(templatesKey) ?? "[]");
  } catch {
    return [];
  }
}
export const findTemplate = (list: TaskTemplate[], name: string) =>
  list.find(
    (t) =>
      t.name.toLocaleLowerCase("pl") === name.trim().toLocaleLowerCase("pl"),
  );
/** Walidacja jak `templates::validate`; zwraca oczyszczoną treść. */
function validTemplate(name: string, body: TemplateBody): TemplateBody {
  if (!TEMPLATE_NAME.test(name))
    throw new Error(
      "Nazwa szablonu może mieć od 1 do 60 liter, cyfr, „-” lub „_” (bez spacji).",
    );
  const clean = {
    title: body.title.trim(),
    estimateMinutes: body.estimateMinutes,
    steps: body.steps.map((s) => s.trim()).filter(Boolean),
    project: body.project.trim(),
    priority: body.priority,
  };
  if ([...clean.title].length > 240)
    throw new Error("Tytuł szablonu może mieć najwyżej 240 znaków.");
  if ([...clean.project].length > 80)
    throw new Error("Nazwa projektu może mieć najwyżej 80 znaków.");
  if (
    ![-1, 0, 1].includes(clean.priority) ||
    !Number.isInteger(clean.estimateMinutes) ||
    clean.estimateMinutes < 0 ||
    clean.estimateMinutes > 600_000
  )
    throw new Error("Niepoprawny priorytet lub szacunek szablonu.");
  if (clean.steps.length > 100 || clean.steps.some((s) => [...s].length > 240))
    throw new Error("Szablon może mieć najwyżej 100 kroków po 240 znaków.");
  return clean;
}
function previewTemplatesChanged(list: TaskTemplate[]) {
  localStorage.setItem(templatesKey, JSON.stringify(list));
  window.dispatchEvent(new CustomEvent(templateEvent));
}
export async function saveTemplate(
  id: number | null,
  name: string,
  body: TemplateBody,
): Promise<void> {
  const clean = validTemplate(name, body);
  const list = previewTemplates();
  const other = findTemplate(list, name);
  if (other && other.id !== id)
    throw new Error(`Szablon „${other.name}” już istnieje.`);
  if (id !== null) {
    const current = list.find((t) => t.id === id);
    if (!current) throw new Error("Nie znaleziono szablonu.");
    Object.assign(current, clean, { name });
  } else
    list.push({
      id: Math.max(0, ...list.map((t) => t.id)) + 1,
      name,
      ...clean,
    });
  previewTemplatesChanged(list);
}
export async function deleteTemplate(id: number): Promise<void> {
  const list = previewTemplates();
  if (!list.some((t) => t.id === id))
    throw new Error("Nie znaleziono szablonu.");
  previewTemplatesChanged(list.filter((t) => t.id !== id));
}
/** Lista szablonów podglądu według nazwy (`templates_list`). */
export function templatesList(): TaskTemplate[] {
  return sortTemplates(previewTemplates());
}
