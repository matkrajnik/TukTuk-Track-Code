import { createApp } from "vue";
import App from "./App.vue";
import ReminderWindow from "./ReminderWindow.vue";
import QuickSwitch from "./QuickSwitch.vue";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./style.css";
import "./features.css";
createApp(
  isTauri() && getCurrentWindow().label === "reminder" ? ReminderWindow : isTauri() && getCurrentWindow().label === "quick" ? QuickSwitch : App,
).mount("#app");
