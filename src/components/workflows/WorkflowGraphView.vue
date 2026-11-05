<script setup lang="ts">
/**
 * Canvas grafu procesu (PLAN_WDROZENIA.md, 4.3): kroki w pozycjach z definicji,
 * połączenia z nazwami wyjść, przeciąganie kroków, powiększenie i dopasowanie.
 * Pozycje nie mają znaczenia wykonawczego. Pełną edycję klawiaturą daje lista
 * kroków obok; tutaj kliknięcie wybiera krok, a przeciągnięcie go przesuwa.
 */
import { computed, nextTick, onMounted, ref } from "vue";
import { Maximize2, ZoomIn, ZoomOut } from "lucide-vue-next";
import {
  nodeTypeNames,
  portNames,
  type WorkflowGraph,
  type WorkflowNode,
} from "../../api/workflows";

const props = defineProps<{
  graph: WorkflowGraph;
  selected: string | null;
  /** Kroki z problemami walidacji (obramowanie i znak „!”). */
  problems: ReadonlySet<string>;
}>();
const emit = defineEmits<{
  select: [id: string];
  move: [id: string, position: { x: number; y: number }];
}>();

const NODE_W = 168;
const NODE_H = 52;
const PADDING = 48;
const MIN_ZOOM = 0.4;
const MAX_ZOOM = 1.6;
const ZOOM_STEP = 0.2;
/** Wewnętrzny margines przewijanego obszaru (px), jak `--space-2`. */
const CANVAS_PADDING = 8;
/** Przesunięcie poniżej tego progu (px) to kliknięcie, nie przeciągnięcie. */
const DRAG_THRESHOLD = 3;

const zoom = ref(1);
/** Pozycja przeciąganego kroku przed zapisem (podgląd ruchu). */
const dragging = ref<{ id: string; x: number; y: number } | null>(null);

function position(node: WorkflowNode) {
  return dragging.value?.id === node.id
    ? { x: dragging.value.x, y: dragging.value.y }
    : node.position;
}

const bounds = computed(() => {
  const points = props.graph.nodes.map(position);
  if (!points.length) return { x: 0, y: 0, w: 400, h: 200 };
  const minX = Math.min(...points.map((p) => p.x)) - PADDING;
  const minY = Math.min(...points.map((p) => p.y)) - PADDING;
  const maxX = Math.max(...points.map((p) => p.x)) + NODE_W + PADDING;
  const maxY = Math.max(...points.map((p) => p.y)) + NODE_H + PADDING * 2;
  return { x: minX, y: minY, w: maxX - minX, h: maxY - minY };
});

const nodeById = computed(
  () => new Map(props.graph.nodes.map((n) => [n.id, n])),
);

const edges = computed(() =>
  props.graph.edges.flatMap((edge) => {
    const source = nodeById.value.get(edge.source);
    const target = nodeById.value.get(edge.target);
    if (!source || !target) return [];
    const s = position(source);
    const t = position(target);
    const sx = s.x + NODE_W;
    const sy = s.y + NODE_H / 2;
    const tx = t.x;
    const ty = t.y + NODE_H / 2;
    // Połączenie wstecz (pętla poprawek) łukiem pod krokami.
    const back = tx <= sx;
    const d = back
      ? `M ${sx} ${sy} C ${sx + 60} ${sy + 110}, ${tx - 60} ${ty + 110}, ${tx} ${ty}`
      : `M ${sx} ${sy} C ${sx + 50} ${sy}, ${tx - 50} ${ty}, ${tx} ${ty}`;
    const labelX = back ? (sx + tx) / 2 : (sx + tx) / 2;
    const labelY = back ? Math.max(sy, ty) + 72 : (sy + ty) / 2 - 6;
    return [
      {
        id: edge.id,
        d,
        back,
        label: edge.sourcePort === "out" ? "" : portNames[edge.sourcePort],
        labelX,
        labelY,
      },
    ];
  }),
);

function setZoom(next: number) {
  zoom.value = Math.min(
    MAX_ZOOM,
    Math.max(MIN_ZOOM, Math.round(next * 10) / 10),
  );
}

const scroller = ref<HTMLElement | null>(null);
/** Dopasowanie: cały graf mieści się w szerokości canvasu. */
function fit() {
  const width = scroller.value?.clientWidth ?? 0;
  if (!width || !bounds.value.w) return;
  const inner = width - 2 * CANVAS_PADDING;
  zoom.value = Math.min(1, Math.max(MIN_ZOOM, inner / bounds.value.w));
}
onMounted(() => void nextTick(fit));

let start: { id: string; clientX: number; clientY: number; x: number; y: number } | null =
  null;

function onPointerDown(node: WorkflowNode, e: PointerEvent) {
  if (e.button !== 0) return;
  (e.currentTarget as Element).setPointerCapture?.(e.pointerId);
  start = {
    id: node.id,
    clientX: e.clientX,
    clientY: e.clientY,
    x: node.position.x,
    y: node.position.y,
  };
}

function onPointerMove(e: PointerEvent) {
  if (!start) return;
  const dx = (e.clientX - start.clientX) / zoom.value;
  const dy = (e.clientY - start.clientY) / zoom.value;
  if (!dragging.value && Math.hypot(dx, dy) < DRAG_THRESHOLD) return;
  dragging.value = {
    id: start.id,
    x: Math.round(start.x + dx),
    y: Math.round(start.y + dy),
  };
}

