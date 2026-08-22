<script setup lang="ts">
// UI_STYLE.md §10: "Login — a single centered card."
import { ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import Card from '../components/Card.vue'
import FormField from '../components/FormField.vue'
import { useLoginMutation } from '../api/queries'
import { ApiError } from '../api/client'
import { buttonClasses, INPUT_CLASSES } from '../lib/controls'

const route = useRoute()
const router = useRouter()

const email = ref('')
const password = ref('')

const { mutate: login, isPending, error } = useLoginMutation()

function loginErrorMessage(err: unknown): string {
  if (err instanceof ApiError) {
    switch (err.code) {
      case 'invalid_credentials':
        return 'Incorrect email or password.'
      case 'no_membership':
        return 'This account is not a member of any Organization.'
      case 'unavailable':
        return 'The server is temporarily unavailable. Try again.'
      default:
        return 'Something went wrong. Try again.'
    }
  }
  return 'Something went wrong. Try again.'
}

function onSubmit() {
  login(
    { email: email.value, password: password.value },
    {
      onSuccess: () => {
        const redirect = route.query.redirect
        router.push(typeof redirect === 'string' ? redirect : '/today').catch(() => {})
      },
    },
  )
}
</script>

<template>
  <div class="flex min-h-screen items-center justify-center bg-surface-1 px-4">
    <Card class="w-full max-w-sm">
      <div class="mb-6 text-center">
        <h1 class="text-section font-semibold text-text">
          CRM
        </h1>
        <p class="mt-1 text-body text-text-muted">
          Sign in to continue
        </p>
      </div>

      <form
        class="space-y-4"
        @submit.prevent="onSubmit"
      >
        <FormField
          v-slot="{ id }"
          label="Email"
          bare
        >
          <input
            :id="id"
            v-model="email"
            type="email"
            autocomplete="username"
            required
            :class="INPUT_CLASSES"
          >
        </FormField>

        <FormField
          v-slot="{ id }"
          label="Password"
          bare
        >
          <input
            :id="id"
            v-model="password"
            type="password"
            autocomplete="current-password"
            required
            :class="INPUT_CLASSES"
          >
        </FormField>

        <p
          v-if="error"
          role="alert"
          class="text-small text-danger"
        >
          {{ loginErrorMessage(error) }}
        </p>

        <button
          type="submit"
          class="w-full"
          :class="buttonClasses('primary')"
          :disabled="isPending"
        >
          {{ isPending ? 'Signing in…' : 'Log in' }}
        </button>
      </form>
    </Card>
  </div>
</template>
