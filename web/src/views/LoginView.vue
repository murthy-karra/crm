<script setup lang="ts">
// UI_STYLE.md §7: a quiet, centered glass sign-in panel.
import { ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
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
  <main class="flex min-h-dvh items-center justify-center bg-surface-0 px-5 py-10">
    <section
      aria-labelledby="login-heading"
      class="glass-panel w-full max-w-[400px] px-6 py-8 sm:px-9 sm:py-10"
    >
      <div class="mb-7 text-center">
        <img
          src="/brand/elysium-lockup-horizontal-name-black.svg"
          alt="Elysium CRM"
          width="164"
          height="48"
          class="mx-auto mb-7 h-12 w-[164px]"
        >
        <h1
          id="login-heading"
          class="text-title font-medium tracking-tight text-text"
        >
          Sign in
        </h1>
      </div>

      <form
        class="login-form space-y-5"
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
          {{ isPending ? 'Signing in…' : 'Sign in' }}
        </button>
      </form>
    </section>
  </main>
</template>

<style scoped>
.login-form :deep(label) {
  color: var(--color-text-muted);
  font-size: var(--text-small);
  font-weight: 400;
}

/* Keep iOS from zooming the viewport when a credential field receives focus. */
@media (max-width: 639px) {
  .login-form input {
    font-size: 16px;
  }
}
</style>
