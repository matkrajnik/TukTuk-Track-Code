<script setup lang="ts">
/**
 * Dialog „Otwórz worktree w Orce” (PLAN_BACKLOG.md, B9): nazwa z klucza i
 * tytułu, repozytorium (przypisany folder zadania albo lista z Orki), agent,
 * opcjonalny prompt i podgląd polecenia `orca worktree create`. Nic nie startuje
 * bez kliknięcia „Utwórz worktree”; polecenie pokazuje backend (`orca_command`),
 * więc podgląd jest tym, co zostanie uruchomione.
 */
import { computed, onMounted, ref, watch } from "vue";
import SearchSelect from "../SearchSelect.vue";
import type { SearchOption } from "../../searchSelect";
import {
  agentBindings,
  orcaCommand,
  orcaCreate,
  orcaFind,
  orcaNotFound,
  orcaNotRunning,
  orcaRepos,
  orcaStatus,
  orcaWorktreeName,
  type OrcaCreateRequest,
  type OrcaStatus,
  type OrcaWorktree,
  type Task,
} from "../../store";
import { issueRef } from "../../issue";
import { normFolder } from "../../settings";
import { useModal } from "../../composables/useModal";

const props = defineProps<{ task: Task; todayKey: string }>();
const emit = defineEmits<{ close: [] }>();
const { root, trapFocus } = useModal();

const issue = computed(() => issueRef(props.task));
const key = computed(() => issue.value?.key ?? "");
/** `--reference`: klucz Jiry albo adres zgłoszenia Redmine (bez adresu nic nie podpinamy). */
const reference = computed(() =>
  issue.value?.system === "jira" ? key.value : issue.value?.url || null,
);

const name = ref(orcaWorktreeName(key.value, props.task.title));
const repo = ref("");
const agent = ref<"" | "claude" | "codex">("claude");
const activate = ref(false);
/** Prompt z tytułu i notatki dnia (do poprawienia przed uruchomieniem). */
const prompt = ref(
  [
    `${key.value}: ${props.task.title}`,
    props.task.details?.notes?.[props.todayKey]?.trim() ?? "",
  ]
    .filter(Boolean)
    .join("\n\n"),
);

const status = ref<OrcaStatus | null>(null);
const repos = ref<{ path: string; name: string }[]>([]);
const bound = ref<string[]>([]);
const existing = ref<OrcaWorktree[]>([]);
const loading = ref(true);
const busy = ref(false);
const error = ref("");
const result = ref("");
const command = ref("");
const commandError = ref("");

const runnable = computed(
  () => !!status.value?.found && (status.value.running || status.value.autostart),
);
/** Komunikat o stanie Orki: brak pliku albo „Orca nie działa”. */
const notice = computed(() => {
  const s = status.value;
  if (!s) return "";
  if (!s.found) return orcaNotFound;
  if (!s.running && !s.autostart) return orcaNotRunning;
  return "";
});

const options = computed<SearchOption<string>[]>(() => {
  const seen = new Set<string>();
  const out: SearchOption<string>[] = [];
  for (const folder of bound.value) {
    seen.add(normFolder(folder));
    out.push({ value: folder, label: folder, group: "Przypisany folder zadania" });
  }
  for (const r of repos.value) {
    if (seen.has(normFolder(r.path))) continue;
    seen.add(normFolder(r.path));
    out.push({ value: r.path, label: `${r.name} · ${r.path}`, group: "Repozytoria Orki" });
  }
  return out;
});

const request = computed<OrcaCreateRequest>(() => ({
  name: name.value,
  repo: repo.value,
  reference: reference.value,
  agent: agent.value || null,
  prompt: agent.value ? prompt.value : null,
  activate: activate.value,
}));

let latest = 0;
async function refreshCommand() {
  const mine = ++latest;
  if (!repo.value) {
    command.value = "";
    commandError.value = "Wybierz repozytorium, żeby zobaczyć polecenie.";
    return;
  }
  try {
    const line = await orcaCommand(request.value);
    if (mine !== latest) return;
    command.value = line;
    commandError.value = "";
  } catch (e) {
    if (mine !== latest) return;
    command.value = "";
    commandError.value = e instanceof Error ? e.message : String(e);
  }
}
watch(request, refreshCommand, { deep: true });

