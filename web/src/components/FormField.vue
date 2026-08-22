<script setup lang="ts">
// UI_STYLE.md §7: "A form is a vertical stack of cards ... each card one
// field or one logical group: heading (15/500), optional description
// (text-muted, 15px), then the control." The card heading doubles as the
// input's <label> (§5: "Label above the input (15px/500)") — both specs
// land on the same size/weight, so one element satisfies both.
//
// `bare` skips the Card wrapper for reuse inside an already-existing card
// (login's single card, the person-detail identity header) — UI_STYLE.md
// §2 forbids nesting a card in a card.
import { useId } from 'vue'
import Card from './Card.vue'
import { DESCRIPTION_CLASSES, HELP_TEXT_CLASSES, LABEL_CLASSES } from '../lib/controls'

withDefaults(
  defineProps<{
    label: string
    description?: string
    helpText?: string
    bare?: boolean
  }>(),
  { bare: false, description: undefined, helpText: undefined },
)

const id = useId()
</script>

<template>
  <component :is="bare ? 'div' : Card">
    <label
      :for="id"
      :class="LABEL_CLASSES"
    >{{ label }}</label>
    <p
      v-if="description"
      :class="DESCRIPTION_CLASSES"
    >
      {{ description }}
    </p>
    <slot :id="id" />
    <p
      v-if="helpText"
      :class="HELP_TEXT_CLASSES"
    >
      {{ helpText }}
    </p>
  </component>
</template>
