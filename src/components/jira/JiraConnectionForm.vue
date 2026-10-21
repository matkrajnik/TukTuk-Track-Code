<script setup lang="ts">
/**
 * Połączenie konta Jiry (adres, e-mail, token API) z własnym `busy` i błędem.
 * Token trafia tylko do `jira_connect` i znika z pola po każdej próbie.
 */
import { ref, watch } from "vue";
import { ExternalLink } from "lucide-vue-next";
import {
  jiraConnect,
  jiraDisconnect,
  jiraOpen,
  type JiraAccount,
} from "../../store";

const props = defineProps<{ account: JiraAccount }>();
const emit = defineEmits<{
  connected: [account: JiraAccount];
  disconnected: [account: JiraAccount];
}>();
const site = ref(props.account.config.site),
  email = ref(props.account.config.email),
  apiToken = ref("");
// Adres i e-mail z konta po odczycie, połączeniu i rozłączeniu.
watch(
  () => props.account,
  (account) => {
    site.value = account.config.site;
    email.value = account.config.email;
  },
);
const busy = ref(false),
  error = ref("");
async function run(fn: () => Promise<void>) {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    await fn();
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  } finally {
    busy.value = false;
  }
}
const connect = () =>
  run(async () => {
    let account: JiraAccount;
    try {
      account = await jiraConnect(site.value, email.value, apiToken.value);
    } finally {
      apiToken.value = "";
    }
    emit("connected", account);
  });
const disconnect = () =>
  run(async () => {
    const account = await jiraDisconnect();
    apiToken.value = "";
    emit("disconnected", account);
  });
const openTokens = () =>
  run(() => jiraOpen("https://id.atlassian.com/manage-profile/security/api-tokens"));
</script>

<template>
  <p v-if="error" class="error" role="alert">{{ error }}</p>
  <form class="jira-connection" @submit.prevent="connect">
    <h2>
      {{ account.connected ? "Połączenie z Jirą" : "Połącz swoje konto" }}
    </h2>
    <div class="connection-fields">
      <label
        >Adres Jiry<input
          v-model="site"
          type="url"
          required
          aria-label="Adres Jiry"
          :disabled="busy" /></label
      ><label
        >E-mail Atlassian<input
          v-model="email"
          type="email"
          required
          autocomplete="username"
          aria-label="E-mail Atlassian"
          :disabled="busy" /></label
      ><label
        >Token API<input
          v-model="apiToken"
          type="password"
          required
          autocomplete="off"
          aria-label="Token API"
          :disabled="busy"
      /></label>
    </div>
    <p class="hint">
      Użyj tokenu API bez zakresów. Token zostanie zapisany w magazynie
      poświadczeń Windows.
    </p>
    <div class="connection-actions">
      <button class="text-link" type="button" :disabled="busy" @click="openTokens">
        Utwórz token Atlassian <ExternalLink :size="13" /></button
      ><button
        v-if="account.connected"
        type="button"
        class="danger"
        :disabled="busy"
        @click="disconnect"
      >
        Rozłącz konto</button
      ><button class="primary-button" :disabled="busy">
        {{ busy ? "Łączenie…" : "Połącz i sprawdź" }}
      </button>
    </div>
  </form>
</template>