function onPointerUp() {
  if (!start) return;
  const moved = dragging.value;
  const id = start.id;
  start = null;
  dragging.value = null;
  if (moved) emit("move", id, { x: moved.x, y: moved.y });
  else emit("select", id);
}

const summary = computed(
  () =>
    `Graf procesu: ${props.graph.nodes.length} kroków, ${props.graph.edges.length} połączeń. Edycja klawiaturą na liście kroków.`,
);
</script>

<template>
  <div class="wf-canvas">
    <div class="wf-canvas-tools" role="group" aria-label="Widok grafu">
      <button
        type="button"
        class="icon-button"
        aria-label="Pomniejsz"
        title="Pomniejsz"
        :disabled="zoom <= MIN_ZOOM"
        @click="setZoom(zoom - ZOOM_STEP)"
      >
        <ZoomOut :size="16" />
      </button>
      <span class="wf-zoom" aria-live="polite">{{ Math.round(zoom * 100) }}%</span>
      <button
        type="button"
        class="icon-button"
        aria-label="Powiększ"
        title="Powiększ"
        :disabled="zoom >= MAX_ZOOM"
        @click="setZoom(zoom + ZOOM_STEP)"
      >
        <ZoomIn :size="16" />
      </button>
      <button
        type="button"
        class="icon-button"
        aria-label="Dopasuj widok"
        title="Dopasuj widok"
        @click="fit"
      >
        <Maximize2 :size="16" />
      </button>
    </div>
    <div ref="scroller" class="wf-canvas-scroll">
      <svg
        role="img"
        :aria-label="summary"
        :viewBox="`${bounds.x} ${bounds.y} ${bounds.w} ${bounds.h}`"
        :width="bounds.w * zoom"
        :height="bounds.h * zoom"
        @pointermove="onPointerMove"
        @pointerup="onPointerUp"
        @pointercancel="onPointerUp"
      >
        <defs>
          <marker
            id="wf-arrow"
            viewBox="0 0 10 10"
            refX="9"
            refY="5"
            markerWidth="8"
            markerHeight="8"
            orient="auto-start-reverse"
          >
            <path d="M 0 0 L 10 5 L 0 10 z" class="wf-arrow" />
          </marker>
        </defs>
        <g v-for="edge in edges" :key="edge.id">
          <path
            :d="edge.d"
            class="wf-edge"
            :class="{ back: edge.back }"
            marker-end="url(#wf-arrow)"
          />
          <text
            v-if="edge.label"
            :x="edge.labelX"
            :y="edge.labelY"
            class="wf-edge-label"
            text-anchor="middle"
          >
            {{ edge.label }}
          </text>
        </g>
        <g
          v-for="node in graph.nodes"
          :key="node.id"
          class="wf-node"
          :class="[
            `wf-node-${node.kind.type}`,
            {
              selected: node.id === selected,
              problem: problems.has(node.id),
              dragging: dragging?.id === node.id,
            },
          ]"
          :transform="`translate(${position(node).x} ${position(node).y})`"
          @pointerdown="onPointerDown(node, $event)"
        >
          <rect :width="NODE_W" :height="NODE_H" rx="8" />
          <text x="12" y="20" class="wf-node-type">
            {{ nodeTypeNames[node.kind.type] }}{{ problems.has(node.id) ? " · !" : "" }}
          </text>
          <text x="12" y="39" class="wf-node-label">
            {{ node.label.length > 22 ? node.label.slice(0, 21) + "…" : node.label }}
          </text>
        </g>
      </svg>
    </div>
  </div>
</template>

<style scoped>
.wf-canvas {
  display: flex;
  flex-direction: column;
  min-width: 0;
  border: 1px solid var(--line);
  border-radius: var(--radius);
  background: var(--bg);
}
.wf-canvas-tools {
  display: flex;
  align-items: center;
  gap: var(--space-1);
  padding: var(--space-1) var(--space-2);
  border-bottom: 1px solid var(--line);
}
.wf-zoom {
  min-width: 3.5em;
  color: var(--muted);
  font-size: var(--fs-xs);
  text-align: center;
  font-variant-numeric: tabular-nums;
}
.wf-canvas-scroll {
  overflow: auto;
  max-height: 460px;
  padding: var(--space-2);
}
svg {
  display: block;
  touch-action: none;
  user-select: none;
}
.wf-edge {
  fill: none;
  stroke: var(--line-strong);
  stroke-width: 1.5;
}
.wf-edge.back {
  stroke-dasharray: 5 4;
}
.wf-arrow {
  fill: var(--line-strong);
}
.wf-edge-label {
  fill: var(--muted);
  font-size: 11px;
}
.wf-node {
  cursor: grab;
}
.wf-node.dragging {
  cursor: grabbing;
}
.wf-node rect {
  fill: var(--surface);
  stroke: var(--line-strong);
  stroke-width: 1;
}
.wf-node-trigger rect,
.wf-node-end rect {
  fill: var(--raised);
}
.wf-node.selected rect {
  stroke: var(--accent);
  stroke-width: 2.5;
}
.wf-node.problem rect {
  stroke: var(--warning);
  stroke-width: 2;
  stroke-dasharray: 4 3;
}
.wf-node-type {
  fill: var(--muted);
  font-size: 11px;
}
.wf-node-label {
  fill: var(--text);
  font-size: 13px;
  font-weight: 600;
}
</style>
