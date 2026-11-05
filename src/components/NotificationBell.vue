<script setup lang="ts">
/**
 * Dzwoneczek w górnym pasku: historia powiadomień (`inbox.ts`) z licznikiem
 * nieprzeczytanych. Kliknięcie wpisu oznacza go jako przeczytany i otwiera
 * widok z powiadomienia (ten sam `navigate` co kliknięcie toastu).
 */
import { computed, nextTick, onMounted, onUnmounted, ref } from "vue";
import { Bell, CheckCheck, Trash2 } from "lucide-vue-next";
import {
  clearInbox,
  markInboxRead,
  onInboxChanged,
  readInbox,
  unreadCount,
  type InboxEntry,
} from "../inbox";
import { openView } from "../navigation";
import { clock, dayKey, dayText } from "../time";

const emit = defineEmits<{ error: [message: string] }>();

const entries = ref<InboxEntry[]>([]);
const open = ref(false);
const root = ref<HTMLElement | null>(null);
const toggle = ref<HTMLButtonElement | null>(null);
const panel = ref<HTMLElement | null>(null);
const unread = computed(() => unreadCount(entries.value));
const label = computed(() =>
  unread.value
    ? `Powiadomienia: nieprzeczytane ${unread.value}`
    : "Powiadomienia",
);

function when(at: number): string {
  const day = dayKey(at);
  return day === dayKey(Date.now())
    ? clock(at)
    : `${dayText(day)}, ${clock(at)}`;
}

async function run(action: () => Promise<InboxEntry[]>) {
  try {
    entries.value = await action();
  } catch (e) {
    emit("error", String(e instanceof Error ? e.message : e));
  }
}

async function toggleOpen() {
  open.value = !open.value;
  if (!open.value) return;
  await run(readInbox);
  await nextTick();
  panel.value?.focus();
}
function close(focusButton: boolean) {
  open.value = false;
  if (focusButton) toggle.value?.focus();
}
function focusOut(event: FocusEvent) {
  const to = event.relatedTarget as Node | null;
  if (open.value && to && !root.value?.contains(to)) open.value = false;
}
function outside(event: PointerEvent) {
  if (open.value && !root.value?.contains(event.target as Node))
    open.value = false;
}
// Escape na dokumencie: po „Oznacz wszystkie” przycisk staje się nieaktywny
// i fokus spada na body, więc obsługa na samym panelu by go nie złapała.
function escape(event: KeyboardEvent) {
  if (!open.value || event.key !== "Escape") return;
  const inside = root.value?.contains(document.activeElement);
  if (!inside && document.activeElement !== document.body) return;
  event.preventDefault();
  close(true);
}

async function choose(entry: InboxEntry) {
  if (!entry.read) await run(() => markInboxRead(entry.id));
  close(false);
  if (entry.open?.view) openView(entry.open);
}

const unsubscribers: (() => void)[] = [];
let unmounted = false;
onMounted(async () => {
  document.addEventListener("pointerdown", outside);
  document.addEventListener("keydown", escape);
  try {
    // Zdarzenie, które przyszło w trakcie odczytu, jest nowsze od jego wyniku.
    let changed = false;
    const off = await onInboxChanged((list) => {
      changed = true;
      entries.value = list;
    });
    if (unmounted) off();
    else unsubscribers.push(off);
    const list = await readInbox();
    if (!changed) entries.value = list;
  } catch (e) {
    // Licznik jest pomocniczy: lista odczyta się ponownie po otwarciu panelu.
    console.warn("Nie udało się odczytać historii powiadomień:", e);
  }
});
onUnmounted(() => {
  unmounted = true;
  document.removeEventListener("pointerdown", outside);
  document.removeEventListener("keydown", escape);
  unsubscribers.forEach((off) => off());
});
</script>