async function loadOrca() {
  loading.value = true;
  error.value = "";
  try {
    status.value = await orcaStatus();
    if (runnable.value) {
      repos.value = await orcaRepos();
      if (reference.value) existing.value = await orcaFind(reference.value);
    }
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    loading.value = false;
  }
}

onMounted(async () => {
  try {
    bound.value = (await agentBindings())
      .filter((b) => b.taskId === props.task.id)
      .map((b) => b.folder);
  } catch {
    // Bez przypisań zostaje lista z Orki.
  }
  await loadOrca();
  await refreshCommand();
});
/** Pierwsze repozytorium z listy, dopóki użytkownik nie wybrał innego (także po „Sprawdź ponownie”). */
watch(options, (list) => {
  if (!repo.value && list.length) repo.value = list[0].value;
});

const canCreate = computed(
  () => runnable.value && !busy.value && !!repo.value && !!name.value.trim(),
);
async function create() {
  if (!canCreate.value) return;
  busy.value = true;
  error.value = "";
  result.value = "";
  try {
    const made = await orcaCreate(request.value);
    result.value = `Utworzono worktree: ${made.path}`;
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div
    class="modal-overlay"
    @click.self="emit('close')"
    @keydown.esc="emit('close')"
  >
    <section
      ref="root"
      class="modal orca-modal"
      role="dialog"
      aria-modal="true"
      aria-label="Otwórz worktree w Orce"
      @keydown="trapFocus"
    >
      <h2 tabindex="-1">Otwórz worktree w Orce · {{ key }}</h2>
      <p v-if="notice" class="error" role="alert">
        {{ notice }}
        <button
          v-if="status?.found"
          type="button"
          class="action-button"
          :disabled="loading"
          @click="loadOrca"
        >
          Sprawdź ponownie
        </button>
      </p>
      <p v-else-if="status && !status.running && status.autostart" class="hint">
        Orca nie działa; zostanie uruchomiona przy tworzeniu worktree
        (Ustawienia → Agenci).
      </p>
      <p v-if="existing.length" class="hint" role="status">
        Dla {{ key }} jest już worktree w Orce:
        {{ existing.map((w) => w.path).join(", ") }}.
      </p>
      <div class="planning-fields orca-fields">
        <label
          >Nazwa worktree<input
            v-model="name"
            maxlength="100"
            aria-label="Nazwa worktree"
        /></label>
        <label
          >Repozytorium<SearchSelect
            v-model="repo"
            label="Repozytorium"
            :options="options"
            placeholder="Wybierz repozytorium"
            empty-text="Brak repozytoriów"
        /></label>
        <label
          >Agent<select v-model="agent" aria-label="Agent">
            <option value="">Bez agenta</option>
            <option value="claude">Claude Code</option>
            <option value="codex">Codex</option>
          </select></label
        >
        <label class="orca-prompt"
          >Prompt dla agenta (opcjonalny)<textarea
            v-model="prompt"
            rows="4"
            maxlength="4000"
            aria-label="Prompt"
            :disabled="!agent"
          ></textarea
        ></label>
        <label class="inline-check"
          ><input v-model="activate" type="checkbox" />Przełącz Orkę na nowy
          worktree</label
        >
      </div>
      <p v-if="!repo && !loading && !options.length" class="hint">
        Zadanie nie ma przypisanego folderu, a Orca nie podała repozytoriów.
        Uruchom Orkę i dodaj repozytorium albo przypisz folder zadaniu.
      </p>
      <h3>Polecenie</h3>
      <pre
        v-if="command"
        class="orca-command mono"
        aria-label="Polecenie Orki"
        >{{ command }}</pre
      >
      <p v-else class="hint" aria-label="Polecenie Orki">{{ commandError }}</p>
      <p class="hint">
        Prompt trafia do polecenia w jednej linii. Nic nie uruchomi się bez
        kliknięcia „Utwórz worktree”.
      </p>
      <p v-if="error" class="error" role="alert">{{ error }}</p>
      <p v-if="result" class="hint orca-result" role="status">{{ result }}</p>
      <div class="modal-actions">
        <button type="button" @click="emit('close')">Zamknij</button>
        <button
          type="button"
          class="primary-button"
          :disabled="!canCreate"
          @click="create"
        >
          Utwórz worktree
        </button>
      </div>
    </section>
  </div>
</template>
