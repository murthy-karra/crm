<script setup lang="ts">
import { computed, ref } from 'vue'
import { RouterLink } from 'vue-router'
import { ArrowUpRight, X } from 'lucide-vue-next'
import { reviewHistoryDetail, reviewHistoryTitle, useActivityReviewCore } from '../api/activityReview'
import { buttonClasses } from '../lib/controls'
import { describeApiError } from '../lib/errors'
import { formatAbsoluteTime, initials } from '../lib/format'
import StageLabel from './StageLabel.vue'
const props = defineProps<{ personId: string }>()
const emit = defineEmits<{ close: [] }>()
const { access, reading, data } = useActivityReviewCore(() => props.personId)
const root = ref<HTMLElement | null>(null)
const tab = ref<'activity' | 'contact'>('activity')
const profilePath = computed(() => `/people/${encodeURIComponent(props.personId)}`)
defineExpose({ focus: () => root.value?.focus() })
</script>
<template>
  <section
    ref="root"
    role="dialog"
    aria-label="Person preview"
    tabindex="-1"
    class="person-preview glass-panel min-w-0 p-5 focus:outline-none"
    data-testid="person-preview"
  >
    <div class="mb-4 flex justify-end gap-1">
      <RouterLink
        :to="profilePath"
        :class="buttonClasses('ghost')"
        aria-label="Open full profile"
      >
        <ArrowUpRight
          class="h-4 w-4"
          aria-hidden="true"
        />
      </RouterLink>
      <button
        type="button"
        :class="buttonClasses('ghost')"
        aria-label="Close person preview"
        @click="emit('close')"
      >
        <X
          class="h-4 w-4"
          aria-hidden="true"
        />
      </button>
    </div>
    <p
      v-if="!access.enabled.value"
      role="alert"
      class="text-small text-text-muted"
    >
      Person review requires a current administrator session.
    </p>
    <div
      v-else-if="reading.isError.value"
      role="alert"
      class="space-y-3 text-small text-danger"
    >
      <p>{{ describeApiError(reading.error.value, 'Could not load this Person for review.') }}</p><button
        type="button"
        :class="buttonClasses()"
        @click="reading.refetch()"
      >
        Retry Person review
      </button>
    </div>
    <p
      v-else-if="!data"
      role="status"
      class="text-small text-text-muted"
    >
      Loading Person…
    </p>
    <template v-else>
      <div
        class="avatar-surface mb-4 h-12 w-12"
        aria-hidden="true"
      >
        {{ initials(data.person.display_name) }}
      </div>
      <h2 class="mb-3 break-words text-title font-medium">
        {{ data.person.display_name }}
      </h2>
      <StageLabel
        :stage="data.person.stage"
        badge
      />
      <p class="mt-3 text-small text-text-muted">
        Administrator review
      </p>
      <div
        class="mt-2 flex flex-wrap gap-1.5"
        aria-label="Tags"
        data-testid="person-preview-tags"
      >
        <span
          v-for="tag in data.tags"
          :key="tag.id"
          class="rounded-lg border border-border px-2 py-0.5 text-small text-text-muted"
        >{{ tag.name }}</span>
      </div>
      <dl class="mt-4 space-y-3 text-small">
        <div>
          <dt class="text-text-muted">
            Owner
          </dt><dd class="break-words">
            {{ data.person.assigned_user?.display_name ?? 'Unassigned' }}
          </dd>
        </div><div>
          <dt class="text-text-muted">
            Latest source
          </dt><dd class="break-words">
            {{ data.inquiries[0]?.source ?? '—' }}
          </dd>
        </div><div>
          <dt class="text-text-muted">
            Added
          </dt><dd>{{ formatAbsoluteTime(data.person.created_at) }}</dd>
        </div>
      </dl>
      <div
        class="people-tabs mt-7"
        aria-label="Person information"
      >
        <button
          type="button"
          class="people-tab"
          :aria-pressed="tab === 'activity'"
          @click="tab = 'activity'"
        >
          Activity
        </button><button
          type="button"
          class="people-tab"
          :aria-pressed="tab === 'contact'"
          @click="tab = 'contact'"
        >
          Contact
        </button>
      </div>
      <div
        v-if="tab === 'activity'"
        class="space-y-3 text-small"
      >
        <p class="text-text-muted">
          {{ data.activity.notes_count }} notes · {{ data.activity.open_tasks_count }} open tasks · {{ data.activity.completed_tasks_count }} completed tasks
        </p>
        <ol class="space-y-3">
          <li
            v-for="entry in data.core_history.slice(-3)"
            :key="entry.id"
          >
            <p>{{ reviewHistoryTitle(entry.kind) }}</p><p class="break-words text-text-muted">
              {{ reviewHistoryDetail(entry) }}
            </p><p class="text-text-muted">
              {{ formatAbsoluteTime(entry.occurred_at) }}
            </p>
          </li>
        </ol>
        <RouterLink
          :to="profilePath"
          class="inline-flex min-h-10 items-center text-small text-text-muted hover:text-text"
        >
          Review full profile and activity <ArrowUpRight
            class="ml-1 h-3.5 w-3.5"
            aria-hidden="true"
          />
        </RouterLink>
      </div>
      <ul
        v-else
        class="space-y-3 text-small"
      >
        <li
          v-for="method in data.contact_methods"
          :key="method.id"
          class="break-all text-text-muted"
        >
          {{ method.kind }} · {{ method.value }}
        </li><li v-if="!data.contact_methods.length">
          No contact methods.
        </li>
      </ul>
    </template>
  </section>
</template>
