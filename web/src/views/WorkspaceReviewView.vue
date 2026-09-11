<script setup lang="ts">
import { ref } from 'vue'
import PageHeader from '../components/PageHeader.vue'
import Card from '../components/Card.vue'
import { refreshWorkspace } from '../workspaceLifecycle'
import { buttonClasses } from '../lib/controls'
const busy = ref(false)
const error = ref<string | null>(null)
async function refresh() {
  if (busy.value) return
  busy.value = true; error.value = null
  try { await refreshWorkspace() } catch { error.value = 'Could not verify workspace status. Try again.' }
  finally { busy.value = false }
}
</script>
<template>
  <div data-testid="workspace-waiting">
    <PageHeader title="Workspace under review" />
    <Card class="max-w-2xl">
      <p class="text-body text-text">
        Your Organization’s administrators are reviewing imported records.
      </p>
      <p class="mt-2 text-body text-text-muted">
        People, Today, calls and the AI Operator will remain unavailable until a later activation. You can refresh this status or log out.
      </p>
      <p
        v-if="error"
        role="alert"
        class="mt-3 text-small text-danger"
      >
        {{ error }}
      </p>
      <button
        type="button"
        class="mt-4"
        :class="buttonClasses('secondary')"
        :disabled="busy"
        data-testid="workspace-status-refresh"
        @click="refresh"
      >
        {{ busy ? 'Refreshing…' : 'Refresh status' }}
      </button>
    </Card>
  </div>
</template>
