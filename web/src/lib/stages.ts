import type { Stage, StageRef } from '../api/types'

/**
 * D-019 makes stages per-Organization rows seeded from Follow Up Boss's
 * nine defaults (backend `DEFAULT_STAGE_NAMES`), so the API exposes no
 * stable id or semantic key for "Hot Prospect" — the seeded name is the
 * only handle a client has. An Organization that renames the stage simply
 * stops getting the flame; nothing else depends on this.
 */
const HOT_PROSPECT_STAGE_NAME = 'hot prospect'

export function isHotProspect(stage: Stage | StageRef): boolean {
  return stage.name.trim().toLowerCase() === HOT_PROSPECT_STAGE_NAME
}

/** D-045: visual only; custom stages never acquire an inferred meaning. */
export function stageTintClasses(stage: Stage | StageRef): string {
  switch (stage.name.trim().toLowerCase()) {
    case 'lead': return 'bg-stage-lead-bg text-stage-lead-text'
    case 'active client': return 'bg-stage-active-bg text-stage-active-text'
    case 'nurture': return 'bg-stage-nurture-bg text-stage-nurture-text'
    case 'hot prospect': return 'bg-stage-hot-bg text-stage-hot-text'
    default: return 'bg-tint-neutral-bg text-tint-neutral-text'
  }
}
