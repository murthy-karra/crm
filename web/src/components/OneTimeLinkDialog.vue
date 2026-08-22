<script setup lang="ts">
// SLICE_004 §1 step 3 / §10: "a one-time 'copy link' panel... the link is
// never shown again — re-issue to get a new one" / "closing it is final".
// `acceptPath` is the bare path SLICE_004 §5 returns
// (`POST /api/organization/invitations` etc.) — this component is the one
// place that absolutizes it with `window.location.origin` (task brief: "the
// client absolutizes with window.location.origin"). Shared by MembersView's
// Invite-and-Re-issue flows and PlatformOrganizationView's Invite-admin flow.
import { computed, ref, watch } from 'vue'
import Dialog from 'primevue/dialog'
import { Check, Copy } from 'lucide-vue-next'
import { buttonClasses, dialogPt, INPUT_CLASSES } from '../lib/controls'

const props = defineProps<{
  visible: boolean
  email: string
  acceptPath: string
}>()

const emit = defineEmits<{ 'update:visible': [value: boolean] }>()

const copied = ref(false)

watch(
  () => props.visible,
  (visible) => {
    if (visible) copied.value = false
  },
)

const link = computed(() => `${window.location.origin}${props.acceptPath}`)

async function copyLink() {
  try {
    await navigator.clipboard.writeText(link.value)
    copied.value = true
  } catch {
    copied.value = false
  }
}

function done() {
  emit('update:visible', false)
}
</script>

<template>
  <Dialog
    :visible="visible"
    modal
    :closable="false"
    :pt="dialogPt()"
    @update:visible="(value: boolean) => !value && done()"
  >
    <template #header>
      <h2 class="text-section font-semibold text-text">
        Invitation sent
      </h2>
    </template>

    <p class="text-body text-text-muted">
      Share this link with <span class="text-text">{{ email }}</span> — it will not be shown again. Closing this
      dialog is final; re-issue the invitation to get a new link.
    </p>

    <div class="mt-4 flex items-center gap-2">
      <input
        type="text"
        readonly
        :value="link"
        :class="INPUT_CLASSES"
        class="font-mono text-small"
        @focus="($event.target as HTMLInputElement).select()"
      >
      <button
        type="button"
        :class="buttonClasses('secondary')"
        @click="copyLink"
      >
        <Check
          v-if="copied"
          class="h-4 w-4"
          stroke-width="1.5"
        />
        <Copy
          v-else
          class="h-4 w-4"
          stroke-width="1.5"
        />
        {{ copied ? 'Copied' : 'Copy' }}
      </button>
    </div>

    <template #footer>
      <button
        type="button"
        :class="buttonClasses('primary')"
        @click="done"
      >
        Done
      </button>
    </template>
  </Dialog>
</template>
