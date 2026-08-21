<script setup lang="ts">
import { onMounted, ref } from 'vue'

const apiBaseUrl = import.meta.env.VITE_API_BASE_URL ?? '/api'

interface UserInfo {
  id: string
  email: string
  display_name: string
}

interface OrganizationInfo {
  id: string
  name: string
}

interface Member {
  user_id: string
  display_name: string
  email: string
  joined_at: string
}

const user = ref<UserInfo | null>(null)
const organization = ref<OrganizationInfo | null>(null)
const members = ref<Member[]>([])
const email = ref('')
const password = ref('')
const loginError = ref<string | null>(null)
const loading = ref(false)
const checkingSession = ref(true)

async function fetchMembers() {
  const response = await fetch(`${apiBaseUrl}/organization/members`, { credentials: 'same-origin' })
  if (response.ok) {
    const body = await response.json()
    members.value = body.members
  }
}

async function loadSession() {
  try {
    const response = await fetch(`${apiBaseUrl}/me`, { credentials: 'same-origin' })
    if (response.ok) {
      const body = await response.json()
      user.value = body.user
      organization.value = body.organization
      await fetchMembers()
    }
  } finally {
    checkingSession.value = false
  }
}

onMounted(loadSession)

async function login() {
  loginError.value = null
  loading.value = true
  try {
    const response = await fetch(`${apiBaseUrl}/session`, {
      method: 'POST',
      credentials: 'same-origin',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email: email.value, password: password.value }),
    })
    if (!response.ok) {
      const body = await response.json().catch(() => null)
      loginError.value = body?.error ?? `login failed (${response.status})`
      return
    }
    const body = await response.json()
    user.value = body.user
    organization.value = body.organization
    password.value = ''
    await fetchMembers()
  } catch {
    loginError.value = 'network error'
  } finally {
    loading.value = false
  }
}

async function logout() {
  await fetch(`${apiBaseUrl}/session`, { method: 'DELETE', credentials: 'same-origin' })
  user.value = null
  organization.value = null
  members.value = []
  email.value = ''
}
</script>

<template>
  <main>
    <h1>CRM</h1>

    <p v-if="checkingSession">
      Checking session…
    </p>

    <form
      v-else-if="!user"
      @submit.prevent="login"
    >
      <h2>Log in</h2>
      <label>
        Email
        <input
          v-model="email"
          type="email"
          required
          autocomplete="username"
        >
      </label>
      <label>
        Password
        <input
          v-model="password"
          type="password"
          required
          autocomplete="current-password"
        >
      </label>
      <button
        type="submit"
        :disabled="loading"
      >
        {{ loading ? 'Logging in…' : 'Log in' }}
      </button>
      <p
        v-if="loginError"
        role="alert"
      >
        {{ loginError }}
      </p>
    </form>

    <section v-else>
      <p>Signed in as {{ user.display_name }} ({{ user.email }})</p>
      <p>Organization: {{ organization?.name }}</p>
      <button
        type="button"
        @click="logout"
      >
        Log out
      </button>

      <h2>Members</h2>
      <ul>
        <li
          v-for="member in members"
          :key="member.user_id"
        >
          {{ member.display_name }} — {{ member.email }}
        </li>
      </ul>
    </section>
  </main>
</template>
