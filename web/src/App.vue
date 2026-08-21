<script setup lang="ts">
import { onMounted, ref } from 'vue'

const apiBaseUrl = import.meta.env.VITE_API_BASE_URL ?? '/api'
const status = ref<'checking' | 'ok' | 'error'>('checking')

onMounted(async () => {
  try {
    const response = await fetch(`${apiBaseUrl}/health`)
    if (!response.ok) throw new Error(`status ${response.status}`)
    const body = await response.json()
    status.value = body.status === 'ok' ? 'ok' : 'error'
  } catch {
    status.value = 'error'
  }
})
</script>

<template>
  <main>
    <h1>CRM</h1>
    <p>API health: {{ status }}</p>
  </main>
</template>