<template>
  <div ref="root" class="inbox" @focusout="focusOut">
    <button
      ref="toggle"
      class="icon-button inbox-toggle"
      :aria-label="label"
      :title="label"
      aria-haspopup="dialog"
      :aria-expanded="open"
      @click="toggleOpen"
    >
      <Bell :size="18" aria-hidden="true" /><span
        v-if="unread > 0"
        class="inbox-count"
        aria-hidden="true"
        >{{ unread > 99 ? "99+" : unread }}</span
      >
    </button>
    <section
      v-if="open"
      ref="panel"
      class="inbox-panel"
      role="dialog"
      aria-label="Historia powiadomień"
      tabindex="-1"
    >
      <header class="inbox-head">
        <h2>Powiadomienia</h2>
        <button
          class="icon-button"
          :disabled="unread === 0"
          aria-label="Oznacz wszystkie jako przeczytane"
          title="Oznacz wszystkie jako przeczytane"
          @click="run(() => markInboxRead())"
        >
          <CheckCheck :size="16" /></button
        ><button
          class="icon-button"
          :disabled="entries.length === 0"
          aria-label="Wyczyść historię powiadomień"
          title="Wyczyść historię"
          @click="run(clearInbox)"
        >
          <Trash2 :size="16" />
        </button>
      </header>
      <p v-if="entries.length === 0" class="inbox-empty">
        Brak powiadomień. Tu zobaczysz spotkania, przypomnienia i nowe
        zgłoszenia z Jiry i Redmine.
      </p>
      <ul v-else class="inbox-list">
        <li v-for="entry in entries" :key="entry.id">
          <button
            class="inbox-item"
            :class="{ unread: !entry.read }"
            :aria-label="`${entry.read ? '' : 'Nieprzeczytane: '}${entry.title}. ${entry.body}`"
            @click="choose(entry)"
          >
            <span class="inbox-dot" aria-hidden="true" />
            <span class="inbox-text">
              <span class="inbox-title">{{ entry.title }}</span>
              <span v-if="entry.body" class="inbox-body">{{ entry.body }}</span>
              <time
                class="inbox-time"
                :datetime="new Date(entry.at).toISOString()"
                >{{ when(entry.at) }}</time
              >
            </span>
          </button>
        </li>
      </ul>
    </section>
  </div>
</template>

<style scoped>
.inbox {
  position: relative;
}
.inbox-toggle {
  position: relative;
}
.inbox-count {
  position: absolute;
  top: 0;
  right: 0;
  min-width: 16px;
  padding: 0 4px;
  border-radius: 8px;
  background: var(--warning);
  color: var(--bg);
  font-size: var(--fs-xs);
  font-weight: 600;
  line-height: 16px;
  text-align: center;
  font-variant-numeric: tabular-nums;
  pointer-events: none;
}
.inbox-panel {
  position: absolute;
  z-index: 40;
  top: calc(100% + 6px);
  right: 0;
  display: flex;
  flex-direction: column;
  width: min(380px, calc(100vw - 32px));
  max-height: min(480px, calc(100vh - 80px));
  border: 1px solid var(--line);
  border-radius: var(--radius);
  background: var(--surface);
  box-shadow: 0 8px 24px rgb(0 0 0 / 30%);
}
.inbox-panel:focus {
  outline: none;
}
.inbox-head {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 8px 8px 8px 14px;
  border-bottom: 1px solid var(--line);
}
.inbox-head h2 {
  margin: 0 auto 0 0;
  font-size: var(--fs-sm);
  font-weight: 600;
}
.inbox-empty {
  margin: 0;
  padding: 16px 14px;
  color: var(--muted);
  font-size: var(--fs-sm);
}
.inbox-list {
  margin: 0;
  padding: 4px;
  list-style: none;
  overflow-y: auto;
}
.inbox-item {
  display: flex;
  justify-content: flex-start;
  align-items: flex-start;
  gap: 10px;
  width: 100%;
  padding: 8px 10px;
  border-radius: calc(var(--radius) - 2px);
  text-align: left;
}
.inbox-item:hover,
.inbox-item:focus-visible {
  background: var(--raised);
}
.inbox-dot {
  flex: none;
  width: 8px;
  height: 8px;
  margin-top: 6px;
  border-radius: 50%;
}
.inbox-item.unread .inbox-dot {
  background: var(--warning);
}
.inbox-text {
  display: flex;
  flex: 1;
  flex-direction: column;
  align-items: flex-start;
  gap: 2px;
  min-width: 0;
}
.inbox-title {
  font-size: var(--fs-sm);
  color: var(--muted);
}
.inbox-item.unread .inbox-title {
  color: var(--text);
  font-weight: 600;
}
.inbox-body {
  font-size: var(--fs-sm);
  color: var(--muted);
  overflow-wrap: anywhere;
}
.inbox-time {
  font-size: var(--fs-xs);
  color: var(--muted);
  font-variant-numeric: tabular-nums;
}
</style>
