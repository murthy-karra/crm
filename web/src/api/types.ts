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
  /** SLICE_007a §5: the rendered intake address (top-level, additive). */
  intake_address: string
}

// --- Slice 007a: Organization intake address (docs/specs/SLICE_007a.md §5) ---

export type IntakeAddressScheme = 'subdomain' | 'local_part'

export interface IntakeAddressResponse {
  address: string
  scheme: IntakeAddressScheme
}

// --- Slice 008: intake routing modes (docs/specs/SLICE_008.md §5, D-041) ---
// Supersedes SLICE_007c §5's single-key `IntakeSettingsResponse` shape
// (declared breaking change, SLICE_007c §5 pointer amendment).

export type IntakeRoutingMode = 'default_assignee' | 'round_robin' | 'unassigned'

export interface IntakeSettingsResponse {
  intake_routing_mode: IntakeRoutingMode
  intake_default_assignee_user_id: string | null
}

export type IntakeSettingsRequest = IntakeSettingsResponse

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

// --- Slice 011a: Filter vocabulary (docs/specs/SLICE_011a.md §4a) ---------
// The wire shape backend/crates/crm-app/src/domain/person/filter.rs
// (de)serializes. Every clause object and the top level are
// `deny_unknown_fields` server-side — this client only ever constructs
// well-formed values, so no client-side enforcement is needed here, but
// the shapes must match exactly.

export type Assignee = 'me' | 'unassigned' | { user_id: string }

export type AgeOp =
  | { op: 'within_days'; days: number }
  | { op: 'not_within_days'; days: number }
  | { op: 'never' }

// --- Slice 011d: derived boolean clause kinds (docs/specs/SLICE_011d.md §2) -
// Three more `{"kind": K, "value": bool}` clauses, one per system feed axis.
// Counted against the existing 20-clause cap server-side; no client-side cap
// enforcement here, matching the rest of this vocabulary (§4a note above).

export type FilterClauseKind =
  | 'stage'
  | 'assigned_to'
  | 'source'
  | 'created'
  | 'last_inquiry'
  | 'last_contact'
  | 'last_inbound'
  | 'has_replied'
  | 'has_phone'
  | 'has_email'
  | 'awaiting_response'
  | 'client_replied_unanswered'
  | 'awaiting_call_outcome'
  | 'tags'
  | 'not_tags'

export type FilterClause =
  | { kind: 'stage'; stage_ids: string[] }
  | { kind: 'assigned_to'; assignees: Assignee[] }
  | { kind: 'source'; sources: string[] }
  | { kind: 'created'; age: AgeOp }
  | { kind: 'last_inquiry'; age: AgeOp }
  | { kind: 'last_contact'; age: AgeOp }
  | { kind: 'last_inbound'; age: AgeOp }
  | { kind: 'has_replied'; value: boolean }
  | { kind: 'has_phone'; value: boolean }
  | { kind: 'has_email'; value: boolean }
  | { kind: 'awaiting_response'; value: boolean }
  | { kind: 'client_replied_unanswered'; value: boolean }
  | { kind: 'awaiting_call_outcome'; value: boolean }
  // --- Slice 011e e2 (docs/specs/SLICE_011e.md §4a): tags (any-of) /
  // not_tags (none-of), one clause per kind, same 20-clause cap. -----------
  | { kind: 'tags'; tag_ids: string[] }
  | { kind: 'not_tags'; tag_ids: string[] }

export interface FilterDefinition {
  version: 1
  clauses: FilterClause[]
}

// --- Slice 011b-sort: People sort vocabulary (docs/specs/SLICE_011b_SORT.md
// §3, §6) --------------------------------------------------------------
// The eight lowercase dotted tokens `backend/crates/crm-app/src/domain/person/sort.rs`
// (de)serializes through `TryFrom<String>`. `null`/absent means the default
// order (`created.desc`), which is itself also a legal, byte-identical token.

export type PersonSortToken =
  | 'created.asc' | 'created.desc'
  | 'name.asc' | 'name.desc'
  | 'stage.asc' | 'stage.desc'
  | 'assignee.asc' | 'assignee.desc'

