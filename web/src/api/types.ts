// Wire shapes for the HTTP contracts frozen in docs/specs/SLICE_002.md §5
// (and the Slice 001 session/organization endpoints they build on). This
// file is a consumer of that contract, not an owner of it — every shape
// here must match §5 exactly; do not add, rename, or infer fields.

// ---- Session / identity (Slice 001, unchanged by Slice 002) --------------

export interface UserSummary {
  id: string
  email: string
  display_name: string
}

export interface OrganizationSummary {
  id: string
  name: string
}

export interface MeResponse {
  user: UserSummary
  organization: OrganizationSummary
}

export interface Member {
  user_id: string
  display_name: string
  email: string
  joined_at: string
}

export interface MembersResponse {
  members: Member[]
}

// ---- Stages (§5 GET /api/stages) -----------------------------------------

export interface Stage {
  id: string
  name: string
  position: number
}

export interface StagesResponse {
  stages: Stage[]
}

// ---- People (§5 GET /api/people, GET /api/people/{id}) -------------------

export interface StageRef {
  id: string
  name: string
}

export interface ActorRef {
  id: string
  display_name: string
}

export interface PersonSummary {
  id: string
  first_name: string | null
  last_name: string | null
  display_name: string
  stage: StageRef
  assigned_user: ActorRef | null
  primary_email: string | null
  primary_phone: string | null
  inquiry_count: number
  last_inquiry_at: string | null
  created_at: string
}

export interface PeopleResponse {
  people: PersonSummary[]
  truncated: boolean
}

export interface ContactMethod {
  id: string
  kind: 'email' | 'phone'
  value: string
}

export interface PersonInquiry {
  id: string
  source: string
  source_external_id: string | null
  message: string | null
  received_at: string
}

// History `detail` shapes are per-kind (§5 "History entries"). Modeled as a
// discriminated union on `kind` so a template can narrow `detail` from the
// sibling `kind` field without a cast.
export interface InquiryReceivedDetail {
  inquiry_id: string
  source: string
  person_created: boolean
  matched_by: 'email' | 'phone' | null
}

export type RoutingStrategy = 'explicit' | 'actor_default' | 'kept_existing'

export interface RoutingDecisionDetail {
  inquiry_id: string
  strategy: RoutingStrategy
  assignee: ActorRef | null
}

export type ChangeReason = 'intake' | 'manual'

export interface AssignmentChangedDetail {
  from: ActorRef | null
  to: ActorRef | null
  reason: ChangeReason
}

export interface StageChangedDetail {
  from_stage: StageRef | null
  to_stage: StageRef
  reason: ChangeReason
}

interface HistoryEntryBase {
  id: string
  occurred_at: string
  recorded_at: string
  actor: ActorRef | null
  origin: string
  correlation_id: string
}

export type HistoryEntry =
  | (HistoryEntryBase & { kind: 'inquiry_received'; detail: InquiryReceivedDetail })
  | (HistoryEntryBase & { kind: 'routing_decision'; detail: RoutingDecisionDetail })
  | (HistoryEntryBase & { kind: 'assignment_changed'; detail: AssignmentChangedDetail })
  | (HistoryEntryBase & { kind: 'stage_changed'; detail: StageChangedDetail })

export interface PersonDetailResponse {
  person: PersonSummary
  contact_methods: ContactMethod[]
  inquiries: PersonInquiry[]
  // Ordered by the server (occurred_at, recorded_at, kind_rank, id) — never
  // re-sort this client-side (§5).
  history: HistoryEntry[]
}

// ---- Mutations: assignment / stage (§5 POST .../assignment, .../stage) ---

export interface AssignmentRequest {
  assigned_user_id: string | null
}

export interface StageRequest {
  stage_id: string
}

export interface MutatePersonResponse {
  person: PersonSummary
  changed: boolean
}

// ---- Intake (§5 POST /api/inquiries, GET /api/intake/unresolved) ---------

export interface ReceiveInquiryPayload {
  first_name?: string
  last_name?: string
  email?: string
  phone?: string
  message?: string
  external_id?: string
  // Optional client-generated idempotency helper (§3 "Idempotency scope"):
  // stable for one form instance so a retry of the same submission dedupes,
  // while a fresh visit to the form is a genuinely new Inquiry.
  submission_id?: string
}

export interface ReceiveInquiryRequest {
  source: string
  payload: ReceiveInquiryPayload
  assign_to_user_id?: string
}

export interface ReceiveInquiryResolved {
  status: 'resolved'
  inquiry_id: string
  person_id: string
  person_created: boolean
  routing_strategy: RoutingStrategy
  assigned_user_id: string | null
  duplicate: boolean
}

export type UnresolvedReason = 'invalid_json' | 'not_an_object' | 'no_contact_method'

export interface ReceiveInquiryUnresolved {
  status: 'unresolved'
  raw_payload_id: string
  reason: UnresolvedReason
  duplicate: boolean
}

export type ReceiveInquiryResponse = ReceiveInquiryResolved | ReceiveInquiryUnresolved

// A raw_payload row visible in the queue always has `resolution` other than
// 'resolved': either 'unresolved' (parser/identify rejected it, reason set)
// or 'pending' (stored, Phase B not yet completed — reason null) (§3
// "Unresolved").
export type UnresolvedResolution = 'pending' | 'unresolved'

export interface UnresolvedItem {
  id: string
  source: string
  received_at: string
  resolution: UnresolvedResolution
  reason: UnresolvedReason | null
  byte_len: number
}

export interface UnresolvedResponse {
  items: UnresolvedItem[]
  truncated: boolean
}
