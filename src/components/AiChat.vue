<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { Bot, Send, Square, SquarePen, X } from "lucide-vue-next";
import { applyProposal } from "../aiproposals";
import { aiEngineName } from "../api/ai";
import {
  cardTitle,
  chat,
  chatWidthRange,
  contextLabel,
  newConversation,
  quickText,
  sendMessage,
  setChatOpen,
  setChatWidth,
  stopChat,
  type ChatCard,
} from "../aichat";
import { mcpProposalResolve, proposalName, proposalText } from "../mcpapi";
import { openView } from "../navigation";
import { useSettings } from "../target";
import type { Snapshot, Task } from "../store";

/**
 * Czat w panelu po prawej (A3): rozmowa z asystentem, który czyta dane przez
 * serwer MCP. Model tylko proponuje: zapisy są kartami „Zastosuj” / „Odrzuć” /
 * „Popraw”, a „Zastosuj” wykonuje istniejące komendy, jak baner propozycji
 * agentów. Pod odpowiedzią lista użytych narzędzi; pod wiadomością „Pokaż, co
 * zostało wysłane” (tekst po filtrze danych osobowych).
 */
const props = defineProps<{ tasks: Task[] }>();
const emit = defineEmits<{
  updated: [data: Snapshot];
  error: [message: string];
}>();
const settings = useSettings();
const input = ref<HTMLTextAreaElement | null>(null);
const log = ref<HTMLElement | null>(null);

async function submit() {
  const text = chat.draft;
  if (!text.trim() || chat.busy) return;
  await sendMessage(text);
}

/** Karta: stan i błąd po kliknięciu; zapis robi istniejąca komenda. */
async function apply(card: ChatCard) {
  if (card.state !== "open") return;
  const proposal = card.draft
    ? { ...card.proposal, text: card.draft.text.trim(), day: card.draft.day }
    : card.proposal;
  if (card.draft) {
    if (card.proposal.kind === "note_append" && !proposal.text) {
      card.error = "Wpisz treść notatki.";
      return;
    }
    if (!/^\d{4}-\d{2}-\d{2}$/.test(proposal.day)) {
      card.error = "Wybierz dzień.";
      return;
    }
  }
  card.state = "applying";
  card.error = "";
  try {
    const data = await applyProposal(proposal, props.tasks);
    // Zmiana zapisana; dopiero potem propozycja znika z kolejki.
    await mcpProposalResolve(card.proposal.id);
    card.state = "applied";
    emit("updated", data);
  } catch (e) {
    card.state = "open";
    card.error = String(e instanceof Error ? e.message : e);
  }
}
async function dismiss(card: ChatCard) {
  if (card.state !== "open") return;
  card.error = "";
  try {
    await mcpProposalResolve(card.proposal.id);
    card.state = "dismissed";
  } catch (e) {
    card.error = String(e instanceof Error ? e.message : e);
  }
}
/**
 * „Popraw” z wypełnionymi polami:
 * - nowe zadanie: pole „Nowe zadanie” w składni szybkiego dodawania (cała
 *   propozycja przechodzi do pola, więc schodzi z kolejki),
 * - notatka i plan: pola treści i dnia w karcie; „Zastosuj” zapisuje poprawione
 *   wartości tymi samymi komendami, a do tego czasu propozycja czeka w kolejce
 *   („Anuluj poprawkę” wraca do niej bez zmian),
 * - folder i przełączenie nie mają pól: zadanie otwiera się na liście, a karta
 *   zostaje z „Zastosuj” / „Odrzuć”.
 */
async function edit(card: ChatCard) {
  if (card.state !== "open") return;
  card.error = "";
  const p = card.proposal;
  if (p.kind === "note_append" || p.kind === "plan") {
    card.draft = { text: p.text, day: p.day };
    return;
  }
  if (p.kind !== "task_create") {
    if (p.taskId != null) openView({ view: "tasks", task: p.taskId });
    card.info = "Otwarto zadanie na liście. Propozycja czeka na decyzję.";
    return;
  }
  try {
    await mcpProposalResolve(p.id);
    card.state = "edited";
    openView({ view: "tasks", newTask: quickText(p) });
  } catch (e) {
    card.error = String(e instanceof Error ? e.message : e);
  }
}
const stateText: Record<string, string> = {
  applied: "Zastosowano.",
  dismissed: "Odrzucono, nic się nie zmieniło.",
  edited: "Otwarto do poprawy w zwykłym formularzu.",
};

