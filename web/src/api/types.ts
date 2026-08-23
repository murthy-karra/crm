// Wire shapes for the HTTP contracts frozen in docs/specs/SLICE_002.md §5
// (and the Slice 001 session/organization endpoints they build on). This
// file is a consumer of that contract, not an owner of it — every shape
// here must match §5 exactly; do not add, rename, or infer fields.

// ---- Session / identity (Slice 001; §3/§4 declared changes in SLICE_004
// §5 "Declared changes to existing contracts" — organization is now
// nullable and carries `role`; `platform_admin` is new) ---------------------

export interface UserSummary {
  id: string
  email: string
  display_name: string
}

export interface OrganizationSummary {
  id: string
  name: string
}

export type MembershipRole = 'admin' | 'member'
export type MembershipStatus = 'active' | 'inactive'

export interface MeOrganization extends OrganizationSummary {
  role: MembershipRole
}

// SLICE_004 §5 item 2: `organization` is null for a platform-only session
// (no active Organization); `platform_admin` is additive. The three
// session shapes router.ts's guard handles: member (`organization != null`,
// role 'member'), admin (`organization != null`, role 'admin'), and
// platform-only (`organization: null`, `platform_admin: true`) — plus a
// user who is both an Organization admin/member and a platform admin.
export interface MeResponse {
  user: UserSummary
  organization: MeOrganization | null
  platform_admin: boolean
}

// SLICE_004 §5 "GET /api/organization/members gains role, status,
// joined_at, assigned_people_count (additive)".
export interface Member {
  user_id: string
  display_name: string
  email: string
  role: MembershipRole
  status: MembershipStatus
  joined_at: string
  assigned_people_count: number
}

export interface MembersResponse {
  members: Member[]
}

// ---- Membership mutations (§5 PUT .../members/{id}/role, .../status) -----

export interface ChangeMemberRoleRequest {
  role: MembershipRole
}

export interface SetMemberStatusRequest {
  status: MembershipStatus
}

export interface MemberMutationResponse {
  member: Member
}

// ---- Invitations (§5 GET/POST /api/organization/invitations, DELETE
// /api/organization/invitations/{id}; §2 "Invitation state is derived") ----

// Derived state, never stored server-side (§2): accepted if accepted_at,
// else revoked if revoked_at, else expired if expires_at <= now, else
// pending.
export type InvitationStatus = 'pending' | 'expired' | 'accepted' | 'revoked'

export interface InvitedBy {
  id: string
  display_name: string
}

export interface Invitation {
  id: string
  email: string
  role: MembershipRole
  status: InvitationStatus
  expires_at: string
  created_at: string
  invited_by: InvitedBy
}

export interface InvitationsResponse {
  invitations: Invitation[]
}

export interface IssueInvitationRequest {
  email: string
  role: MembershipRole
}

// The only response that ever contains the raw token, embedded in
// `accept_path` (§5); the client absolutizes it with its own origin.
export interface IssueInvitationResponse {
  invitation: Invitation
  accept_path: string
}

// ---- Platform (§5 "Platform routes"; PlatformAuthContext, Organization id
// from the path, never the session) ----------------------------------------

export type OrganizationState = 'ok' | 'pending_first_admin' | 'needs_attention'

export interface PlatformOrganizationSummary {
  id: string
  name: string
  status: 'active'
  created_at: string
  member_count: number
  admin_count: number
  pending_admin_invitations: number
  state: OrganizationState
}

export interface PlatformOrganizationsResponse {
  organizations: PlatformOrganizationSummary[]
}

export interface CreateOrganizationRequest {
  name: string
}

export interface CreateOrganizationResponse {
  organization: PlatformOrganizationSummary
}

export interface PlatformOrganizationDetailResponse {
  organization: PlatformOrganizationSummary
  members: Member[]
  invitations: Invitation[]
}

