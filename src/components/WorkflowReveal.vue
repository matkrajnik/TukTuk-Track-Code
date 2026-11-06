<script setup lang="ts">
/**
 * Animacja odsłonięcia Workflows po udanym odblokowaniu (PLAN_WDROZENIA.md, 4.1.2).
 * Dekoracja nad treścią: po krawędziach okna biegną dwa „ledy” (świetlne
 * smugi po obwodzie), w środku zapala się graf z impulsem biegnącym przez
 * węzły, a na końcu graf odpływa w stronę panelu nawigacji. Nie przechwytuje
 * kliknięć (Stop timera działa), czytnik ekranu jej nie widzi (komunikat jest
 * w Ustawieniach). Escape pomija; przy ograniczonym ruchu kończy się od razu.
 */
import { onMounted, onUnmounted } from "vue";

const emit = defineEmits<{ done: [] }>();

/** Czas całej sekwencji (ms); CSS używa tych samych progów. */
const DURATION_MS = 2600;
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
    <div class="reveal-glow" />
    <div class="reveal-frame reveal-frame-blur" />
    <div class="reveal-frame" />
    <svg
      class="workflow-reveal-graph"
      viewBox="0 0 320 120"
      width="480"
      height="180"
    >
      <defs>
        <filter id="reveal-neon" x="-50%" y="-50%" width="200%" height="200%">
          <feGaussianBlur stdDeviation="3" result="blur" />
          <feMerge>
            <feMergeNode in="blur" />
            <feMergeNode in="SourceGraphic" />
          </feMerge>
        </filter>
      </defs>
      <g filter="url(#reveal-neon)">
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
        <circle class="reveal-trail" r="9" cx="30" cy="60" />
        <circle class="reveal-pulse" r="5" cx="30" cy="60" />
      </g>
    </svg>
  </div>
</template>

<style scoped>
/* Kąt smugi po obwodzie: animowalny dzięki @property (Chromium/WebView2). */
@property --led-angle {
  syntax: "<angle>";
  inherits: false;
  initial-value: 0deg;
}

.workflow-reveal {
  /* Rdzeń „leda” i impulsu: biały na ciemnym tle, w jasnym motywie ciemny,
     bo biały zlałby się z --surface. */
  --reveal-core: #fff;
  position: fixed;
  inset: 0;
  z-index: 40;
  display: grid;
  place-items: center;
  pointer-events: none;
}
:root[data-theme="light"] .workflow-reveal {
  --reveal-core: color-mix(in srgb, var(--accent) 45%, var(--text));
}

/* Delikatny błysk tła w kolorze akcentu. */
.reveal-glow {
  position: absolute;
  inset: 0;
  background: radial-gradient(
    ellipse at center,
    color-mix(in srgb, var(--accent) 22%, transparent) 0%,
    transparent 60%
  );
  opacity: 0;
  animation: reveal-glow 2600ms ease-out forwards;
}

/* Dwa „ledy” biegnące po krawędzi okna: obracający się gradient stożkowy
   widoczny tylko w ramce (maska wycina środek). */
.reveal-frame {
  position: absolute;
  inset: 4px;
  border-radius: 12px;
  padding: 3px;
  background: conic-gradient(
    from var(--led-angle),
    transparent 0deg,
    transparent 120deg,
    color-mix(in srgb, var(--accent) 60%, transparent) 160deg,
    var(--reveal-core) 176deg,
    var(--accent) 180deg,
    transparent 182deg,
    transparent 300deg,
    color-mix(in srgb, var(--accent) 60%, transparent) 340deg,
    var(--reveal-core) 356deg,
    var(--accent) 360deg
  );
  will-change: background, opacity;
  -webkit-mask:
    linear-gradient(#000 0 0) content-box,
    linear-gradient(#000 0 0);
  -webkit-mask-composite: xor;
  mask:
    linear-gradient(#000 0 0) content-box exclude,
    linear-gradient(#000 0 0);
  opacity: 0;
  animation:
    reveal-led-spin 1100ms linear infinite,
    reveal-frame-fade 2600ms ease-in-out forwards;
}
.reveal-frame-blur {
  filter: blur(8px);
  padding: 6px;
}

.workflow-reveal-graph {
  width: min(480px, 80vw);
  height: auto;
  overflow: visible;
  animation: reveal-travel 2600ms cubic-bezier(0.4, 0, 0.2, 1) forwards;
}
.reveal-edge {
  fill: none;
  stroke: var(--accent);
  stroke-width: 2.5;
  stroke-linecap: round;
  stroke-dasharray: 90;
  stroke-dashoffset: 90;
  animation: reveal-draw 450ms 250ms ease-out forwards;
}
.reveal-loop {
  stroke-dasharray: 4 6;
  animation-delay: 450ms;
}
.reveal-node {
  fill: var(--surface);
  stroke: var(--accent);
  stroke-width: 2.5;
  transform-box: fill-box;
  transform-origin: center;
  transform: scale(0);
  animation:
    reveal-pop 300ms ease-out forwards,
    reveal-flash 260ms ease-out forwards;
}
/* Węzeł zapala się, gdy dobiega do niego impuls (opóźnienia jak w reveal-pulse). */
.reveal-node:nth-of-type(1) {
  animation-delay: 60ms, 700ms;
}
.reveal-node:nth-of-type(2) {
  animation-delay: 180ms, 900ms;
}
.reveal-node:nth-of-type(3) {
  animation-delay: 300ms, 1060ms;
}
.reveal-node:nth-of-type(4) {
  animation-delay: 420ms, 1240ms;
}
.reveal-pulse,
.reveal-trail {
  fill: var(--reveal-core);
  opacity: 0;
  animation: reveal-pulse 600ms 700ms ease-in-out forwards;
}
.reveal-trail {
  fill: var(--accent);
  animation-delay: 740ms;
}
@keyframes reveal-led-spin {
  to {
    --led-angle: 360deg;
  }
}
@keyframes reveal-frame-fade {
  0% {
    opacity: 0;
  }
  10%,
  70% {
    opacity: 1;
  }
  100% {
    opacity: 0;
  }
}
@keyframes reveal-glow {
  0% {
    opacity: 0;
  }
  25% {
    opacity: 1;
  }
  100% {
    opacity: 0;
  }
}
@keyframes reveal-draw {
  to {
    stroke-dashoffset: 0;
  }
}
@keyframes reveal-pop {
  70% {
    transform: scale(1.2);
  }
  to {
    transform: scale(1);
  }
}
@keyframes reveal-flash {
  0% {
    fill: var(--surface);
  }
  40% {
    fill: var(--accent);
  }
  to {
    fill: color-mix(in srgb, var(--accent) 35%, var(--surface));
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
/* Od ok. 1500 ms graf odpływa i maleje w stronę panelu; potem wygasa. */
@keyframes reveal-travel {
  0%,
  58% {
    transform: none;
    opacity: 1;
  }
  82% {
    transform: translate(-36vw, 8vh) scale(0.3);
    opacity: 0.8;
  }
  100% {
    transform: translate(-40vw, 8vh) scale(0.2);
    opacity: 0;
  }
}
@media (prefers-reduced-motion: reduce) {
  .workflow-reveal {
    display: none;
  }
}
</style>