// Szerokość: przeciąganie krawędzi albo strzałki na uchwycie; zapamiętana.
let dragging = false;
function drag(e: PointerEvent) {
  if (dragging) setChatWidth(window.innerWidth - e.clientX);
}
function stopDrag() {
  dragging = false;
  window.removeEventListener("pointermove", drag);
  window.removeEventListener("pointerup", stopDrag);
}
function startDrag(e: PointerEvent) {
  e.preventDefault();
  dragging = true;
  window.addEventListener("pointermove", drag);
  window.addEventListener("pointerup", stopDrag);
}
function resizeKey(e: KeyboardEvent) {
  const step = e.shiftKey ? 60 : 20;
  if (e.key === "ArrowLeft") setChatWidth(chat.width + step);
  else if (e.key === "ArrowRight") setChatWidth(chat.width - step);
  else return;
  e.preventDefault();
}
onUnmounted(stopDrag);

function scrollDown() {
  void nextTick(() => {
    if (log.value) log.value.scrollTop = log.value.scrollHeight;
  });
}
watch(
  () => chat.messages.map((m) => m.text.length + m.tools.length + m.cards.length),
  scrollDown,
  { deep: true },
);
watch(
  () => chat.focus,
  () => void nextTick(() => input.value?.focus()),
);
onMounted(() => {
  scrollDown();
  void nextTick(() => input.value?.focus());
});
</script>

