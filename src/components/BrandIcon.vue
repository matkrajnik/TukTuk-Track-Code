<script setup lang="ts">
// Sylwetka kiwi z design/logo/kiwi-mono.svg (wariant A), kolor z currentColor,
// żeby ikona marki w nagłówku brała barwę akcentu tak jak dotąd ikona lucide.
withDefaults(defineProps<{ size?: number }>(), { size: 18 });
</script>

<template>
  <svg
    xmlns="http://www.w3.org/2000/svg"
    :width="size"
    :height="size"
    viewBox="48 8 480 480"
    fill="currentColor"
    aria-hidden="true"
  >
    <g transform="translate(16 -48)">
      <path
        d="M198 398 194 452h42 M262 400l6 52h42"
        fill="none"
        stroke="currentColor"
        stroke-width="34"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
      <path d="M350 190 454 287 Q464 299 450 305 L322 254 Z" />
      <path
        fill-rule="evenodd"
        d="M80 290 C80 200 150 140 250 140 C302 140 344 162 360 210 C366 260 330 412 230 412 C140 412 80 370 80 290 Z M316 188a12 12 0 1 0 0.01 0Z"
      />
    </g>
  </svg>
</template>
