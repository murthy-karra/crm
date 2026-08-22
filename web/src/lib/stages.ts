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
