import { beforeEach, describe, expect, it, vi } from 'vitest'
import { apiFetch } from './client'
import { cancelPeopleAdmission, confirmPeopleAdmission, fetchAdmissionField, fetchAdmissionItems, fetchAdmissionProvenanceContacts, fetchPeopleAdmissions, retryPeopleAdmission } from './peopleAdmissions'
vi.mock('./client',()=>({apiFetch:vi.fn()}))
const api=vi.mocked(apiFetch)
describe('people admissions wire contract',()=>{beforeEach(()=>api.mockReset())
 it('uses relative admission paths and opaque bounded cursors',async()=>{await fetchPeopleAdmissions('parent','opaque +/');await fetchAdmissionItems('run','plan','held_target','item +/');await fetchAdmissionField('run','item','first_name','field +/');await fetchAdmissionProvenanceContacts('person','contacts +/')
 expect(api.mock.calls.map(v=>v[0])).toEqual(['/migrations/fub/people-admissions?parent_import_id=parent&cursor=opaque+%2B%2F&limit=20','/migrations/fub/people-admissions/run/items?plan_id=plan&disposition=held_target&cursor=item+%2B%2F&limit=50','/migrations/fub/people-admissions/run/items/item/fields/first_name?cursor=field+%2B%2F&limit=16384','/people/person/admission-provenance/contacts?cursor=contacts+%2B%2F&limit=50'])})
 it('preserves decimal strings in frozen confirm and lifecycle bodies',async()=>{const body={request_id:'request',plan_id:'plan',plan_revision:'9007199254740993123',plan_digest:'a'.repeat(64),eligible_count:'9007199254740993124',acknowledged_coverage:true,acknowledged_mappings:true,acknowledged_distinct_contacts:true,acknowledged_review_hold:true};await confirmPeopleAdmission('run',body);await retryPeopleAdmission('run','retry','9007199254740993125');await cancelPeopleAdmission('run','cancel','9007199254740993126');expect(api.mock.calls.map(v=>JSON.parse(String(v[1]?.body)))).toEqual([body,{request_id:'retry',expected_lifecycle_revision:'9007199254740993125'},{request_id:'cancel',expected_lifecycle_revision:'9007199254740993126'}])})
})
