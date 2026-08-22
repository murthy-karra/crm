<script setup lang="ts">
// SLICE_004 §1 step 4, §10: the public `/invite/:token` page. States:
// loading, invalid (404), expired (410), used (409), valid -> form (display
// name, password with a 12-char minimum hint). Success -> `me` set from the
// response -> /today; router.ts's guard then routes a platform-only session
// on to /platform on its own, so this view never has to special-case that.
import { computed, ref } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import { useRouter } from 'vue-router'
import Card from '../components/Card.vue'
import FormField from '../components/FormField.vue'
import { fetchInvitationPreview, queryKeys, useAcceptInvitationMutation } from '../api/queries'
import { buttonClasses, INPUT_CLASSES } from '../lib/controls'
import { describeMutationError } from '../lib/errors'
import { deriveInviteState } from '../lib/inviteState'

const props = defineProps<{ token: string }>()

const router = useRouter()

const {
  data: preview,
  error: previewError,
  isPending: previewPending,
} = useQuery({
  queryKey: computed(() => queryKeys.invitationPreview(props.token)),
  queryFn: () => fetchInvitationPreview(props.token),
  // Terminal outcomes (404/409/410) are not transient — retrying just
  // delays the correct state; TanStack Query's default (one retry) would
  // otherwise leave the page on "Loading…" a moment longer for no benefit.
  retry: false,
})

const state = computed(() => deriveInviteState(previewPending.value, previewError.value))

const displayName = ref('')
const password = ref('')

const acceptMutation = useAcceptInvitationMutation()

function onSubmit() {
  acceptMutation.mutate(
    { token: props.token, display_name: displayName.value, password: password.value },
    { onSuccess: () => router.push('/today').catch(() => {}) },
  )
}

const roleArticle = computed(() => (preview.value?.role === 'admin' ? 'an admin' : 'a member'))
</script>

<template>
  <div class="flex min-h-screen items-center justify-center bg-surface-1 px-4">
    <Card class="w-full max-w-sm">
      <div
        v-if="state === 'loading'"
        class="py-4 text-center text-body text-text-muted"
      >
        Loading invitation…
      </div>

      <div
        v-else-if="state === 'invalid'"
        class="py-4 text-center"
      >
        <p class="text-body text-text">
          This invitation is no longer valid.
        </p>
      </div>

      <div
        v-else-if="state === 'expired'"
        class="py-4 text-center"
      >
        <p class="text-body text-text">
          This invitation has expired.
        </p>
      </div>

      <div
        v-else-if="state === 'used'"
        class="py-4 text-center"
      >
        <p class="text-body text-text">
          This invitation has already been used.
        </p>
      </div>

      <template v-else-if="state === 'valid' && preview">
        <div class="mb-6 text-center">
          <h1 class="text-section font-semibold text-text">
            {{ preview.organization_name }}
          </h1>
          <p class="mt-1 text-body text-text-muted">
            has invited {{ preview.email }} as {{ roleArticle }}
          </p>
        </div>

        <form
          class="space-y-4"
          @submit.prevent="onSubmit"
        >
          <FormField
            v-slot="{ id }"
            label="Display name"
            bare
          >
            <input
              :id="id"
              v-model="displayName"
              type="text"
              autocomplete="name"
              required
              :disabled="acceptMutation.isPending.value"
              :class="INPUT_CLASSES"
            >
          </FormField>

          <FormField
            v-slot="{ id }"
            label="Password"
            bare
            help-text="At least 12 characters."
          >
            <input
              :id="id"
              v-model="password"
              type="password"
              autocomplete="new-password"
              minlength="12"
              required
              :disabled="acceptMutation.isPending.value"
              :class="INPUT_CLASSES"
            >
          </FormField>

          <p
            v-if="acceptMutation.error.value"
            role="alert"
            class="text-small text-danger"
          >
            {{ describeMutationError(acceptMutation.error.value, 'Could not create your account. Try again.') }}
          </p>

          <button
            type="submit"
            class="w-full"
            :class="buttonClasses('primary')"
            :disabled="acceptMutation.isPending.value"
          >
            {{ acceptMutation.isPending.value ? 'Creating account…' : 'Create account' }}
          </button>
        </form>
      </template>
    </Card>
  </div>
</template>
