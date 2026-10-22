<script setup lang="ts">
/**
 * Treść zakładki Ustawienia: jedna sekcja naraz (`components/settings/*`),
 * każda z własnym `busy` i błędem. Panel trzyma wspólne ustawienia
 * (`v-model:settings` sekcji) i informacje o aplikacji.
 */
import { onMounted, onUnmounted, ref } from "vue";
import WorkSection from "./settings/WorkSection.vue";
import FocusSection from "./settings/FocusSection.vue";
import WindowsSection from "./settings/WindowsSection.vue";
import AgentsSection from "./settings/AgentsSection.vue";
import AiSection from "./settings/AiSection.vue";
import JiraSection from "./settings/JiraSection.vue";
import RedmineSection from "./settings/RedmineSection.vue";
import TimesheetSection from "./settings/TimesheetSection.vue";
import CalendarSection from "./settings/CalendarSection.vue";
import NotificationsSection from "./settings/NotificationsSection.vue";
import TemplatesSection from "./settings/TemplatesSection.vue";
import ShortcutsSection from "./settings/ShortcutsSection.vue";
import StartupSection from "./settings/StartupSection.vue";
import type { SettingsSection } from "../navigation";
import {
  defaultSettings,
  onSettingsChanged,
  onShortcutsChanged,
  readAppInfo,
  readSettings,
  type AppInfo,
  type AppSettings,
} from "../settings";
import { loadTemplates } from "../store";
/** Widoczna sekcja; zakładka Ustawienia pokazuje jedną naraz. */
defineProps<{ section: SettingsSection }>();
const settings = ref<AppSettings>({
  ...defaultSettings,
  workDays: [...defaultSettings.workDays],
});
const info = ref<AppInfo>({
  version: "",
  shortcutErrors: { quick: "", toggle: "" },
});
/** Błąd odczytu ustawień przy otwarciu panelu. */
const error = ref("");
let unlisten: (() => void) | undefined;
let unlistenShortcuts: (() => void) | undefined;
let alive = true;
onMounted(async () => {
  try {
    const stop = await onSettingsChanged((s) => (settings.value = s));
    // Panel mógł zostać odmontowany w trakcie rejestracji nasłuchu.
    if (!alive) return stop();
    unlisten = stop;
    const stopShortcuts = await onShortcutsChanged(
      (errors) => (info.value = { ...info.value, shortcutErrors: errors }),
    );
    if (!alive) return stopShortcuts();
    unlistenShortcuts = stopShortcuts;
    settings.value = await readSettings();
    info.value = await readAppInfo();
    await loadTemplates();
  } catch (e) {
    if (alive) error.value = String(e);
  }
});
onUnmounted(() => {
  alive = false;
  unlisten?.();
  unlistenShortcuts?.();
});
</script>
<template>
  <section class="dashboard-card settings-panel">
    <p v-if="error" class="error" role="alert">{{ error }}</p>
    <WorkSection v-if="section === 'work'" v-model:settings="settings" />
    <FocusSection v-if="section === 'focus'" v-model:settings="settings" />
    <WindowsSection v-if="section === 'windows'" v-model:settings="settings" />
    <AgentsSection v-if="section === 'agents'" v-model:settings="settings" />
    <AiSection v-if="section === 'ai'" v-model:settings="settings" />
    <JiraSection v-if="section === 'jira'" v-model:settings="settings" />
    <RedmineSection v-if="section === 'redmine'" v-model:settings="settings" />
    <TimesheetSection
      v-if="section === 'timesheet'"
      v-model:settings="settings"
    />
    <CalendarSection v-if="section === 'calendar'" v-model:settings="settings" />
    <NotificationsSection
      v-if="section === 'notifications'"
      v-model:settings="settings"
    />
    <TemplatesSection v-if="section === 'templates'" />
    <ShortcutsSection
      v-if="section === 'shortcuts'"
      v-model:settings="settings"
      :shortcut-errors="info.shortcutErrors"
    />
    <StartupSection
      v-if="section === 'startup'"
      v-model:settings="settings"
      :version="info.version"
    />
    <small v-if="info.version">TukTuk Track {{ info.version }}</small>
  </section>
</template>
