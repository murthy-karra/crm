<script setup lang="ts">
import { RouterView, useRoute } from 'vue-router'
import { computed } from 'vue'
import { useWorkspaceEpoch, useWorkspacePending } from './workspaceLifecycle'
import AppShell from './components/AppShell.vue'

const route = useRoute()
const workspaceEpoch = useWorkspaceEpoch()
const workspacePending = useWorkspacePending()
// Only the migration flow retains its reviewed confirmation across this fence.
// Ordinary views mount after /me succeeds so their first read has current authority.
const mountPrivateRoute = computed(() => !workspacePending.value || route.name === 'manage-migration')
const viewKey = computed(() => route.name === 'manage-migration' ? route.fullPath : `${route.fullPath}:${workspaceEpoch.value}`)
</script>

<template>
  <AppShell v-if="!route.meta.public">
    <RouterView
      v-if="mountPrivateRoute"
      :key="viewKey"
    />
  </AppShell>
  <RouterView v-else />
</template>
