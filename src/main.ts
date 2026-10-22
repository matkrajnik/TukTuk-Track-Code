import { createApp, type Component } from "vue";
import App from "./App.vue";
import ReminderWindow from "./ReminderWindow.vue";
import QuickSwitch from "./QuickSwitch.vue";
import WidgetWindow from "./WidgetWindow.vue";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./styles/base.css";
import "./styles/layout.css";
import "./styles/tasks.css";
import "./styles/reports.css";
import "./features.css";
import { migrateLocalStorage } from "./legacy";
// Klucze localStorage z poprzedniej nazwy aplikacji, zanim okna je odczytają.
migrateLocalStorage();
// Okna Tauri wybierają widok po etykiecie. W podglądzie (bez Tauri) widget
// otwiera się pod adresem #widget, żeby testy mogły go wyrenderować.
const label = isTauri()
  ? getCurrentWindow().label
  : location.hash === "#widget"
    ? "widget"
    : "main";
const windows: Record<string, Component> = {
  reminder: ReminderWindow,
  quick: QuickSwitch,
  widget: WidgetWindow,
};
createApp(windows[label] ?? App).mount("#app");
