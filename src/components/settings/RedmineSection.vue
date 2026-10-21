<script setup lang="ts">
/** Ustawienia → Redmine: połączenie, projekty i zapisy (`RedmineSettings`). */
import RedmineSettings from "../RedmineSettings.vue";
import type { AppSettings } from "../../settings";
import { useSettingsSection } from "./section";
const settings = defineModel<AppSettings>("settings", { required: true });
const { busy, error, save } = useSettingsSection(settings);
</script>
<template>
  <p v-if="error" class="error" role="alert">{{ error }}</p>
  <h2>Redmine</h2>
  <RedmineSettings :settings="settings" :saving="busy" @save="save" />
</template>
