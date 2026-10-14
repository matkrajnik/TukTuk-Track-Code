<script setup lang="ts">
import { computed, ref } from "vue";
import type { Snapshot } from "../store";
import { hhmm } from "../target";
import {
  atRisk,
  biggestMisses,
  buckets,
  estimateRows,
  histogram,
  percent,
  periods,
  projectMedians,
  ratioText,
  shareText,
  summarize,
  type EstimatePeriod,
} from "../estimates";

const props = defineProps<{ data: Snapshot; now: number }>();
defineEmits<{ report: [day: string] }>();

const period = ref<EstimatePeriod>("30");
const rows = computed(() => estimateRows(props.data, props.now, period.value));
const summary = computed(() => summarize(rows.value));
const counts = computed(() => histogram(rows.value));
const maxCount = computed(() => Math.max(1, ...counts.value));
const projects = computed(() => projectMedians(rows.value));
const misses = computed(() => biggestMisses(rows.value));
const risky = computed(() => atRisk(props.data, props.now));
const stats = computed(() => [
  { label: "Zadania", value: String(summary.value.count) },
  {
    label: "Mediana",
    value: summary.value.count ? ratioText(summary.value.median) : "—",
  },
  {
    label: "W widełkach ±20%",
    value: summary.value.count ? shareText(summary.value.within) : "—",
  },
  {
    label: "Przekroczenia",
    value: summary.value.count ? shareText(summary.value.over) : "—",
  },
]);
</script>

<template>
  <section class="dashboard-card estimate-card" aria-label="Trafność szacunków">
    <div class="estimate-heading">
      <h2>Trafność szacunków</h2>
      <div
        class="tabs estimate-periods"
        role="group"
        aria-label="Okres trafności"
      >
        <button
          v-for="item in periods"
          :key="item.value"
          :class="{ selected: period === item.value }"
          :aria-pressed="period === item.value"
          @click="period = item.value"
        >
          {{ item.label }}
        </button>
      </div>
    </div>
    <p class="report-note">
      Ukończone zadania z szacunkiem; stosunek czasu rzeczywistego do planu,
      okres według dnia ostatniej pracy.
    </p>
    <dl class="estimate-stats" aria-label="Wskaźniki trafności">
      <div v-for="item in stats" :key="item.label">
        <dt>{{ item.label }}</dt>
        <dd class="mono">{{ item.value }}</dd>
      </div>
    </dl>
    <p v-if="!summary.count" class="report-note">
      Brak ukończonych zadań z szacunkiem w tym okresie.
    </p>
    <template v-else>
      <div class="estimate-columns">
        <div>
          <h3>Rozkład rzeczywistość / plan</h3>
          <div aria-label="Histogram trafności" role="list">
            <div
              v-for="(bucket, index) in buckets"
              :key="bucket.label"
              class="estimate-bucket"
              role="listitem"
              :aria-label="'Koszyk ' + bucket.label + ': ' + counts[index]"
            >
              <span class="mono">{{ bucket.label }}</span
              ><span class="dashboard-bar"
                ><span
                  :class="{ over: index > 2 }"
                  :style="{
                    width: `${(counts[index] / maxCount) * 100}%`,
                  }" /></span
              ><span class="mono">{{ counts[index] }}</span>
            </div>
          </div>
        </div>
        <div>
          <h3>Mediana według projektu</h3>
          <table class="estimate-table" aria-label="Mediana według projektu">
            <thead>
              <tr>
                <th>Projekt</th>
                <th>Zadania</th>
                <th>Mediana</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="row in projects" :key="row.project">
                <td>{{ row.project }}</td>
                <td class="mono">{{ row.count }}</td>
                <td class="mono">{{ ratioText(row.median) }}</td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
      <h3>Największe rozjazdy</h3>
      <ol class="estimate-misses" aria-label="Największe rozjazdy">
        <li v-for="row in misses" :key="row.task.id">
          <span class="estimate-miss-title">{{ row.task.title }}</span>
          <span class="mono"
            >{{ hhmm(row.actual) }} / {{ hhmm(row.estimate) }} ·
            <span :class="{ 'estimate-over': row.ratio > 1 }">{{
              ratioText(row.ratio)
            }}</span></span
          >
          <button
            class="estimate-day mono"
            :aria-label="'Raport ' + row.lastDay + ': ' + row.task.title"
            @click="$emit('report', row.lastDay)"
          >
            {{ row.lastDay }}
          </button>
        </li>
      </ol>
    </template>
  </section>
  <section class="dashboard-card estimate-card" aria-label="Zagrożone teraz">
    <h2>Zagrożone teraz</h2>
    <p v-if="!risky.length" class="report-note">
      Żadne otwarte zadanie nie przekroczyło szacunku.
    </p>
    <div v-for="row in risky" :key="row.task.id" class="dashboard-count">
      <span>{{ row.task.title }}</span
      ><strong class="mono"
        >{{ hhmm(row.actual) }} / {{ hhmm(row.estimate) }} ·
        <span class="estimate-over"
          >{{ percent(row.actual, row.estimate) }}%</span
        ></strong
      >
    </div>
  </section>
</template>
