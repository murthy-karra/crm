<script setup lang="ts">
// Slice 011b polish: "Create a list" on /lists sends the agent to /people,
// because a list is a saved People filter (SLICE_011b §"Create, Save, Save
// as and Duplicate"). Nothing on /people said so, which read as a misroute.
// This card appears only when the URL carries `guide=create-list`, explains
// the three steps, and offers a recorded walkthrough on demand. UI_STYLE §8:
// no ambient animation, so the video lives inside a wide dialog, plays with
// native controls, and autoplays (muted) only when motion is not reduced.
import Dialog from 'primevue/dialog'
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { buttonClasses, dialogPt } from '../lib/controls'

const CREATE_LIST_GUIDE_QUERY = 'guide'
const CREATE_LIST_GUIDE_VALUE = 'create-list'
// Served from web/public; bound dynamically so the build does not try to
// resolve the recorded assets at compile time.
const VIDEO_SRC = '/guides/create-list.mp4'
const POSTER_SRC = '/guides/create-list-poster.png'

const route = useRoute()
const router = useRouter()

const active = computed(() => route.query[CREATE_LIST_GUIDE_QUERY] === CREATE_LIST_GUIDE_VALUE)
const showWalkthrough = ref(false)
const reducedMotion = ref(false)
let motionQuery: MediaQueryList | null = null
const onMotionChange = (event: MediaQueryListEvent) => { reducedMotion.value = event.matches }

onMounted(() => {
  if (typeof window.matchMedia !== 'function') return
  motionQuery = window.matchMedia('(prefers-reduced-motion: reduce)')
  reducedMotion.value = motionQuery.matches
  motionQuery.addEventListener('change', onMotionChange)
})
onBeforeUnmount(() => motionQuery?.removeEventListener('change', onMotionChange))

const autoplay = computed(() => !reducedMotion.value)
// The shared dialog shell is sized for confirmations; the walkthrough needs
// the width its frames were recorded at to stay legible.
const walkthroughPt = computed(() => ({
  ...dialogPt(),
  // Width follows the viewport height too (video aspect 1.6, about 14rem of
  // header, steps and footer), so the dialog never exceeds the viewport.
  root: { class: 'glass-panel w-full max-w-[min(96vw,1240px,calc((100vh-14rem)*1.6))] max-h-[calc(100vh-2rem)] overflow-y-auto' },
}))

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
      :pt="walkthroughPt"
      @update:visible="(value: boolean) => { if (!value) showWalkthrough = false }"
    >
      <template #header>
        <h2 class="text-section font-semibold text-text">
          How a list is created
        </h2>
      </template>

      <video
        :src="VIDEO_SRC"
        :poster="POSTER_SRC"
        :autoplay="autoplay"
        controls
        muted
        loop
        playsinline
        preload="metadata"
        class="w-full rounded-lg border border-border bg-surface-0"
        width="1120"
        height="700"
        aria-label="Recorded walkthrough: the Lists page, then People with a stage filter applied, then the Save as list dialog with a name entered, then the new list's page."
      >
        Your browser cannot play this video. The steps are listed below.
      </video>
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
