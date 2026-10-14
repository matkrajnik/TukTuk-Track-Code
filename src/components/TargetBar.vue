<script setup lang="ts">
import { computed } from "vue";
import { hhmm, progress } from "../target";

const props = defineProps<{ total: number; target: number; label: string }>();
const over = computed(() => props.total > props.target);
</script>
<template>
  <div
    class="target-bar"
    :class="{ over }"
    role="progressbar"
    :aria-label="label"
    aria-valuemin="0"
    :aria-valuemax="Math.round(target / 60000)"
    :aria-valuenow="Math.min(Math.floor(total / 60000), Math.round(target / 60000))"
    :aria-valuetext="hhmm(total) + ' / ' + hhmm(target)"
  >
    <span :style="{ width: `${progress(total, target)}%` }" />
  </div>
</template>
