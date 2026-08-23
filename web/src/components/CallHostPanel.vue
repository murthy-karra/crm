<script setup lang="ts">
// The one docked CallPanel, bound to the app-level call host
// (docs/specs/SLICE_006b.md §6). Mounted once in AppShell; the bindings
// are exactly what PersonDetailView passed before the lift.
import CallPanel from './CallPanel.vue'
import { useCallHost } from '../telephony/callHost'

const host = useCallHost()
const { call } = host
</script>

<template>
  <CallPanel
    :phase="call.phase.value"
    :person-name="host.calleeName.value"
    :elapsed-seconds="call.elapsedSeconds.value"
    :muted="call.muted.value"
    :error="call.error.value"
    :call="call.call.value"
    :outcome-prompt="host.outcomePromptOpen.value"
    :outcome-saving="host.outcomeSaving.value"
    :outcome-saved="host.outcomeSaved.value"
    :outcome-error="host.outcomeError.value"
    @hangup="call.hangup()"
    @toggle-mute="call.toggleMute()"
    @hangup-previous="call.hangupPrevious()"
    @dismiss="host.dismissCall()"
    @save-outcome="host.saveOutcome"
  />
</template>