// --- Slice 011b: saved People definitions ---------------------------------
// Saved lists retain the existing v1 filter vocabulary verbatim. The server
// derives visibility and capabilities; no owner or Organization identifier is
// ever accepted or returned by this surface.

export type SavedListScope = 'personal' | 'shared'

export interface SavedListMetadata {
  id: string
  name: string
  scope: SavedListScope
  revision: number
  created_at: string
  updated_at: string
  can_edit: boolean
  can_delete: boolean
}

export type SavedListFilterError =
  | 'unsupported_filter'
  | 'invalid_stage'
  | 'invalid_assignee'
  | 'invalid_tag'

export interface SavedListsResponse {
  lists: SavedListMetadata[]
}

export interface SavedListDetailResponse {
  list: SavedListMetadata
  filter: FilterDefinition | null
  // SLICE_011b_SORT.md §6: `null` means the default order (`created.desc`
  // stored is indistinguishable from absent). A stored sort the binary
  // cannot read fails closed the same way as `filter: null` (§5), so this
  // is never observed diverging from `filter_error`.
  sort: PersonSortToken | null
  description: string[]
  filter_error: SavedListFilterError | null
}

export interface SavedListCountResponse {
  list_id: string
  revision: number
  count: number
  truncated: boolean
}

export interface CreateSavedListRequest {
  request_id: string
  scope: SavedListScope
  name: string
  filter: FilterDefinition
  // SLICE_011b_SORT.md §6: optional; `null`/absent means the default order.
  sort?: PersonSortToken | null
}

export interface CreateSavedListResponse {
  list: SavedListMetadata
  created: boolean
}

export interface UpdateSavedListRequest {
  expected_revision: number
  name: string
  filter: FilterDefinition
  // SLICE_011b_SORT.md §6: optional; `null`/absent means the default order.
  sort?: PersonSortToken | null
}

export interface UpdateSavedListResponse {
  list: SavedListMetadata
  changed: boolean
}

export interface DeleteSavedListRequest {
  expected_revision: number
}

export interface DeleteSavedListResponse {
  deleted: boolean
}

// --- Slice 011a: Inquiry sources (docs/specs/SLICE_011a.md §5b GET
// /api/inquiry-sources) --------------------------------------------------

