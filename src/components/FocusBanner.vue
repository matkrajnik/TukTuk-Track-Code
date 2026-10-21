<script setup lang="ts">
import { computed, ref } from "vue";
import { Coffee, Timer } from "lucide-vue-next";
import { countdown, focusAction, useFocus, type FocusAction } from "../focus";
import { read, type Snapshot } from "../store";
import { clock } from "../time";

/**
 * Baner trybu skupienia (wzór: baner bezczynności): po ukończonym bloku
 * „Przerwa” / „Kolejny blok”, w czasie przerwy odliczanie, po przerwie
 * „Wracasz do X?” z „Rozpocznij”. Timer zmienia się tylko po kliknięciu.
 */
const props = defineProps<{ now: number }>();
const emit = defineEmits<{
  updated: [data: Snapshot];
  error: [message: string];
}>();
const focus = useFocus();
const busy = ref(false);
const minutes = computed(() => {
  const b = focus.value.ended;
  return b ? Math.round((b.end - b.start) / 60_000) : 0;
});

async function run(action: FocusAction) {
  if (busy.value) return;
  busy.value = true;
  emit("error", "");
  try {
    await focusAction(action);
    emit("updated", await read());
  } catch (e) {
    emit("error", String(e instanceof Error ? e.message : e));
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <section
    v-if="focus.ended"
    class="idle-banner focus-banner"
    aria-label="Koniec bloku skupienia"
  >
    <Timer class="infobar-icon" :size="20" aria-hidden="true" />
    <h2>Koniec bloku skupienia</h2>
    <p>
      „{{ focus.ended.title }}” · {{ minutes }} min. Przerwa czy kolejny blok?
    </p>
    <div class="planning-actions">
      <button class="primary-button" :disabled="busy" @click="run('break')">
        Przerwa</button
      ><button class="secondary-button" :disabled="busy" @click="run('next')">
        Kolejny blok</button
      ><button class="today-button" :disabled="busy" @click="run('dismiss')">
        Zamknij
      </button>
    </div>
  </section>
  <section
    v-else-if="focus.pause"
    class="idle-banner focus-banner"
    aria-label="Przerwa"
  >
    <Coffee class="infobar-icon" :size="20" aria-hidden="true" />
    <h2>
      Przerwa ·
      <span class="mono" aria-label="Pozostało z przerwy">{{
        countdown(focus.pause.until - props.now)
      }}</span>
    </h2>
    <p>
      Do {{ clock(focus.pause.until) }}.
      {{
        focus.pause.stops
          ? "Pomiar zatrzymany."
          : `Pomiar „${focus.pause.title}” trwa dalej.`
      }}
    </p>
    <div class="planning-actions">
      <button class="secondary-button" :disabled="busy" @click="run('end-break')">
        Zakończ przerwę
      </button>
    </div>
  </section>
  <section
    v-else-if="focus.resume"
    class="idle-banner focus-banner"
    aria-label="Powrót z przerwy"
  >
    <Timer class="infobar-icon" :size="20" aria-hidden="true" />
    <h2>Wracasz do „{{ focus.resume.title }}”?</h2>
    <p>Przerwa minęła. Pomiar ruszy dopiero po kliknięciu.</p>
    <div class="planning-actions">
      <button class="primary-button" :disabled="busy" @click="run('resume')">
        Rozpocznij</button
      ><button class="secondary-button" :disabled="busy" @click="run('next')">
        Kolejny blok</button
      ><button class="today-button" :disabled="busy" @click="run('dismiss')">
        Nie teraz
      </button>
    </div>
  </section>
</template>
