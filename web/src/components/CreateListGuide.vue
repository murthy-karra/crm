<script setup lang="ts">
// Slice 011b polish: "Create a list" on /lists sends the agent to /people,
// because a list is a saved People filter (SLICE_011b §"Create, Save, Save
// as and Duplicate"). Nothing on /people said so, which read as a misroute.
// This card appears only when the URL carries `guide=create-list`, explains
// the three steps, and offers a recorded walkthrough on demand. UI_STYLE §8:
// no ambient animation, so the animated image loads only inside the dialog
// and only after an explicit choice when reduced motion is preferred.
import Dialog from 'primevue/dialog'
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { buttonClasses, dialogPt } from '../lib/controls'

const CREATE_LIST_GUIDE_QUERY = 'guide'
const CREATE_LIST_GUIDE_VALUE = 'create-list'
// Served from web/public; bound dynamically so the build does not try to
// resolve the recorded assets at compile time.
const GIF_SRC = '/guides/create-list.gif'
const POSTER_SRC = '/guides/create-list-poster.png'

const route = useRoute()
const router = useRouter()

const active = computed(() => route.query[CREATE_LIST_GUIDE_QUERY] === CREATE_LIST_GUIDE_VALUE)
const showWalkthrough = ref(false)
const reducedMotion = ref(false)
const playAnyway = ref(false)
let motionQuery: MediaQueryList | null = null
const onMotionChange = (event: MediaQueryListEvent) => { reducedMotion.value = event.matches }

onMounted(() => {
  if (typeof window.matchMedia !== 'function') return
  motionQuery = window.matchMedia('(prefers-reduced-motion: reduce)')
  reducedMotion.value = motionQuery.matches
  motionQuery.addEventListener('change', onMotionChange)
})
onBeforeUnmount(() => motionQuery?.removeEventListener('change', onMotionChange))

const animated = computed(() => !reducedMotion.value || playAnyway.value)

function dismiss() {
  const query = { ...route.query }
  delete query[CREATE_LIST_GUIDE_QUERY]
  void router.replace({ query, hash: route.hash })
}

const STEPS = [
  'Narrow People with the filter chips, or leave the filter empty to include everyone.',
  'Choose Save as list, give it a name, and pick who can see it.',
  'Your new list opens. From there you can also use it as a Today source.',
]
</script>

<template>
  <section
    v-if="active"
    class="mb-5 rounded-xl border border-border bg-surface-0 p-4"
    aria-label="Creating a list"
    data-testid="create-list-guide"
  >
    <div class="flex flex-wrap items-start justify-between gap-3">
      <div class="min-w-0">
        <h2 class="text-body font-semibold text-text">
          Creating a list
        </h2>
        <ol class="mt-1 list-decimal space-y-0.5 pl-5 text-small text-text-muted">
          <li
            v-for="step in STEPS"
            :key="step"
          >
            {{ step }}
          </li>
        </ol>
      </div>
      <div class="flex shrink-0 items-center gap-2">
        <button
          type="button"
          :class="buttonClasses('secondary')"
          @click="showWalkthrough = true"
        >
          Show me
        </button>
        <button
          type="button"
          :class="buttonClasses('secondary')"
          aria-label="Dismiss the creating-a-list guide"
          @click="dismiss"
        >
          Dismiss
        </button>
      </div>
    </div>

    <Dialog
      :visible="showWalkthrough"
      modal
      :closable="false"
      close-on-escape
      dismissable-mask
      :pt="dialogPt()"
      @update:visible="(value: boolean) => { if (!value) showWalkthrough = false }"
    >
      <template #header>
        <h2 class="text-section font-semibold text-text">
          How a list is created
        </h2>
      </template>

      <img
        v-if="animated"
        :src="GIF_SRC"
        alt="Recorded walkthrough: the Lists page, then People with a stage filter applied, then the Save as list dialog with a name entered, then the new list's page."
        class="w-full rounded-lg border border-border"
        width="1000"
        height="625"
      >
      <div v-else>
        <img
          :src="POSTER_SRC"
          alt="First frame of the recorded walkthrough: the Lists page with the Create a list button."
          class="w-full rounded-lg border border-border"
          width="1000"
          height="625"
        >
        <button
          type="button"
          :class="[buttonClasses('secondary'), 'mt-3']"
          @click="playAnyway = true"
        >
          Play the animation
        </button>
      </div>
      <ol class="mt-3 list-decimal space-y-0.5 pl-5 text-small text-text-muted">
        <li
          v-for="step in STEPS"
          :key="step"
        >
          {{ step }}
        </li>
      </ol>

      <template #footer>
        <button
          type="button"
          :class="buttonClasses('primary')"
          @click="showWalkthrough = false"
        >
          Close
        </button>
      </template>
    </Dialog>
  </section>
</template>
