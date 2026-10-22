<script setup lang="ts">
/** Ustawienia → Redmine: połączenie, projekty i zapisy (`RedmineSettings`). */
import LegacyReconnect from "./LegacyReconnect.vue";
import RedmineSettings from "../RedmineSettings.vue";
import type { AppSettings } from "../../settings";
import { useSettingsSection } from "./section";
const settings = defineModel<AppSettings>("settings", { required: true });
const { busy, error, save } = useSettingsSection(settings);
</script>
<template>
  <p v-if="error" class="error" role="alert">{{ error }}</p>
  <h2>Redmine</h2>
  <LegacyReconnect kind="redmine" />
  <RedmineSettings :settings="settings" :saving="busy" @save="save" />
</template>