// Platform's role/invitation routes only ever accept 'admin' (D-026 §4) —
// the route rejects 'member' before it reaches the domain (§4's
// ChangeMemberRole table) — but the request shape is otherwise identical
// to the org-admin one, so these are typed narrowly rather than reusing
// ChangeMemberRoleRequest/IssueInvitationRequest, which allow 'member'.
export interface PlatformChangeMemberRoleRequest {
  role: 'admin'
}

export interface PlatformIssueInvitationRequest {
  email: string
  role: 'admin'
}

// ---- Public invitation routes (§5 "Public routes"; no session, the token
// is the credential) --------------------------------------------------------

export interface InvitationPreviewRequest {
  token: string
}

// Deliberately excludes the `state` field the domain query
// (`invitation::preview`, §4) mentions internally — the HTTP contract (§5)
// distinguishes expired/used/invalid via status code (410/409/404), not a
// body field, and the frozen contract is §5, not §4's prose.
export interface InvitationPreviewResponse {
  organization_name: string
  email: string
  role: MembershipRole
  expires_at: string
}

export interface AcceptInvitationRequest {
  token: string
  display_name: string
  password: string
}

// "body identical to POST /api/session" (§5).
export type AcceptInvitationResponse = MeResponse

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

// SLICE_003 §5's declared additive change to the SLICE_002 §5 contract:
// `contact_attempted`, `kind_rank` 4, `detail: {"channel", "outcome"}`.
export interface ContactAttemptedDetail {
  channel: ContactChannel
  outcome: ContactOutcome
}

// SLICE_006 §2's declared additive change: `call_completed`, `kind_rank` 5,
// `detail: {call_id, outcome, talk_seconds, answered_at}`. `outcome` is
// `reached` for an answered call, otherwise the call's `failure_reason`
// (backend `settle.rs`; the `call_completed.outcome` CHECK constraint).
export type CallCompletedOutcome = 'reached' | CallFailureReason

export interface CallCompletedDetail {
  call_id: string
  outcome: CallCompletedOutcome
  talk_seconds: number | null
  answered_at: string | null
}

export type HistoryEntry =
  | (HistoryEntryBase & { kind: 'inquiry_received'; detail: InquiryReceivedDetail })
  | (HistoryEntryBase & { kind: 'routing_decision'; detail: RoutingDecisionDetail })
  | (HistoryEntryBase & { kind: 'assignment_changed'; detail: AssignmentChangedDetail })
  | (HistoryEntryBase & { kind: 'stage_changed'; detail: StageChangedDetail })
  | (HistoryEntryBase & { kind: 'contact_attempted'; detail: ContactAttemptedDetail })
  | (HistoryEntryBase & { kind: 'call_completed'; detail: CallCompletedDetail })

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

// ---- Today (SLICE_003 §5 GET /api/today; §3 reasons/priority/action) -----

export type TodayPriority = 'high' | 'normal'
export type RecommendedAction = 'call' | 'email'

// Discriminated on `code`, in the fixed wire order (§3) — never re-sorted
// client-side, same discipline as `history` above.
export type TodayReason =
  | { code: 'new_inquiry'; source: string; received_at: string }
  | { code: 'no_contact_attempt'; since: string }
  | { code: 'repeat_inquiry'; inquiry_count: number }

// `latest_inquiry` on a TodayItem — exactly `{id, source, received_at}` (§5),
// narrower than `PersonInquiry` (which also carries `source_external_id` and
// `message`).
export interface TodayInquiryRef {
  id: string
  source: string
  received_at: string
}

export interface ContactAttemptRef {
  id: string
  channel: ContactChannel
  outcome: ContactOutcome
  occurred_at: string
}

export interface TodayItem {
  person: PersonSummary
  priority: TodayPriority
  recommended_action: RecommendedAction
  reasons: TodayReason[]
  waiting_since: string
  latest_inquiry: TodayInquiryRef
  last_contact_attempt: ContactAttemptRef | null
}

export interface TodayResponse {
  generated_at: string
  items: TodayItem[]
  truncated: boolean
}

// ---- Contact attempts (SLICE_003 §5 POST /api/people/{id}/contact-attempts)

