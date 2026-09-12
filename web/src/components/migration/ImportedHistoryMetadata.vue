<script setup lang="ts">
import type { ImportedMetadata } from '../../api/historyImports'
defineProps<{ metadata: ImportedMetadata; compact?: boolean }>()
</script>
<template>
  <dl class="grid min-w-0 gap-3 text-small sm:grid-cols-2">
    <div>
      <dt class="text-text-muted">
        FUB record ID
      </dt><dd class="break-all">
        {{ metadata.source_id ?? 'Unknown' }}
      </dd>
    </div>
    <div>
      <dt class="text-text-muted">
        FUB Person ID
      </dt><dd class="break-all">
        {{ metadata.source_person_id ?? 'Unknown' }}
      </dd>
    </div>
    <div>
      <dt class="text-text-muted">
        FUB record created
      </dt><dd class="break-all">
        {{ metadata.source_created ?? 'Date unknown' }}
      </dd>
    </div>
    <div v-if="metadata.source_event_type">
      <dt class="text-text-muted">
        FUB event classification
      </dt><dd class="break-words">
        {{ metadata.source_event_type }}
      </dd>
    </div>
    <div v-if="metadata.source_duration !== null">
      <dt class="text-text-muted">
        FUB duration
      </dt><dd class="break-words">
        {{ metadata.source_duration }} {{ metadata.source_duration_unit ?? '(unit unknown)' }}
      </dd>
    </div>
    <div v-if="metadata.source_is_incoming !== null">
      <dt class="text-text-muted">
        FUB direction claim
      </dt><dd>{{ metadata.source_is_incoming ? 'Incoming' : 'Outgoing' }}</dd>
    </div>
    <div v-if="metadata.source_outcome">
      <dt class="text-text-muted">
        FUB outcome label
      </dt><dd class="break-words">
        {{ metadata.source_outcome }}
      </dd>
    </div>
    <div v-if="metadata.source_status">
      <dt class="text-text-muted">
        FUB status claim
      </dt><dd class="break-words">
        {{ metadata.source_status }}
      </dd>
    </div>
    <template v-if="!compact">
      <div>
        <dt class="text-text-muted">
          FUB updated
        </dt><dd class="break-all">
          {{ metadata.source_updated ?? 'Unknown' }}
        </dd>
      </div>
      <div>
        <dt class="text-text-muted">
          FUB sent
        </dt><dd class="break-all">
          {{ metadata.source_sent ?? 'Unknown' }}
        </dd>
      </div>
      <div>
        <dt class="text-text-muted">
          Source access user ID
        </dt><dd class="break-all">
          {{ metadata.source_access_user_id ?? 'Unresolved' }}
        </dd>
      </div>
      <div>
        <dt class="text-text-muted">
          Attributed source user ID
        </dt><dd class="break-all">
          {{ metadata.source_attributed_user_id ?? 'Unresolved' }}
        </dd>
      </div>
      <div>
        <dt class="text-text-muted">
          Source creator user ID
        </dt><dd class="break-all">
          {{ metadata.source_creator_user_id ?? 'Unresolved' }}
        </dd>
      </div>
      <div>
        <dt class="text-text-muted">
          Source editor user ID
        </dt><dd class="break-all">
          {{ metadata.source_editor_user_id ?? 'Unresolved' }}
        </dd>
      </div>
      <div v-if="metadata.source_note_id">
        <dt class="text-text-muted">
          Source note reference
        </dt><dd class="break-all">
          {{ metadata.source_note_id }}
        </dd>
      </div>
      <div>
        <dt class="text-text-muted">
          Content evidence
        </dt><dd>{{ metadata.content_availability === 'returned_in_raw' ? 'A content field was returned in retained data; readability and completeness are unverified.' : 'Content not returned or unknown.' }}</dd>
      </div>
    </template>
  </dl>
  <p
    v-if="metadata.preview_truncated"
    class="mt-2 text-small text-text-muted"
  >
    Source labels are abbreviated. Original evidence remains retained.
  </p>
</template>
