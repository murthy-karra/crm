<script setup lang="ts">
// SLICE_004 §10: "Invite button -> dialog (email, role) -> on 201, a
// one-time copy-link panel". This component owns only the form phase;
// MembersView.vue and PlatformOrganizationView.vue own the mutation and, on
// success, close this dialog and open OneTimeLinkDialog.vue with the
// result — keeping "the panel is shown exactly once, closing is final" a
// property of the caller's state, not something this dialog has to track
// across re-opens.
import { ref, watch } from 'vue'
import Dialog from 'primevue/dialog'
import Select from 'primevue/select'
import FormField from './FormField.vue'
import type { MembershipRole } from '../api/types'
import { buttonClasses, dialogPt, INPUT_CLASSES, selectPt } from '../lib/controls'
import { describeMutationError } from '../lib/errors'

const props = defineProps<{
  visible: boolean
  /** Org-admin invites offer both roles; the platform's admin-continuity
   * invite (D-026 §4) offers only 'admin' — passing a single option hides
   * the Select and shows a static "Admin" label instead. */
  roleOptions: { id: MembershipRole; label: string }[]
  isPending: boolean
  error?: unknown
}>()

const emit = defineEmits<{
  'update:visible': [value: boolean]
  submit: [payload: { email: string; role: MembershipRole }]
}>()

const email = ref('')
const role = ref<MembershipRole>(props.roleOptions[0]?.id ?? 'member')

watch(
  () => props.visible,
  (visible) => {
    if (!visible) return
    email.value = ''
    role.value = props.roleOptions[0]?.id ?? 'member'
  },
)

function onRoleChange(value: unknown) {
  if (typeof value !== 'string') return
  role.value = value as MembershipRole
}

function close() {
  if (props.isPending) return
  emit('update:visible', false)
}

function submit() {
  if (props.isPending) return
  emit('submit', { email: email.value.trim(), role: role.value })
}
</script>

<template>
  <Dialog
    :visible="visible"
    modal
    :closable="false"
    :close-on-escape="!isPending"
    :dismissable-mask="!isPending"
    :pt="dialogPt()"
    @update:visible="(value: boolean) => !value && close()"
  >
    <template #header>
      <h2 class="text-section font-semibold text-text">
        Invite
      </h2>
    </template>

    <form
      class="space-y-4"
      @submit.prevent="submit"
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
          autocomplete="off"
          required
          placeholder="name@example.com"
          :disabled="isPending"
          :class="INPUT_CLASSES"
        >
      </FormField>

      <FormField
        v-if="roleOptions.length > 1"
        label="Role"
        bare
      >
        <Select
          :model-value="role"
          :options="roleOptions"
          option-label="label"
          option-value="id"
          aria-label="Role"
          :disabled="isPending"
          :pt="selectPt()"
          class="w-full"
          @update:model-value="onRoleChange"
        />
      </FormField>
      <FormField
        v-else
        label="Role"
        bare
      >
        <p class="text-body text-text">
          {{ roleOptions[0]?.label ?? 'Admin' }}
        </p>
      </FormField>

      <p
        v-if="error"
        role="alert"
        class="text-small text-danger"
      >
        {{ describeMutationError(error, 'Could not send the invitation.') }}
      </p>
    </form>

    <template #footer>
      <button
        type="button"
        :class="buttonClasses('secondary')"
        :disabled="isPending"
        @click="close"
      >
        Cancel
      </button>
      <button
        type="button"
        :class="buttonClasses('primary')"
        :disabled="isPending || !email.trim()"
        @click="submit"
      >
        {{ isPending ? 'Sending…' : 'Send invite' }}
      </button>
    </template>
  </Dialog>
</template>