export type ContactChannel = 'call' | 'text' | 'email' | 'other'
export type ContactOutcome = 'reached' | 'no_answer' | 'left_message' | 'sent'

export interface LogContactRequest {
  channel: ContactChannel
  outcome: ContactOutcome
}

export interface LogContactResponse {
  person: PersonSummary
  contact_attempt: ContactAttemptRef
}

// ---- Realtime token (SLICE_003 §5 POST /api/realtime/token; §6) ----------

export interface RealtimeTokenResponse {
  token: string
}

// --- Slice 005: Operator (docs/specs/SLICE_005.md §5) -----------------------

export type OperatorRoute = 'today' | 'person' | 'people' | 'other'

export interface OperatorScreenContext {
  route: OperatorRoute
  person_id?: string
}

export type OperatorHistoryRole = 'user' | 'assistant'

export interface OperatorHistoryMessage {
  role: OperatorHistoryRole
  content: string
}

export interface OperatorTurnRequest {
  /** 1–2000 chars after trim. */
  message: string
  /** ≤ 6 items, each ≤ 2000 chars, ≤ 6000 total; oldest dropped first. */
  history: OperatorHistoryMessage[]
  context: OperatorScreenContext
}

/** `WirePersonCard`: plain strings — the only source of cards in the drawer. */
export interface OperatorPersonCard {
  id: string
  display_name: string
  stage_name: string
  assigned_user_display_name: string | null
  primary_email: string | null
  primary_phone: string | null
  inquiry_count: number
  last_inquiry_at: string | null
}

export type OperatorToolOutcome = 'ok' | 'not_found' | 'invalid_arguments' | 'error'

export interface OperatorToolCall {
  name: string
  outcome: OperatorToolOutcome
  duration_ms: number
}

/** The 200 outcomes; every other `TurnOutcome` is a 503 `operator_unavailable`. */
export type OperatorTurnOutcome = 'completed' | 'tool_budget_exhausted' | 'malformed_tool_call'

export interface OperatorTurnResponse {
  turn_id: string
  /** Plain text. Rendered by interpolation only — never as HTML or markdown. */
  reply: string
  references: { people: OperatorPersonCard[] }
  tool_calls: OperatorToolCall[]
  outcome: OperatorTurnOutcome
}

// --- Slice 006: Calling (docs/specs/SLICE_006.md §5; backend
// `domain/telephony/mod.rs` enums, `queries.rs` `CallView`) -----------------

export type CallStatus = 'placing' | 'ringing' | 'answered' | 'ended' | 'failed'

/** `call.failure_reason` (set exactly when `status = 'failed'`). */
export type CallFailureReason =
  | 'no_answer'
  | 'busy'
  | 'declined'
  | 'cancelled'
  | 'ring_timeout'
  | 'agent_not_joined'
  | 'provider_error'
  | 'expired'

/** `call.end_reason` (set exactly when `status = 'ended'`). */
export type CallEndReason = 'agent_hangup' | 'agent_disconnected' | 'remote_hangup' | 'max_duration' | 'reconciled'

/** `CallView` — PII-free by construction: no number, no token, no room. */
export interface CallView {
  id: string
  person_id: string
  contact_method_id: string
  caller: ActorRef
  status: CallStatus
  failure_reason: CallFailureReason | null
  end_reason: CallEndReason | null
  placed_at: string
  ringing_at: string | null
  answered_at: string | null
  ended_at: string | null
  talk_seconds: number | null
}

/** The one-room LiveKit join grant minted by `POST /api/people/{id}/calls`.
 * Held in component state only for the duration of `room.connect`; never
 * cached, logged, or persisted. */
export interface JoinGrant {
  url: string
  token: string
  room: string
}

/** `deny_unknown_fields` server-side: exactly this one field. The client
 * never sends a phone number — only the contact method's id. */
export interface StartCallRequest {
  contact_method_id: string
}

export interface StartCallResponse {
  call: CallView
  join: JoinGrant
}

/** `POST …/dial` (202), `POST …/hangup` (200), `GET /api/calls/{id}` (200). */
export interface CallResponse {
  call: CallView
}