export interface InquirySourcesResponse {
  sources: string[]
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

// SLICE_007c §5's declared additive change to the frozen `POST
// /api/inquiries` `routing_strategy` vocabulary: `organization_default`
// and `unassigned` appear there only on a `duplicate: true` replay of a
// system-routed row (nothing on the user-actor endpoint ever produces
// them directly). SLICE_008 §5 adds `round_robin` the same way.
export type RoutingStrategy =
  | 'explicit'
  | 'actor_default'
  | 'kept_existing'
  | 'organization_default'
  | 'unassigned'
  | 'round_robin'

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
  // SLICE_006c §2's declared additive change: `call_id` (= `causation_id`
  // when the attempt is call-derived, null for a manual attempt),
  // `corrects_id` (set on a correction row), `superseded` (a corrector
  // exists). Render from these, never from row position — a correction is
  // ordered after its original but not necessarily adjacent to it.
  call_id: string | null
  corrects_id: string | null
  superseded: boolean
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

// SLICE_009 §8's declared additive change: `correspondence`, `kind_rank` 6,
// `detail: {direction, agent, captured_at, via, backdated}` — deliberately
// no address/subject/message-id (D-042.1/2). `agent` names the attributed
// member; unlike `HistoryEntryBase.actor` (always null on these rows —
// Actor::System, no human actor caused an unattended capture), `agent` is
// always present.
export type CaptureDirection = 'inbound' | 'outbound'
export type CaptureVia = 'cc' | 'forward'

export interface CorrespondenceDetail {
  direction: CaptureDirection
  agent: ActorRef
  captured_at: string
  via: CaptureVia
  backdated: boolean
}

// SLICE_015 §5's declared additive change: `note`, `kind_rank` 7,
// `detail: {body, updated_at, edited, can_manage}`. `HistoryEntryBase.actor`
// is the note's author (`null` only for an imported note whose FUB author
// matched no member, §1 rule 1) — an edit does not move `occurred_at`, so
// the timeline position never changes on edit.
export interface NoteDetail {
  body: string
  updated_at: string
  edited: boolean
  // The server's rule-1 verdict for THIS viewer (admin, or the note's own
  // author) — a display hint; the command re-decides under the note row's
  // lock (§5).
  can_manage: boolean
}

// SLICE_016.md §4's declared additive change: `task_completed`, `kind_rank`
// 8 (after `note`), `detail: {title, kind, due_at, assignee, created_by,
// can_manage}`. `HistoryEntryBase.actor` is the completer's `ActorRef`, or
// `null` for an imported task (§1 rule 1). Reopening removes the entry;
// completing again re-adds it at the new time (occurred_at = completed_at).
export interface TaskCompletedDetail {
  title: string
  kind: TaskKind
  due_at: string | null
  assignee: ActorRef | null
  created_by: ActorRef | null
  // The server's rule-1 verdict for THIS viewer (admin, or the task's own
  // assignee/creator) — a display hint; the command re-decides under the
  // task row's lock (§4).
  can_manage: boolean
}

export type HistoryEntry =
  | (HistoryEntryBase & { kind: 'inquiry_received'; detail: InquiryReceivedDetail })
  | (HistoryEntryBase & { kind: 'routing_decision'; detail: RoutingDecisionDetail })
  | (HistoryEntryBase & { kind: 'assignment_changed'; detail: AssignmentChangedDetail })
  | (HistoryEntryBase & { kind: 'stage_changed'; detail: StageChangedDetail })
  | (HistoryEntryBase & { kind: 'contact_attempted'; detail: ContactAttemptedDetail })
  | (HistoryEntryBase & { kind: 'call_completed'; detail: CallCompletedDetail })
  | (HistoryEntryBase & { kind: 'correspondence'; detail: CorrespondenceDetail })
  | (HistoryEntryBase & { kind: 'note'; detail: NoteDetail })
  | (HistoryEntryBase & { kind: 'task_completed'; detail: TaskCompletedDetail })

export interface PersonDetailResponse {
  person: PersonSummary
  contact_methods: ContactMethod[]
  inquiries: PersonInquiry[]
  // Ordered by the server (occurred_at, recorded_at, kind_rank, id) — never
  // re-sort this client-side (§5).
  history: HistoryEntry[]
  // Slice 011e §5: ordered `lower(name), id`, additive.
  tags: TagRef[]
  // Slice 016a §4: open tasks only, in `open_for_person` order
  // (`due_at ASC NULLS LAST, created_at, id`) — never re-sort client-side.
  tasks: Task[]
  // Slice 019a §4: set values on LIVE fields only, in field position
  // order — never re-sort client-side.
  custom_fields: PersonCustomFieldValue[]
}

// --- Slice 011e: Tags (docs/specs/SLICE_011e.md §5) -------------------------
// Free-form tags on People: created inline by any member, applied/removed on
// the Person page, renamed/deleted by an admin or (while unused) the
// creator (D-051). `Tag` is the `GET /api/tags` row / mutation response
// shape; `TagRef` is the narrower `{id,name}` shape carried on the Person
// detail and the apply/remove responses — never widen one into the other.

export interface Tag {
  id: string
  name: string
  person_count: number
  // The server's rule-1 verdict for THIS viewer at read time — a display
  // hint; the command re-decides under the tag row's lock (§5).
  can_manage: boolean
}

export interface TagRef {
  id: string
  name: string
}

export interface TagsResponse {
  tags: Tag[]
}

export interface CreateTagRequest {
  name: string
}

export interface CreateTagResponse {
  tag: Tag
  created: boolean
}

export interface RenameTagRequest {
  name: string
}

export interface RenameTagResponse {
  tag: Tag
  changed: boolean
}

export interface DeleteTagResponse {
  deleted: boolean
  removed_from_people: number
}

/** `PUT`/`DELETE /api/people/{id}/tags/{tag_id}` — same shape for apply and remove. */
export interface PersonTagMutationResponse {
  tags: TagRef[]
  changed: boolean
}

// --- Slice 015: Notes (docs/specs/SLICE_015.md §5) --------------------------
// Free-text notes on a Person: written by any active member from the Person
// page, edited/deleted by the author or an Organization admin. No separate
// GET route — the Person detail's `history[]` (the `note` kind above) is the
// read surface; these are the three mutation shapes only.

/** `author` is `null` only for an imported note whose FUB author matched no
 * member (§1 rule 1) — the same `ActorRef | null` shape `HistoryEntryBase`
 * uses. */
export interface Note {
  id: string
  person_id: string
  body: string
  author: ActorRef | null
  created_at: string
  updated_at: string
  edited: boolean
  can_manage: boolean
}

export interface AddNoteRequest {
  body: string
}

export interface AddNoteResponse {
  note: Note
}

export interface EditNoteRequest {
  body: string
}

export interface EditNoteResponse {
  note: Note
  changed: boolean
}

export interface DeleteNoteResponse {
  deleted: boolean
}

// --- Slice 016a: Tasks (docs/specs/SLICE_016.md §4) -------------------------
// Typed to-dos on a Person: created by any active member from the Person
// page, edited/completed/reopened/deleted by the assignee, the creator, or
// an Organization admin. `tasks[]` on the Person detail (open only) is the
// list read surface; `task_completed` history entries (above) cover
// completed ones. `due_at` is an instant; a date-only pick is converted to
// local end of day BY THE CLIENT (rule 2 — no Organization timezone
// exists), never on the server.

export type TaskKind = 'call' | 'email' | 'text' | 'follow_up' | 'other'

/** `assignee`/`created_by` are `null` only for an imported task whose FUB
 * assignee/creator matched no member (§1 rule 1). */
export interface Task {
  id: string
  person_id: string
  title: string
  kind: TaskKind
  due_at: string | null
  assignee: ActorRef | null
  created_by: ActorRef | null
  completed_at: string | null
  completed_by: ActorRef | null
  created_at: string
  updated_at: string
  // The server's rule-1 verdict for THIS viewer at read time — a display
  // hint; every mutating command re-decides under the task row's lock (§4).
  can_manage: boolean
}

export interface CreateTaskRequest {
  title: string
  kind?: TaskKind
  due_at?: string | null
  assignee_user_id?: string | null
}

export interface CreateTaskResponse {
  task: Task
}

/** `PUT .../tasks/{task_id}`: full replace of all four mutable fields — no
 * partial update exists (§4). `assignee_user_id` is required, unlike
 * `CreateTaskRequest` (an existing task always has one to keep or change). */
export interface UpdateTaskRequest {
  title: string
  kind: TaskKind
  due_at: string | null
  assignee_user_id: string
}

export interface UpdateTaskResponse {
  task: Task
  changed: boolean
}

export interface CompleteTaskResponse {
  task: Task
  changed: boolean
}

export interface ReopenTaskResponse {
  task: Task
  changed: boolean
}

/** `POST .../tasks/{task_id}/snooze`: its own route (never a full
 * `UpdateTaskRequest`) so the 016b Today panel can snooze from a possibly
 * stale row — due_at only. */
export interface SnoozeTaskRequest {
  due_at: string
}

export interface SnoozeTaskResponse {
  task: Task
  changed: boolean
}

export interface DeleteTaskResponse {
  deleted: boolean
}

// --- Slice 016b: GET /api/tasks?scope=mine (docs/specs/SLICE_016.md §4) ----
// The Today page's Tasks panel: the viewer's own open, dated tasks with
// `due_at <= generated_at + 24h`, ordered `due_at, id`. `TaskWithPerson` is
// `Task` plus the owning Person's minimal reference (id + display name for
// the panel row's "Person's name as a link").

export interface TaskPersonRef {
  id: string
  display_name: string
}

export interface TaskWithPerson extends Task {
  person: TaskPersonRef
}

export interface ListTasksResponse {
  tasks: TaskWithPerson[]
  generated_at: string
  truncated: boolean
}

// --- Slice 019a: Custom fields (docs/specs/SLICE_019.md §4) -----------------
// Typed custom fields on People: an Organization admin defines them under
// Manage → Fields (label, type, and for `choice` its options); any active
// member sets or clears a value on a Person. Definitions and options are
// archived, never deleted — `archived_at` is `null` for a live row.

export type CustomFieldType = 'text' | 'number' | 'date' | 'choice'

export interface CustomFieldOption {
  id: string
  label: string
  position: number
  archived_at: string | null
}

/** `GET /api/custom-fields` row / every definition-write response's `field`
 * (§4): options ordered `position, id`, always `[]` for a non-`choice` type. */
export interface CustomField {
  id: string
  label: string
  field_type: CustomFieldType
  position: number
  archived_at: string | null
  person_count: number
  options: CustomFieldOption[]
}

export interface CustomFieldsResponse {
  fields: CustomField[]
}

export interface CreateCustomFieldRequest {
  label: string
  field_type: CustomFieldType
  options?: string[]
}

export interface CreateCustomFieldResponse {
  field: CustomField
}

/** `PUT /api/custom-fields/order`: the full live order, no more, no fewer. */
export interface ReorderCustomFieldsRequest {
  field_ids: string[]
}

export interface ReorderCustomFieldsResponse {
  fields: CustomField[]
}

/** `PUT /api/custom-fields/{field_id}`: full replace (rename/archive/
 * restore all in one shape — the `UpdateTaskRequest` precedent). */
export interface UpdateCustomFieldRequest {
  label: string
  archived: boolean
}

export interface UpdateCustomFieldResponse {
  field: CustomField
  changed: boolean
}

export interface AddCustomFieldOptionRequest {
  label: string
}

export interface AddCustomFieldOptionResponse {
  field: CustomField
}

export interface UpdateCustomFieldOptionRequest {
  label: string
  archived: boolean
}

export interface UpdateCustomFieldOptionResponse {
  field: CustomField
  changed: boolean
}

/** The externally tagged value payload (§4): exactly one key. A `number` is
 * always the server's/client's decimal STRING (§2: no decimal type crosses
 * the wire) — never a JSON number, which the server rejects as 400. */
export type CustomFieldValuePayload =
  | { text: string }
  | { number: string }
  | { date: string }
  | { option_id: string }

/** A Person's value for one live custom field (§4) — `GET /api/people/{id}`'s
 * `custom_fields[]` row and the shape of the value mutations' own list. */
export interface PersonCustomFieldValue {
  field_id: string
  label: string
  field_type: CustomFieldType
  value: CustomFieldValuePayload
  option_label: string | null
  updated_at: string
}

export interface SetCustomFieldValueRequest {
  value: CustomFieldValuePayload
}

/** `PUT`/`DELETE /api/people/{id}/custom-fields/{field_id}` — same shape
 * for set and clear. */
export interface PersonCustomFieldValueMutationResponse {
  custom_fields: PersonCustomFieldValue[]
  changed: boolean
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

export type UnresolvedReason =
  | 'invalid_json'
  | 'not_an_object'
  | 'no_contact_method'
  | 'email_unparsed'
  | 'email_unrecognized_format'
  | 'not_a_lead'
  | 'email_extraction_failed'

// ---- SLICE_007e §5: the Unresolved workbench (admin-only, D-037) ---------

export type UnresolvedDetailContent =
  | {
      kind: 'email'
      subject: string | null
      from_display: string | null
      from_addr: string | null
      date: string | null
      text: string | null
      truncated: boolean
    }
  | { kind: 'text'; text: string; truncated: boolean }

export interface UnresolvedDetailResponse {
  id: string
  source: string
  payload_format: string
  received_at: string
  resolution: UnresolvedResolution
  reason: UnresolvedReason | null
  byte_len: number
  content: UnresolvedDetailContent
}

/** `POST .../retry` — the SLICE_002 §5 outcome vocabulary reused. */
export type RetryUnresolvedResponse =
  | {
      status: 'resolved'
      inquiry_id: string
      person_id: string
      person_created: boolean
      routing_strategy: RoutingStrategy
      assigned_user_id: string | null
      duplicate: boolean
    }
  | {
      status: 'unresolved'
      raw_payload_id: string
      reason: UnresolvedReason
      duplicate: boolean
    }

export interface DiscardUnresolvedResponse {
  status: 'discarded'
}

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

// SLICE_006c §5a (D-033): `low` is the "outcome needed" tier, served after
// every other item; `set_outcome` is its recommended action.
export type TodayPriority = 'high' | 'normal' | 'list' | 'low'
export type RecommendedAction = 'call' | 'email' | 'review_person' | 'set_outcome'

// Discriminated on `code`, in the fixed wire order (§3) — never re-sorted
// client-side, same discipline as `history` above. `call_outcome_needed`
// (SLICE_006c §5a) names the viewer's own call whose outcome is still the
// automatic root; it may be appended to the inquiry-based reasons.
// SLICE_009 §6's declared additive change: `client_replied`, which WINS
// the reason slot in place of the Inquiry-based trio when a Person
// qualifies for both (the server never emits it alongside them).
// Slice 016b (docs/specs/SLICE_016.md §5, D-054 §1): the built-in task
// axis's two reasons. `title` rides the reason for the Web (the
// `list_member.name` precedent) — clip it before display (§8: the
// `list_member` chip precedent again). `task_overdue` is `due_at < now`
// (strict) at evaluation time; `task_due` is `due_at >= now`, within the
// 24h admission window.
export type TodayReason =
  | { code: 'new_inquiry'; source: string; received_at: string }
  | { code: 'no_contact_attempt'; since: string }
  | { code: 'repeat_inquiry'; inquiry_count: number }
  | { code: 'call_outcome_needed'; call_id: string; ended_at: string }
  | { code: 'client_replied'; occurred_at: string }
  | { code: 'list_member'; list_id: string; name: string }
  | { code: 'task_overdue'; task_id: string; title: string; kind: TaskKind; due_at: string }
  | { code: 'task_due'; task_id: string; title: string; kind: TaskKind; due_at: string }

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
  waiting_since: string | null
  latest_inquiry: TodayInquiryRef | null
  last_contact_attempt: ContactAttemptRef | null
}

export interface TodayResponse {
  generated_at: string
  items: TodayItem[]
  truncated: boolean
  sources: TodaySourcesStatus
}

// --- Slice 011d: system feed issues on the Today `sources` envelope
// (docs/specs/SLICE_011d.md §5, §6) — additive field, `sources` shape
// otherwise unchanged. -------------------------------------------------

export type SystemFeedIssueError = 'unavailable' | 'invalid_definition'
/** Slice 016b (docs/specs/SLICE_016.md §5): `'task_due'` joins the issue
 * keys with `error: 'unavailable', fallback: false` — a token with no feed
 * row, so it never appears in `TodayFeedKey` (the admin PUT paths' own
 * closed vocabulary, which this union does NOT widen). */
export interface SystemFeedIssue {
  feed_key: TodayFeedKey | 'task_due'
  error: SystemFeedIssueError
  /** `true` when the canonical default was evaluated in place of an invalid
   * stored definition (`invalid_definition`); `false` when the feed
   * contributed nothing (`unavailable`, §5 rule 6). */
  fallback: boolean
}

export type TodaySourceStatus = 'complete' | 'partial' | 'unavailable'
export type TodaySourceIssueError =
  | 'unsupported_filter'
  | 'invalid_stage'
  | 'invalid_assignee'
  | 'invalid_tag'
  | 'unavailable'
export interface TodaySourceIssue {
  list_id: string
  name: string
  revision: number
  error: TodaySourceIssueError
}
export interface TodaySourcesStatus {
  status: TodaySourceStatus
  issues: TodaySourceIssue[]
  // SLICE_011d §5, §6: additive.
  system_feed_issues: SystemFeedIssue[]
}
export interface TodaySource {
  list_id: string
  name: string
  scope: SavedListScope
  revision: number
  filter_error: SavedListFilterError | null
}
export interface TodaySourcesResponse {
  limit: number
  sources: TodaySource[]
}
export interface EnableTodaySourceRequest { expected_list_revision: number }
export interface TodaySourceChange { enabled: boolean; changed: boolean }

// --- Slice 011d: Today system feeds (docs/specs/SLICE_011d.md §3, §4, §6) --
// The three built-in Today rules re-expressed as per-Organization,
// admin-editable feeds in the same filter vocabulary. Mirrors §6's wire
// shapes verbatim; this file owns the contract for the web lane
// (SLICE_011d_IMPL.md "Lane W").

export type TodayFeedKey = 'unanswered_inquiry' | 'client_replied' | 'call_outcome_needed'

export interface UserRef {
  id: string
  display_name: string
}

// Same three codes as `SavedListFilterError` (§6: "the 011b neutral
// placeholders" reuse); aliased rather than duplicated so the two contracts
// cannot silently drift apart in this file.
export type TodayFeedFilterError = SavedListFilterError

/** The admin-only shape returned by `GET /api/organization/today-feeds` and
 * every admin mutation (§6). `filter` is the stored typed definition — the
 * canonical one when `is_default` — and is `null` only when the stored JSON
 * is `unsupported_filter`. `fresh_within_hours` is absent/null for the call
 * feed (both mean "none"); a person-state feed always carries a number. */
export interface Feed {
  feed_key: TodayFeedKey
  enabled: boolean
  revision: number
  is_default: boolean
  filter: FilterDefinition | null
  fresh_within_hours: number | null
  description: string[]
  filter_error: TodayFeedFilterError | null
  updated_at: string
  updated_by: UserRef | null
  default: { filter: FilterDefinition; fresh_within_hours: number | null }
}

/** The member-visible shape returned by `GET /api/today/feeds` (§6) —
 * reflects the EFFECTIVE rule (canonical under fallback); no editor
 * identity, revision or raw JSON. The fallback itself is reported through
 * Today's `system_feed_issues`, not here. */
export interface MemberFeed {
  feed_key: TodayFeedKey
  enabled: boolean
  is_default: boolean
  description: string[]
}

export interface TodayFeedsResponse {
  feeds: Feed[]
}

export interface MemberTodayFeedsResponse {
  feeds: MemberFeed[]
}

/** `PUT /api/organization/today-feeds/{feed_key}` body. `fresh_within_hours`
 * is `null` for the call feed and a `1..=8760` integer for the two
 * person-state feeds (§4). */
export interface UpdateTodayFeedRequest {
  expected_revision: number
  filter: FilterDefinition
  fresh_within_hours: number | null
}

/** `POST /api/organization/today-feeds/{feed_key}/revert` and
 * `PUT .../enabled`'s shared envelope, plus the enable/disable-only field. */
export interface RevertTodayFeedRequest {
  expected_revision: number
}

export interface SetTodayFeedEnabledRequest {
  expected_revision: number
  enabled: boolean
}

/** Response body shared by update/revert/enable-disable (§6). */
export interface TodayFeedMutationResponse {
  feed: Feed
  changed: boolean
}

/** `POST /api/organization/today-feeds/{feed_key}/preview` body — never
 * persisted (§4). `subject_user_id` omitted defaults server-side to the
 * requesting admin; the Web client always sends it explicitly. */
export interface PreviewTodayFeedRequest {
  filter: FilterDefinition
  fresh_within_hours: number | null
  subject_user_id?: string
}

export interface PreviewTodayFeedResponse {
  subject: UserRef
  items: TodayItem[]
  truncated: boolean
  description: string[]
}

// ---- Contact attempts (SLICE_003 §5 POST /api/people/{id}/contact-attempts)

export type ContactChannel = 'call' | 'text' | 'email' | 'other'
// SLICE_006c §2 widens the vocabulary with `busy` and `wrong_number`.
export type ContactOutcome = 'reached' | 'no_answer' | 'left_message' | 'sent' | 'busy' | 'wrong_number'

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
  /** docs/specs/SLICE_018.md §3, §5: `-new Date().getTimezoneOffset()`;
   * additive, −840..=840 (400 outside that range). Used server-side only
   * for `create_task`'s due-instant composition and the prompt's local-time
   * line — never trusted identity or authorization context. */
  utc_offset_minutes: number
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

/** SLICE_006b §4: the turn's `start_call` proposal. The card renders from
 * this server-built object only — never from model prose. Renamed from
 * `OperatorProposal` (docs/specs/SLICE_018.md §5, §8): `OperatorProposal`
 * is now the `kind`-discriminated union of this and
 * `OperatorCreateTaskProposal`. */
export interface OperatorStartCallProposal {
  id: string
  kind: 'start_call'
  person: OperatorPersonCard
  phone: string
  contact_method_id: string
  expires_at: string
}

/** docs/specs/SLICE_018.md §5: `assignee` on a `create_task` proposal —
 * `display_name` is trusted reference-table text (an active member's own
 * name), not outside text, so a plain string. */
export interface OperatorMemberRef {
  id: string
  display_name: string
}

/** docs/specs/SLICE_018.md §5: the turn's `create_task` proposal. The card
 * renders from this server-built object only — never from model prose;
 * `title` is untrusted (model-authored) text, rendered by interpolation
 * only, the same discipline as `reply`. */
export interface OperatorCreateTaskProposal {
  id: string
  kind: 'create_task'
  person: OperatorPersonCard
  title: string
  task_kind: TaskKind
  due_at: string | null
  assignee: OperatorMemberRef
  expires_at: string
}

/** docs/specs/SLICE_018.md §5: a `kind`-discriminated union — the wire's
 * `proposal` field is exactly one of these two shapes. */
export type OperatorProposal = OperatorStartCallProposal | OperatorCreateTaskProposal

/** docs/specs/SLICE_018.md §5: the turn's `complete_task` receipt, present
 * only on 200 outcomes. The card renders from this server-built object
 * only — never from model prose; `title` is untrusted (model-independent,
 * but still user-authored) text, rendered by interpolation only. No
 * `completed_by_display_name` on the wire (model-facing only, not part of
 * this frozen shape). */
export interface OperatorReceipt {
  kind: 'complete_task'
  task_id: string
  person: OperatorPersonCard
  title: string
  task_kind: TaskKind
  due_at: string | null
  completed_at: string
}

export interface OperatorTurnResponse {
  turn_id: string
  /** Plain text. Rendered by interpolation only — never as HTML or markdown. */
  reply: string
  references: { people: OperatorPersonCard[] }
  tool_calls: OperatorToolCall[]
  proposal: OperatorProposal | null
  /** docs/specs/SLICE_018.md §5: additive nullable, present only on 200
   * outcomes. */
  receipt: OperatorReceipt | null
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

// --- Slice 006c: Call outcome correction (docs/specs/SLICE_006c.md §5) ------

/** The five values `POST /api/calls/{id}/outcome` accepts — `ContactOutcome`
 * minus `sent` (rejected with 400 server-side). */
export type CallOutcomeCorrection = Exclude<ContactOutcome, 'sent'>

/** `deny_unknown_fields` server-side: exactly this one field. */
export interface CorrectOutcomeRequest {
  outcome: CallOutcomeCorrection
}

/** The effective attempt after the command — the new correction row, or
 * the unchanged head when `changed: false`. A new type; `ContactAttemptRef`
 * is unchanged. */
export interface CorrectedAttemptRef {
  id: string
  channel: ContactChannel
  outcome: ContactOutcome
  occurred_at: string
  recorded_at: string
  corrects_id: string | null
}

export interface CorrectOutcomeResponse {
  attempt: CorrectedAttemptRef
  changed: boolean
}

// --- Slice 009: Correspondence capture (docs/specs/SLICE_009.md §8) --------

/** `GET /api/capture/address`, `POST /api/capture/address/rotate` — both
 * return this same shape (member-self, NOT the admin `IntakeAddressResponse`
 * shape: no `scheme`, since capture has exactly one grammar). */
export interface CaptureAddressResponse {
  address: string
}

export type CaptureMessageStatus = 'held' | 'linked' | 'dismissed'

/** `GET /api/capture/unmatched` item — the attributed agent's own held
 * queue only. `status` is always `'held'` in this list (the endpoint only
 * ever returns held rows) but the field is frozen in the shape regardless. */
export interface CaptureUnmatchedItem {
  id: string
  counterparty_email: string | null
  captured_at: string
  direction_hint: CaptureDirection | null
  status: CaptureMessageStatus
}

export interface CaptureUnmatchedResponse {
  items: CaptureUnmatchedItem[]
  truncated: boolean
}

/** `deny_unknown_fields` server-side: exactly these two fields. */
export interface LinkUnmatchedRequest {
  person_id: string
  add_contact_method: boolean
}
