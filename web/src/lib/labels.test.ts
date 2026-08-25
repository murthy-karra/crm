// SLICE_006c §2 vocabulary and §10 labels: the manual dialog and the
// post-call prompt derive their options from these maps, so the maps are
// pinned here (the dialog test proves it reads them).
import { describe, expect, it } from 'vitest'
import {
  CALL_OUTCOME_CORRECTION_LABEL,
  CONTACT_OUTCOME_LABEL,
  UNRESOLVED_REASON_LABEL,
  correctedOutcomeLabel,
} from './labels'

describe('contact outcome labels (SLICE_006c §2)', () => {
  it('relabels left_message and adds busy / wrong_number', () => {
    expect(CONTACT_OUTCOME_LABEL.left_message).toBe('Voicemail / left message')
    expect(CONTACT_OUTCOME_LABEL.busy).toBe('Busy')
    expect(CONTACT_OUTCOME_LABEL.wrong_number).toBe('Wrong number')
    expect(Object.keys(CONTACT_OUTCOME_LABEL)).toEqual(['reached', 'no_answer', 'left_message', 'sent', 'busy', 'wrong_number'])
  })

  it('offers exactly the five correction choices in prompt order', () => {
    expect(Object.entries(CALL_OUTCOME_CORRECTION_LABEL)).toEqual([
      ['reached', 'Talked to them'],
      ['left_message', 'Voicemail'],
      ['no_answer', 'No answer'],
      ['busy', 'Busy'],
      ['wrong_number', 'Wrong number'],
    ])
  })

  it('lowercases the prompt label for "Outcome saved — voicemail"', () => {
    expect(correctedOutcomeLabel('left_message')).toBe('voicemail')
    expect(correctedOutcomeLabel('reached')).toBe('talked to them')
    expect(correctedOutcomeLabel('sent')).toBe('sent')
  })
})

// SLICE_007b §7: the Unresolved table's Reason cell renders `undefined`
// without this entry.
describe('unresolved reason labels (SLICE_007b §7)', () => {
  it('labels an unparsed inbound email', () => {
    expect(UNRESOLVED_REASON_LABEL.email_unparsed).toBe('Unparsed email')
  })

  // SLICE_007d §6: valid MIME matching no pinned format.
  it('labels an unrecognized email format', () => {
    expect(UNRESOLVED_REASON_LABEL.email_unrecognized_format).toBe(
      'Unrecognized email format',
    )
  })

  // SLICE_007f §6: the two terminal extraction outcomes.
  it('labels the extraction outcomes', () => {
    expect(UNRESOLVED_REASON_LABEL.not_a_lead).toBe('Not a lead')
    expect(UNRESOLVED_REASON_LABEL.email_extraction_failed).toBe('Extraction failed')
  })
})