<template>
  <aside class="chat-panel" aria-label="Asystent AI">
    <div
      class="chat-resizer"
      role="separator"
      aria-orientation="vertical"
      aria-label="Szerokość asystenta"
      tabindex="0"
      :aria-valuenow="chat.width"
      :aria-valuemin="chatWidthRange.min"
      :aria-valuemax="chatWidthRange.max"
      @pointerdown="startDrag"
      @keydown="resizeKey"
    />
    <div class="chat-inner">
      <header class="chat-head">
        <Bot :size="18" aria-hidden="true" />
        <h2>Asystent</h2>
        <small v-if="aiEngineName(settings.aiEngine)">{{
          aiEngineName(settings.aiEngine)
        }}</small>
        <span class="chat-head-spacer" />
        <button
          class="today-button"
          :disabled="!chat.messages.length && !chat.context"
          title="Zakończ tę rozmowę i zacznij od początku"
          @click="newConversation"
        >
          <SquarePen :size="12" />Nowa rozmowa
        </button>
        <button
          class="icon-button"
          aria-label="Schowaj asystenta"
          title="Schowaj asystenta (Ctrl+J)"
          @click="setChatOpen(false)"
        >
          <X :size="16" />
        </button>
      </header>
      <p v-if="chat.context" class="chat-context">
        <span>{{ contextLabel(chat.context) }}</span
        ><button
          class="icon-button"
          aria-label="Usuń kontekst rozmowy"
          title="Usuń kontekst"
          @click="chat.context = null"
        >
          <X :size="12" />
        </button>
      </p>
      <div
        ref="log"
        class="chat-log"
        role="log"
        aria-live="polite"
        aria-label="Rozmowa z asystentem"
        tabindex="0"
      >
        <p v-if="!chat.messages.length" class="hint chat-empty">
          Zapytaj o swój dzień, zadania albo poproś o propozycję zmiany.
          Asystent czyta dane z aplikacji, a niczego nie zapisze bez Twojego
          kliknięcia. Imiona i nazwiska są ukrywane.
        </p>
        <article
          v-for="m in chat.messages"
          :key="m.id"
          class="chat-message"
          :class="m.role"
          :aria-label="m.role === 'user' ? 'Twoja wiadomość' : 'Odpowiedź asystenta'"
        >
          <template v-if="m.role === 'user'">
            <p v-if="m.context" class="chat-message-context">{{ m.context }}</p>
            <p class="chat-text">{{ m.text }}</p>
            <details v-if="m.sent" class="ai-preview">
              <summary>Pokaż, co zostało wysłane</summary>
              <pre aria-label="Tekst wysłany do modelu">{{ m.sent }}</pre>
            </details>
          </template>
          <template v-else>
            <p v-if="m.text" class="chat-text">{{ m.text }}</p>
            <p v-else-if="m.pending" class="hint" role="status">
              Asystent pisze…
            </p>
            <ul v-if="m.tools.length" class="chat-tools" aria-label="Użyte narzędzia">
              <li v-for="(tool, i) in m.tools" :key="i" :class="{ failed: tool.error }">
                {{ tool.label }}{{ tool.error ? " (błąd)" : "" }}
              </li>
            </ul>
            <section
              v-for="card in m.cards"
              :key="card.proposal.id"
              class="chat-card"
              :aria-label="`Propozycja: ${proposalName(card.proposal)}`"
            >
              <h3>{{ cardTitle(card.proposal.kind) }}</h3>
              <p>{{ proposalText(card.proposal) }}</p>
              <div
                v-if="card.draft && (card.state === 'open' || card.state === 'applying')"
                class="chat-card-fields"
              >
                <label v-if="card.proposal.kind === 'note_append'">
                  Treść notatki
                  <textarea
                    v-model="card.draft.text"
                    rows="3"
                    maxlength="1000"
                    :disabled="card.state === 'applying'"
                  />
                </label>
                <label>
                  {{ card.proposal.kind === "plan" ? "Dzień planu" : "Dzień notatki" }}
                  <input
                    v-model="card.draft.day"
                    type="date"
                    required
                    :disabled="card.state === 'applying'"
                  />
                </label>
              </div>
              <p v-if="card.info && card.state === 'open'" class="hint" role="status">
                {{ card.info }}
              </p>
              <p v-if="card.error" class="error" role="alert">{{ card.error }}</p>
              <span
                v-if="card.state === 'open' || card.state === 'applying'"
                class="planning-actions"
              >
                <button
                  class="secondary-button"
                  :disabled="card.state === 'applying'"
                  :aria-label="`Zastosuj propozycję: ${proposalName(card.proposal)}`"
                  @click="apply(card)"
                >
                  Zastosuj</button
                ><button
                  class="today-button"
                  :disabled="card.state === 'applying'"
                  :aria-label="`Odrzuć propozycję: ${proposalName(card.proposal)}`"
                  @click="dismiss(card)"
                >
                  Odrzuć</button
                ><button
                  v-if="card.draft"
                  class="today-button"
                  :disabled="card.state === 'applying'"
                  :aria-label="`Anuluj poprawkę: ${proposalName(card.proposal)}`"
                  @click="card.draft = null"
                >
                  Anuluj poprawkę</button
                ><button
                  v-else
                  class="today-button"
                  :disabled="card.state === 'applying'"
                  :aria-label="`Popraw propozycję: ${proposalName(card.proposal)}`"
                  title="Otwórz zwykły formularz z wypełnionymi polami"
                  @click="edit(card)"
                >
                  Popraw
                </button>
              </span>
              <p v-else class="hint" role="status">{{ stateText[card.state] }}</p>
            </section>
            <p v-if="m.error" class="error" role="alert">{{ m.error }}</p>
            <p v-if="m.stopped" class="hint">Zatrzymano.</p>
          </template>
        </article>
      </div>
      <form class="chat-form" @submit.prevent="submit">
        <textarea
          ref="input"
          v-model="chat.draft"
          aria-label="Wiadomość do asystenta"
          placeholder="Zapytaj asystenta…"
          rows="2"
          maxlength="4000"
          @keydown.enter.exact.prevent="submit"
        />
        <div class="chat-form-actions">
          <span class="hint">Enter wysyła, Shift+Enter nowy wiersz</span>
          <button
            v-if="chat.busy"
            type="button"
            class="stop-button"
            @click="stopChat"
          >
            <Square :size="12" />Zatrzymaj
          </button>
          <button v-else class="primary-button" :disabled="!chat.draft.trim()">
            <Send :size="14" />Wyślij
          </button>
        </div>
      </form>
    </div>
  </aside>
</template>
