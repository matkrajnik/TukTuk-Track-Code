<script setup lang="ts">
/**
 * Animacja odsłonięcia Workflows po udanym odblokowaniu (PLAN_WDROZENIA.md, 4.1.2).
 * Dekoracja nad treścią: kilka węzłów, impuls biegnący przez graf, potem
 * graf odpływa w stronę panelu nawigacji. Nie przechwytuje kliknięć (Stop
 * timera działa), czytnik ekranu jej nie widzi (komunikat jest w Ustawieniach).
 * Escape pomija; przy ograniczonym ruchu kończy się od razu.
 */
import { onMounted, onUnmounted } from "vue";

const emit = defineEmits<{ done: [] }>();

/** Czas całej sekwencji (ms); CSS używa tych samych progów. */
const DURATION_MS = 1600;
const reducedMotion =
  typeof window.matchMedia === "function" &&
  window.matchMedia("(prefers-reduced-motion: reduce)").matches;

let timer: number | undefined;
function finish() {
  window.clearTimeout(timer);
  window.removeEventListener("keydown", onKey, true);
  emit("done");
}
function onKey(e: KeyboardEvent) {
  if (e.key === "Escape") finish();
}

onMounted(() => {
  if (reducedMotion) {
    finish();
    return;
  }
  window.addEventListener("keydown", onKey, true);
  timer = window.setTimeout(finish, DURATION_MS);
});
onUnmounted(() => {
  window.clearTimeout(timer);
  window.removeEventListener("keydown", onKey, true);
});
</script>

<template>
  <div class="workflow-reveal" aria-hidden="true" data-testid="workflow-reveal">
    <svg
      class="workflow-reveal-graph"
      viewBox="0 0 320 120"
      width="320"
      height="120"
    >
      <path class="reveal-edge" d="M40 60 H120" />
      <path class="reveal-edge" d="M140 60 H200" />
      <path class="reveal-edge" d="M220 60 H280" />
      <path
        class="reveal-edge reveal-loop"
        d="M210 70 C210 108 130 108 130 70"
      />
      <circle class="reveal-node" cx="30" cy="60" r="10" />
      <circle class="reveal-node" cx="130" cy="60" r="10" />
      <circle class="reveal-node" cx="210" cy="60" r="10" />
      <circle class="reveal-node" cx="290" cy="60" r="10" />
      <circle class="reveal-pulse" r="5" cx="30" cy="60" />
    </svg>
  </div>
</template>

<style scoped>
.workflow-reveal {
  position: fixed;
  inset: 0;
  z-index: 40;
  display: grid;
  place-items: center;
  pointer-events: none;
}
.workflow-reveal-graph {
  overflow: visible;
  animation: reveal-travel 1600ms cubic-bezier(0.4, 0, 0.2, 1) forwards;
}
.reveal-edge {
  fill: none;
  stroke: var(--accent);
  stroke-width: 2;
  stroke-linecap: round;
  stroke-dasharray: 90;
  stroke-dashoffset: 90;
  opacity: 0.7;
  animation: reveal-draw 450ms 200ms ease-out forwards;
}
.reveal-loop {
  stroke-dasharray: 4 6;
  animation-delay: 380ms;
}
.reveal-node {
  fill: var(--surface);
  stroke: var(--accent);
  stroke-width: 2;
  transform-box: fill-box;
  transform-origin: center;
  transform: scale(0);
  animation: reveal-pop 260ms ease-out forwards;
}
.reveal-node:nth-of-type(1) {
  animation-delay: 60ms;
}
.reveal-node:nth-of-type(2) {
  animation-delay: 180ms;
}
.reveal-node:nth-of-type(3) {
  animation-delay: 300ms;
}
.reveal-node:nth-of-type(4) {
  animation-delay: 420ms;
}
.reveal-pulse {
  fill: var(--accent);
  opacity: 0;
  animation: reveal-pulse 520ms 260ms ease-in-out forwards;
}
@keyframes reveal-draw {
  to {
    stroke-dashoffset: 0;
  }
}
@keyframes reveal-pop {
  70% {
    transform: scale(1.15);
  }
  to {
    transform: scale(1);
  }
}
@keyframes reveal-pulse {
  0% {
    opacity: 1;
    transform: translateX(0);
  }
  90% {
    opacity: 1;
    transform: translateX(260px);
  }
  to {
    opacity: 0;
    transform: translateX(260px);
  }
}
/* 650–1100 ms: graf odpływa i maleje w stronę panelu; potem wygasa. */
@keyframes reveal-travel {
  0%,
  40% {
    transform: none;
    opacity: 1;
  }
  70% {
    transform: translate(-36vw, 8vh) scale(0.35);
    opacity: 0.8;
  }
  100% {
    transform: translate(-40vw, 8vh) scale(0.25);
    opacity: 0;
  }
}
@media (prefers-reduced-motion: reduce) {
  .workflow-reveal {
    display: none;
  }
}
</style>
