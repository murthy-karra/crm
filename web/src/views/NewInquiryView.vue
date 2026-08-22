<script setup lang="ts">
// UI_STYLE.md §7 + docs/specs/SLICE_002.md §10: a card stack — source, name
// pair, email, phone, message, assignee — with the primary "Add lead"
// button right-aligned below the stack (not in a card, not sticky).
//
// Deliberately does NOT require email or phone client-side: submitting a
// lead with neither is a valid, intended flow this slice (§1 walkthrough
// step 6) that lands in the Unresolved queue — a client-side gate here
// would silently block a real product behavior.
import { computed, ref } from 'vue'
import { RouterLink, useRouter } from 'vue-router'
import Select from 'primevue/select'
import Card from '../components/Card.vue'
import FormField from '../components/FormField.vue'
import PageHeader from '../components/PageHeader.vue'
import { useCreateInquiryMutation, useMe, useMembers } from '../api/queries'
import type { ReceiveInquiryPayload, ReceiveInquiryRequest, ReceiveInquiryResponse } from '../api/types'
import { ApiError } from '../api/client'
import { buttonClasses, INPUT_CLASSES, selectPt, TEXTAREA_CLASSES } from '../lib/controls'
import { UNRESOLVED_REASON_LABEL } from '../lib/labels'

const SOURCE_SUGGESTIONS = ['zillow', 'realtor_com', 'website', 'referral', 'manual']

const router = useRouter()
const { data: me } = useMe()
const orgId = computed(() => me.value?.organization.id ?? '')

const { data: membersData, isPending: membersPending } = useMembers(orgId)
const assigneeOptions = computed(() => [
  { id: '', display_name: 'Me (default)' },
  ...(membersData.value?.members ?? []).map((member) => ({ id: member.user_id, display_name: member.display_name })),
])

const source = ref('')
const firstName = ref('')
const lastName = ref('')
const email = ref('')
const phone = ref('')
const message = ref('')
const assignTo = ref('')

// Stable for this visit to the form: a retry of the same submission (e.g. a
// double-click, or resubmitting after a network blip) carries the same id
// and dedupes server-side; navigating to the form fresh gets a new one, so
// a genuinely new lead is never mistaken for a retry (spec §3).
const submissionId = crypto.randomUUID()

const { mutate: createInquiry, isPending, error: submitError } = useCreateInquiryMutation(orgId)
const unresolvedOutcome = ref<Extract<ReceiveInquiryResponse, { status: 'unresolved' }> | null>(null)

function buildRequest(): ReceiveInquiryRequest {
  const payload: ReceiveInquiryPayload = { submission_id: submissionId }
  if (firstName.value.trim()) payload.first_name = firstName.value.trim()
  if (lastName.value.trim()) payload.last_name = lastName.value.trim()
  if (email.value.trim()) payload.email = email.value.trim()
  if (phone.value.trim()) payload.phone = phone.value.trim()
  if (message.value.trim()) payload.message = message.value.trim()

  const request: ReceiveInquiryRequest = {
    source: source.value.trim().toLowerCase(),
    payload,
  }
  if (assignTo.value) request.assign_to_user_id = assignTo.value
  return request
}

function submitErrorMessage(err: unknown): string {
  if (err instanceof ApiError) {
    switch (err.code) {
      case 'malformed_request':
        return 'Check the source field — it must be lowercase letters, numbers, or underscores.'
      case 'invalid_assignee':
        return 'The chosen assignee is no longer a member of this Organization.'
      case 'unavailable':
        return 'The server is temporarily unavailable. Try again shortly.'
      default:
        return 'Something went wrong. Try again.'
    }
  }
  return 'Something went wrong. Try again.'
}

function onSubmit() {
  unresolvedOutcome.value = null
  createInquiry(buildRequest(), {
    onSuccess: (response) => {
      if (response.status === 'resolved') {
        router.push(`/people/${response.person_id}`).catch(() => {})
        return
      }
      unresolvedOutcome.value = response
    },
  })
}
</script>

<template>
  <div>
    <PageHeader
      title="New lead"
      subtitle="Enter what you know — a lead with no email or phone still gets recorded, in the Unresolved queue."
    />

    <form
      class="space-y-4"
      @submit.prevent="onSubmit"
    >
      <FormField
        v-slot="{ id }"
        label="Source"
        description="Where this lead came from."
      >
        <input
          :id="id"
          v-model="source"
          type="text"
          required
          placeholder="e.g. zillow"
          :class="INPUT_CLASSES"
        >
        <div class="mt-2 flex flex-wrap gap-2">
          <button
            v-for="suggestion in SOURCE_SUGGESTIONS"
            :key="suggestion"
            type="button"
            class="rounded-md border border-border bg-surface-0 px-2 py-1 text-small text-text-muted transition-colors duration-150 ease-out hover:bg-surface-2 hover:text-text"
            @click="source = suggestion"
          >
            {{ suggestion }}
          </button>
        </div>
      </FormField>

      <Card>
        <p class="mb-3 text-body font-medium text-text">
          Name
        </p>
        <div class="grid grid-cols-2 gap-4">
          <FormField
            v-slot="{ id }"
            label="First name"
            bare
          >
            <input
              :id="id"
              v-model="firstName"
              type="text"
              :class="INPUT_CLASSES"
            >
          </FormField>
          <FormField
            v-slot="{ id }"
            label="Last name"
            bare
          >
            <input
              :id="id"
              v-model="lastName"
              type="text"
              :class="INPUT_CLASSES"
            >
          </FormField>
        </div>
      </Card>

      <FormField
        v-slot="{ id }"
        label="Email"
      >
        <input
          :id="id"
          v-model="email"
          type="email"
          placeholder="name@example.com"
          :class="INPUT_CLASSES"
        >
      </FormField>

      <FormField
        v-slot="{ id }"
        label="Phone"
      >
        <input
          :id="id"
          v-model="phone"
          type="tel"
          placeholder="(555) 555-5555"
          :class="INPUT_CLASSES"
        >
      </FormField>

      <FormField
        v-slot="{ id }"
        label="Message"
        description="Optional notes from the lead."
      >
        <textarea
          :id="id"
          v-model="message"
          rows="4"
          :class="TEXTAREA_CLASSES"
        />
      </FormField>

      <FormField
        label="Assignee"
        description="Leave as-is to assign this lead to yourself."
      >
        <Select
          v-model="assignTo"
          :options="assigneeOptions"
          option-label="display_name"
          option-value="id"
          aria-label="Assignee"
          :loading="membersPending"
          :pt="selectPt()"
          class="w-full"
        />
      </FormField>

      <div
        v-if="unresolvedOutcome"
        class="rounded-xl border border-border bg-surface-0 p-5 text-body text-text"
      >
        <p class="font-medium text-text">
          This lead could not be linked to a person.
        </p>
        <p class="mt-1 text-text-muted">
          Reason: {{ UNRESOLVED_REASON_LABEL[unresolvedOutcome.reason] }}. It has been recorded in the
          <RouterLink
            to="/intake/unresolved"
            class="text-accent hover:underline"
          >
            Unresolved queue
          </RouterLink>.
          You can adjust the details above and submit again.
        </p>
      </div>

      <p
        v-if="submitError"
        role="alert"
        class="text-small text-danger"
      >
        {{ submitErrorMessage(submitError) }}
      </p>

      <div class="flex justify-end">
        <button
          type="submit"
          :class="buttonClasses('primary')"
          :disabled="isPending"
        >
          {{ isPending ? 'Adding…' : 'Add lead' }}
        </button>
      </div>
    </form>
  </div>
</template>
