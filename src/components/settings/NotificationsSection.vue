<script setup lang="ts">
/** Ustawienia → Powiadomienia. */
import RemindersList from "./RemindersList.vue";
import type { AppSettings } from "../../settings";
import { meetingReminderMinutes } from "../../remindersapi";
import { checked, number, text, useSettingsSection } from "./section";
const settings = defineModel<AppSettings>("settings", { required: true });
const { busy, error, save } = useSettingsSection(settings);
</script>
<template>
  <p v-if="error" class="error" role="alert">{{ error }}</p>
  <h2>Powiadomienia</h2>
  <label class="inline-check"
    ><input
      :checked="settings.notifyDue"
      type="checkbox"
      :disabled="busy"
      @change="save({ notifyDue: checked($event) })"
    />Przypominaj o terminach na dziś i przekroczonych</label
  >
  <div class="planning-fields settings-fields">
    <label
      >Długi pomiar bez Stop (minuty, 0 wyłącza)<input
        :value="settings.notifyLongMinutes"
        type="number"
        min="0"
        max="1440"
        step="15"
        :disabled="busy"
        @change="save({ notifyLongMinutes: number($event) })"
    /></label>
    <label
      >Wieczorne „pomiar nadal trwa”<input
        :value="settings.notifyEvening"
        type="time"
        :disabled="busy || !settings.notifyEveningEnabled"
        @change="save({ notifyEvening: text($event) })"
    /></label>
  </div>
  <label class="inline-check"
    ><input
      :checked="settings.notifyEveningEnabled"
      type="checkbox"
      :disabled="busy"
      @change="save({ notifyEveningEnabled: checked($event) })"
    />Przypominaj wieczorem, że pomiar nadal trwa</label
  >
  <label class="inline-check"
    ><input
      :checked="settings.notifyTarget"
      type="checkbox"
      :disabled="busy"
      @change="save({ notifyTarget: checked($event) })"
    />Powiadom o osiągnięciu celu dnia</label
  >
  <label class="inline-check"
    ><input
      :checked="settings.notifyMorningReview"
      type="checkbox"
      :disabled="busy"
      @change="save({ notifyMorningReview: checked($event) })"
    />Przypominaj o porannym przeglądzie dnia</label
  >
  <label class="inline-check"
    ><input
      :checked="settings.notifyWeekReview"
      type="checkbox"
      :disabled="busy"
      @change="save({ notifyWeekReview: checked($event) })"
    />Przypominaj o przeglądzie tygodnia</label
  >
  <div class="planning-fields settings-fields">
    <label
      >Przegląd tygodnia o godzinie<input
        :value="settings.weekReviewTime"
        type="time"
        :disabled="busy || !settings.notifyWeekReview"
        @change="save({ weekReviewTime: text($event) })"
    /></label>
  </div>
  <div class="planning-fields settings-fields">
    <label
      >Przypomnij o „Czekam na” po (dni robocze, 0 wyłącza)<input
        :value="settings.waitingNotifyDays"
        type="number"
        min="0"
        max="30"
        step="1"
        :disabled="busy"
        @change="save({ waitingNotifyDays: number($event) })"
    /></label>
  </div>
  <label class="inline-check"
    ><input
      :checked="settings.meetingReminderEnabled"
      type="checkbox"
      :disabled="busy"
      @change="save({ meetingReminderEnabled: checked($event) })"
    />Przypominaj przed spotkaniami</label
  >
  <div class="planning-fields settings-fields">
    <label
      >Ile minut przed spotkaniem<select
        aria-label="Ile minut przed spotkaniem"
        :value="settings.meetingReminderMinutes"
        :disabled="busy || !settings.meetingReminderEnabled"
        @change="save({ meetingReminderMinutes: number($event) })"
      >
        <option v-for="m in meetingReminderMinutes" :key="m" :value="m">
          {{ m }} min
        </option>
      </select></label
    >
  </div>
  <p class="hint">
    Powiadomienia Windows: terminy przy starcie i o 09:00, długi pomiar raz
    na przedział, wieczór o wybranej godzinie, cel raz dziennie, przegląd
    dnia w dzień roboczy po początku dnia, dopóki nie klikniesz „Rozpocznij
    dzień”, przegląd tygodnia raz w ostatni dzień roboczy tygodnia o
    wybranej godzinie, „Czekam na” raz na blokadę po wybranej liczbie dni
    roboczych (z „Odblokuj” i „Przypomnij jutro”). Każde pojawia się
    najwyżej raz, także po ponownym uruchomieniu. Potwierdzenie skrótu
    Start/Stop pokazuje się, gdy widget jest schowany. Po „Zakończ dzień”
    przypomnienie o braku pomiaru, cel dnia i propozycje przełączenia milczą
    do północy albo do „Wznów dzień”. Przypomnienie przed spotkaniem
    (bez całodniowych i „Wolny”) działa także po „Zakończ dzień”; dla
    jednego spotkania zmienisz je albo wyłączysz w edytorze ręcznego spotkania,
    a dla spotkania z Outlooka prawym przyciskiem na osi dnia albo w Zadania →
    Dzisiaj.
  </p>
  <RemindersList />
</template>
