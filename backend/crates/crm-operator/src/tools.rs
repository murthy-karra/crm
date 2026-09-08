//! The seven read-only tools plus `start_call` (docs/specs/SLICE_005.md §3,
//! §7; docs/specs/SLICE_013.md §2): their JSON-schema definitions (a
//! snapshot-pinned shared contract) and the validation of model-supplied
//! arguments against them. No schema carries an Organization or user id
//! (`list_id` is the sole declared exception — a re-validated resource id,
//! not a trusted one, docs/specs/SLICE_013.md §4); every object is
//! `additionalProperties: false`, so an invented trusted id is
//! `invalid_arguments`.

use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::backend::{AgeCondition, PeopleFilterSpec, SavedListSelector};
use crate::provider::ToolDefinition;

pub const SEARCH_QUERY_MAX_CHARS: usize = 100;
pub const SEARCH_LIMIT_MIN: u64 = 1;
pub const SEARCH_LIMIT_MAX: u64 = 10;
pub const TODAY_LIMIT_MIN: u64 = 1;
pub const TODAY_LIMIT_MAX: u64 = 20;

/// docs/specs/SLICE_013.md §1 rule 4: 1..=25, default 10 — unlike
/// `search_people`/`get_today`, whose absent `limit` defaults to the max.
pub const FILTER_LIMIT_MIN: u64 = 1;
pub const FILTER_LIMIT_MAX: u64 = 25;
pub const FILTER_LIMIT_DEFAULT: u64 = 10;
/// `MAX_VALUES` mirrored from `crm-app::domain::person::filter` without a
/// dependency on it (D-034): a name-array property must have 1..=50
/// entries once the model supplies it at all.
pub const FILTER_ARRAY_MIN: usize = 1;
pub const FILTER_ARRAY_MAX: usize = 50;
/// `MIN_DAYS`/`MAX_DAYS` mirrored from the same module, same reason.
pub const FILTER_MIN_DAYS: i64 = 1;
pub const FILTER_MAX_DAYS: i64 = 3650;
pub const STAGE_NAME_MAX_CHARS: usize = 80;
pub const SAVED_LIST_NAME_MAX_CHARS: usize = 80;

pub const SEARCH_PEOPLE: &str = "search_people";
pub const GET_PERSON: &str = "get_person";
pub const GET_TODAY: &str = "get_today";
pub const GET_NEXT_WORK_ITEM: &str = "get_next_work_item";
pub const EXPLAIN_PRIORITY: &str = "explain_priority";
pub const START_CALL: &str = "start_call";
pub const FILTER_PEOPLE: &str = "filter_people";
pub const RUN_SAVED_LIST: &str = "run_saved_list";

/// The tool contract offered to the model on every call.
pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: SEARCH_PEOPLE,
            description: "Find People in the user's Organization whose first or last name contains the query (case-insensitive), or whose email or phone exactly matches it. Returns up to `limit` Person cards.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "A name fragment, an email address, or a phone number.",
                        "minLength": 1,
                        "maxLength": SEARCH_QUERY_MAX_CHARS
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of matches to return.",
                        "minimum": SEARCH_LIMIT_MIN,
                        "maximum": SEARCH_LIMIT_MAX,
                        "default": SEARCH_LIMIT_MAX
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: GET_PERSON,
            description: "Get one Person's card, contact methods, latest inquiries, recent history, and their membership in the bounded Today results, including source availability. Use an id from a previous tool result.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "person_id": {
                        "type": "string",
                        "format": "uuid",
                        "description": "The Person's id."
                    }
                },
                "required": ["person_id"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: GET_TODAY,
            description: "The user's bounded Today results: built-in work and enabled saved-list matches in the exact CRM order, with source availability. Report the order as given; never reorder or infer omitted membership.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of items to return.",
                        "minimum": TODAY_LIMIT_MIN,
                        "maximum": TODAY_LIMIT_MAX,
                        "default": TODAY_LIMIT_MAX
                    }
                },
                "required": [],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: GET_NEXT_WORK_ITEM,
            description: "The first available item in the user's bounded Today results, plus truncation and source availability.",
            parameters: json!({
                "type": "object",
                "properties": {},
                "required": [],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: START_CALL,
            description: "Propose a phone call to a Person. This does NOT place the call: it prepares a proposal the user must confirm in the app before anything happens. Use only when the user explicitly asks to call someone. If the Person has several phone numbers, call this without contact_method_id first to see the options, then ask the user which to use.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "person_id": {
                        "type": "string",
                        "format": "uuid",
                        "description": "The Person's id, from a previous tool result."
                    },
                    "contact_method_id": {
                        "type": "string",
                        "format": "uuid",
                        "description": "The chosen phone contact method's id, from a previous start_call result. Omit when the Person has one phone number."
                    }
                },
                "required": ["person_id"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: EXPLAIN_PRIORITY,
            description: "Explain a Person's returned Today position, tier, reasons, ordering keys, truncation, and source availability. If absent, do not infer why they are not in the returned results.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "person_id": {
                        "type": "string",
                        "format": "uuid",
                        "description": "The Person's id."
                    }
                },
                "required": ["person_id"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: FILTER_PEOPLE,
            description: "Find People in the user's Organization matching named criteria: stage, assignee, source, tags, recency windows (created/last inquiry/last contact/last inbound), and reply/contact-method/outcome flags. Every name is resolved against the Organization's own vocabulary, case-insensitively; an unknown or ambiguous name comes back as a clarification to put to the user, never a guess. At least one property is required.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "stage_names": {
                        "type": "array",
                        "description": "Stage names to match (any of).",
                        "items": { "type": "string", "maxLength": STAGE_NAME_MAX_CHARS },
                        "minItems": FILTER_ARRAY_MIN,
                        "maxItems": FILTER_ARRAY_MAX
                    },
                    "assignees": {
                        "type": "array",
                        "description": "\"me\", \"unassigned\", or a member's display name (any of).",
                        "items": { "type": "string" },
                        "minItems": FILTER_ARRAY_MIN,
                        "maxItems": FILTER_ARRAY_MAX
                    },
                    "sources": {
                        "type": "array",
                        "description": "Lead source tokens to match (any of).",
                        "items": { "type": "string", "pattern": "^[a-z0-9_]{1,64}$" },
                        "minItems": FILTER_ARRAY_MIN,
                        "maxItems": FILTER_ARRAY_MAX
                    },
                    "tag_names_any": {
                        "type": "array",
                        "description": "Match People carrying at least one of these tags.",
                        "items": { "type": "string" },
                        "minItems": FILTER_ARRAY_MIN,
                        "maxItems": FILTER_ARRAY_MAX
                    },
                    "tag_names_none": {
                        "type": "array",
                        "description": "Match People carrying none of these tags.",
                        "items": { "type": "string" },
                        "minItems": FILTER_ARRAY_MIN,
                        "maxItems": FILTER_ARRAY_MAX
                    },
                    "created": {
                        "type": "object",
                        "description": "When the Person was created.",
                        "properties": {
                            "op": { "type": "string", "enum": ["within_days", "not_within_days"] },
                            "days": { "type": "integer", "minimum": FILTER_MIN_DAYS, "maximum": FILTER_MAX_DAYS }
                        },
                        "required": ["op", "days"],
                        "additionalProperties": false
                    },
                    "last_inquiry": {
                        "type": "object",
                        "description": "Time since the Person's last inquiry; \"never\" matches People with no inquiry.",
                        "properties": {
                            "op": { "type": "string", "enum": ["within_days", "not_within_days", "never"] },
                            "days": { "type": "integer", "minimum": FILTER_MIN_DAYS, "maximum": FILTER_MAX_DAYS }
                        },
                        "required": ["op"],
                        "additionalProperties": false
                    },
                    "last_contact": {
                        "type": "object",
                        "description": "Time since the last contact attempt; \"never\" matches People never contacted.",
                        "properties": {
                            "op": { "type": "string", "enum": ["within_days", "not_within_days", "never"] },
                            "days": { "type": "integer", "minimum": FILTER_MIN_DAYS, "maximum": FILTER_MAX_DAYS }
                        },
                        "required": ["op"],
                        "additionalProperties": false
                    },
                    "last_inbound": {
                        "type": "object",
                        "description": "Time since the last inbound correspondence; \"never\" matches People with none.",
                        "properties": {
                            "op": { "type": "string", "enum": ["within_days", "not_within_days", "never"] },
                            "days": { "type": "integer", "minimum": FILTER_MIN_DAYS, "maximum": FILTER_MAX_DAYS }
                        },
                        "required": ["op"],
                        "additionalProperties": false
                    },
                    "has_replied": {
                        "type": "boolean",
                        "description": "Whether the Person has ever replied."
                    },
                    "has_phone": {
                        "type": "boolean",
                        "description": "Whether the Person has a phone contact method on file."
                    },
                    "has_email": {
                        "type": "boolean",
                        "description": "Whether the Person has an email contact method on file."
                    },
                    "awaiting_response": {
                        "type": "boolean",
                        "description": "Whether the Person is awaiting a response from the team."
                    },
                    "client_replied_unanswered": {
                        "type": "boolean",
                        "description": "Whether the Person's latest correspondence is an unanswered reply from them."
                    },
                    "awaiting_call_outcome": {
                        "type": "boolean",
                        "description": "Whether the user has an ended call to this Person with no recorded outcome yet."
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of matching Person cards to return.",
                        "minimum": FILTER_LIMIT_MIN,
                        "maximum": FILTER_LIMIT_MAX,
                        "default": FILTER_LIMIT_DEFAULT
                    }
                },
                "required": [],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: RUN_SAVED_LIST,
            description: "Run a saved People list the user can see (their own personal lists plus the Organization's shared lists) by name, in the list's own stored sort order. If more than one visible list shares the name, this returns candidates with ids instead of results; call again with list_id set to the chosen one.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "The saved list's name.",
                        "maxLength": SAVED_LIST_NAME_MAX_CHARS
                    },
                    "list_id": {
                        "type": "string",
                        "format": "uuid",
                        "description": "A candidate list's id, from a previous run_saved_list clarification. Use only after a duplicate-name clarification named it."
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of matching Person cards to return.",
                        "minimum": FILTER_LIMIT_MIN,
                        "maximum": FILTER_LIMIT_MAX,
                        "default": FILTER_LIMIT_DEFAULT
                    }
                },
                "required": [],
                "additionalProperties": false
            }),
        },
    ]
}

/// A validated, bounded tool invocation — the only thing the loop passes to
/// a `ToolBackend`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolInvocation {
    SearchPeople {
        query: String,
        limit: usize,
    },
    GetPerson {
        person_id: Uuid,
    },
    GetToday {
        limit: usize,
    },
    GetNextWorkItem,
    ExplainPriority {
        person_id: Uuid,
    },
    StartCall {
        person_id: Uuid,
        contact_method_id: Option<Uuid>,
    },
    FilterPeople {
        spec: PeopleFilterSpec,
    },
    RunSavedList {
        selector: SavedListSelector,
    },
}

impl ToolInvocation {
    pub fn tool_name(&self) -> &'static str {
        match self {
            ToolInvocation::SearchPeople { .. } => SEARCH_PEOPLE,
            ToolInvocation::GetPerson { .. } => GET_PERSON,
            ToolInvocation::GetToday { .. } => GET_TODAY,
            ToolInvocation::GetNextWorkItem => GET_NEXT_WORK_ITEM,
            ToolInvocation::ExplainPriority { .. } => EXPLAIN_PRIORITY,
            ToolInvocation::StartCall { .. } => START_CALL,
            ToolInvocation::FilterPeople { .. } => FILTER_PEOPLE,
            ToolInvocation::RunSavedList { .. } => RUN_SAVED_LIST,
        }
    }
}

/// Why a model-supplied call was rejected. The strings are fixed reasons
/// shown to the model; they never echo argument text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgumentError {
    UnknownTool,
    NotJson,
    NotAnObject,
    UnknownProperty(String),
    MissingProperty(&'static str),
    WrongType(&'static str),
    InvalidUuid(&'static str),
    EmptyQuery,
    /// A `string[1..50]` array property was present but empty (docs/specs/
    /// SLICE_013.md §2).
    EmptyArray(&'static str),
    /// A `string[1..50]` array property had more than
    /// [`FILTER_ARRAY_MAX`] entries.
    TooManyValues(&'static str),
    /// An age condition's `days` was outside 1..=3650.
    DaysOutOfRange(&'static str),
    /// An age condition's `op` was not one of the property's allowed
    /// values — in particular `created.never` (§1 rule 5's mirror: `never`
    /// is excluded for `created`, docs/specs/SLICE_011a.md §4b).
    InvalidOp(&'static str),
    /// `filter_people` with no condition at all (§1 rule 5): the People
    /// page's valid-empty semantics are for a table, not a chat reply.
    EmptyFilterSpec,
    /// `run_saved_list` with neither `name` nor `list_id`.
    MissingOneOf(&'static str, &'static str),
    /// `run_saved_list` with both `name` and `list_id`.
    ConflictingProperties(&'static str, &'static str),
}

impl ArgumentError {
    pub fn message(&self) -> String {
        match self {
            ArgumentError::UnknownTool => "unknown tool".to_string(),
            ArgumentError::NotJson => "arguments are not valid JSON".to_string(),
            ArgumentError::NotAnObject => "arguments must be a JSON object".to_string(),
            ArgumentError::UnknownProperty(name) => format!("unknown property: {name}"),
            ArgumentError::MissingProperty(name) => format!("missing required property: {name}"),
            ArgumentError::WrongType(name) => format!("wrong type for property: {name}"),
            ArgumentError::InvalidUuid(name) => format!("property is not a UUID: {name}"),
            ArgumentError::EmptyQuery => "query must not be empty".to_string(),
            ArgumentError::EmptyArray(name) => format!("{name} must not be empty"),
            ArgumentError::TooManyValues(name) => {
                format!("too many values for property: {name}")
            }
            ArgumentError::DaysOutOfRange(name) => {
                format!("days out of range (1-3650) for property: {name}")
            }
            ArgumentError::InvalidOp(name) => format!("invalid operator for property: {name}"),
            ArgumentError::EmptyFilterSpec => {
                "at least one filter condition is required".to_string()
            }
            ArgumentError::MissingOneOf(a, b) => format!("exactly one of {a} or {b} is required"),
            ArgumentError::ConflictingProperties(a, b) => {
                format!("only one of {a} or {b} may be given")
            }
        }
    }
}

fn known_properties(name: &str) -> Option<&'static [&'static str]> {
    match name {
        SEARCH_PEOPLE => Some(&["query", "limit"]),
        GET_PERSON | EXPLAIN_PRIORITY => Some(&["person_id"]),
        START_CALL => Some(&["person_id", "contact_method_id"]),
        GET_TODAY => Some(&["limit"]),
        GET_NEXT_WORK_ITEM => Some(&[]),
        // docs/specs/SLICE_013.md §2, §4: `list_id` is the sole id-shaped
        // property either tool declares — it names a candidate a prior
        // `run_saved_list` clarification already offered (the `start_call`
        // `contact_method_id` precedent), and the adapter re-validates it
        // through the visibility predicate rather than trusting it, exactly
        // like `person_id`/`contact_method_id` above.
        FILTER_PEOPLE => Some(&[
            "stage_names",
            "assignees",
            "sources",
            "tag_names_any",
            "tag_names_none",
            "created",
            "last_inquiry",
            "last_contact",
            "last_inbound",
            "has_replied",
            "has_phone",
            "has_email",
            "awaiting_response",
            "client_replied_unanswered",
            "awaiting_call_outcome",
            "limit",
        ]),
        RUN_SAVED_LIST => Some(&["name", "list_id", "limit"]),
        _ => None,
    }
}

/// Parses and validates `arguments` for `name`. Unknown tool, non-JSON,
/// non-object, unknown properties, missing required properties, and wrong
/// types are errors; out-of-range `limit`s are clamped and an over-long
/// `query` is clipped (§3: "clamped server-side as well"). Property names
/// in `UnknownProperty` are clipped so a hostile key cannot smuggle long
/// text back into the prompt.
pub fn parse_invocation(name: &str, arguments: &str) -> Result<ToolInvocation, ArgumentError> {
    let allowed = known_properties(name).ok_or(ArgumentError::UnknownTool)?;
    let value: Value = if arguments.trim().is_empty() {
        Value::Object(Default::default())
    } else {
        serde_json::from_str(arguments).map_err(|_| ArgumentError::NotJson)?
    };
    let object = value.as_object().ok_or(ArgumentError::NotAnObject)?;
    if let Some(extra) = object.keys().find(|k| !allowed.contains(&k.as_str())) {
        return Err(ArgumentError::UnknownProperty(
            extra.chars().take(32).collect(),
        ));
    }

    match name {
        SEARCH_PEOPLE => {
            let query = object
                .get("query")
                .ok_or(ArgumentError::MissingProperty("query"))?
                .as_str()
                .ok_or(ArgumentError::WrongType("query"))?;
            // Control characters (NUL in particular is rejected by Postgres
            // as an encoding error, which would read as a backend outage)
            // and invisible formatting characters are stripped, never
            // passed through.
            let query: String = query
                .chars()
                .filter(|c| !c.is_control() && !crate::views::is_invisible_format(*c))
                .collect::<String>()
                .trim()
                .chars()
                .take(SEARCH_QUERY_MAX_CHARS)
                .collect();
            if query.is_empty() {
                return Err(ArgumentError::EmptyQuery);
            }
            let limit = parse_limit(object.get("limit"), SEARCH_LIMIT_MIN, SEARCH_LIMIT_MAX)?;
            Ok(ToolInvocation::SearchPeople { query, limit })
        }
        GET_PERSON => Ok(ToolInvocation::GetPerson {
            person_id: parse_uuid(object.get("person_id"))?,
        }),
        EXPLAIN_PRIORITY => Ok(ToolInvocation::ExplainPriority {
            person_id: parse_uuid(object.get("person_id"))?,
        }),
        GET_TODAY => Ok(ToolInvocation::GetToday {
            limit: parse_limit(object.get("limit"), TODAY_LIMIT_MIN, TODAY_LIMIT_MAX)?,
        }),
        GET_NEXT_WORK_ITEM => Ok(ToolInvocation::GetNextWorkItem),
        START_CALL => Ok(ToolInvocation::StartCall {
            person_id: parse_uuid(object.get("person_id"))?,
            contact_method_id: match object.get("contact_method_id") {
                None | Some(Value::Null) => None,
                some => Some(parse_uuid_named(some, "contact_method_id")?),
            },
        }),
        FILTER_PEOPLE => Ok(ToolInvocation::FilterPeople {
            spec: parse_people_filter_spec(object)?,
        }),
        RUN_SAVED_LIST => Ok(ToolInvocation::RunSavedList {
            selector: parse_saved_list_selector(object)?,
        }),
        _ => Err(ArgumentError::UnknownTool),
    }
}

/// `filter_people`'s full body (docs/specs/SLICE_013.md §2): every
/// property is optional, but at least one must resolve to something
/// (§1 rule 5). Structural checks only — array length/emptiness, `days`
/// bounds, `created`'s `never` prohibition, JSON types. Name-content
/// validation (does this stage/tag/member actually exist) is the
/// resolver's job (`crm-api`'s `operator::filter`, step 2), never this
/// parser's: an unknown name is a clarification, not `invalid_arguments`.
fn parse_people_filter_spec(
    object: &Map<String, Value>,
) -> Result<PeopleFilterSpec, ArgumentError> {
    let stage_names = parse_string_array(object.get("stage_names"), "stage_names")?;
    let assignees = parse_string_array(object.get("assignees"), "assignees")?;
    let sources = parse_string_array(object.get("sources"), "sources")?;
    let tag_names_any = parse_string_array(object.get("tag_names_any"), "tag_names_any")?;
    let tag_names_none = parse_string_array(object.get("tag_names_none"), "tag_names_none")?;
    let created = parse_age_condition(object.get("created"), "created", false)?;
    let last_inquiry = parse_age_condition(object.get("last_inquiry"), "last_inquiry", true)?;
    let last_contact = parse_age_condition(object.get("last_contact"), "last_contact", true)?;
    let last_inbound = parse_age_condition(object.get("last_inbound"), "last_inbound", true)?;
    let has_replied = parse_bool(object.get("has_replied"), "has_replied")?;
    let has_phone = parse_bool(object.get("has_phone"), "has_phone")?;
    let has_email = parse_bool(object.get("has_email"), "has_email")?;
    let awaiting_response = parse_bool(object.get("awaiting_response"), "awaiting_response")?;
    let client_replied_unanswered = parse_bool(
        object.get("client_replied_unanswered"),
        "client_replied_unanswered",
    )?;
    let awaiting_call_outcome =
        parse_bool(object.get("awaiting_call_outcome"), "awaiting_call_outcome")?;
    let limit = parse_limit_with_default(
        object.get("limit"),
        FILTER_LIMIT_MIN,
        FILTER_LIMIT_MAX,
        FILTER_LIMIT_DEFAULT,
    )?;

    let has_condition = !stage_names.is_empty()
        || !assignees.is_empty()
        || !sources.is_empty()
        || !tag_names_any.is_empty()
        || !tag_names_none.is_empty()
        || created.is_some()
        || last_inquiry.is_some()
        || last_contact.is_some()
        || last_inbound.is_some()
        || has_replied.is_some()
        || has_phone.is_some()
        || has_email.is_some()
        || awaiting_response.is_some()
        || client_replied_unanswered.is_some()
        || awaiting_call_outcome.is_some();
    if !has_condition {
        return Err(ArgumentError::EmptyFilterSpec);
    }

    Ok(PeopleFilterSpec {
        stage_names,
        assignees,
        sources,
        tag_names_any,
        tag_names_none,
        created,
        last_inquiry,
        last_contact,
        last_inbound,
        has_replied,
        has_phone,
        has_email,
        awaiting_response,
        client_replied_unanswered,
        awaiting_call_outcome,
        limit,
    })
}

/// `run_saved_list`'s full body (docs/specs/SLICE_013.md §2): exactly one
/// of `name`/`list_id`. `name` is trimmed and control/invisible-stripped
/// like `search_people.query`, but — unlike `query` — clipping to
/// [`SAVED_LIST_NAME_MAX_CHARS`] rather than erroring, since an over-long
/// name simply will not resolve to any visible list (the resolver's job,
/// step 2), the same "clamped server-side as well" precedent as `limit`.
fn parse_saved_list_selector(
    object: &Map<String, Value>,
) -> Result<SavedListSelector, ArgumentError> {
    let name = match object.get("name") {
        None | Some(Value::Null) => None,
        Some(Value::String(raw)) => {
            let cleaned: String = raw
                .chars()
                .filter(|c| !c.is_control() && !crate::views::is_invisible_format(*c))
                .collect::<String>()
                .trim()
                .chars()
                .take(SAVED_LIST_NAME_MAX_CHARS)
                .collect();
            if cleaned.is_empty() {
                None
            } else {
                Some(cleaned)
            }
        }
        Some(_) => return Err(ArgumentError::WrongType("name")),
    };
    let list_id = match object.get("list_id") {
        None | Some(Value::Null) => None,
        some => Some(parse_uuid_named(some, "list_id")?),
    };
    match (&name, &list_id) {
        (None, None) => return Err(ArgumentError::MissingOneOf("name", "list_id")),
        (Some(_), Some(_)) => return Err(ArgumentError::ConflictingProperties("name", "list_id")),
        _ => {}
    }
    let limit = parse_limit_with_default(
        object.get("limit"),
        FILTER_LIMIT_MIN,
        FILTER_LIMIT_MAX,
        FILTER_LIMIT_DEFAULT,
    )?;
    Ok(SavedListSelector {
        name,
        list_id,
        limit,
    })
}

/// A `string[1..50]` array property (docs/specs/SLICE_013.md §2): absent
/// or `null` means "not supplied" (an empty `Vec`, per `PeopleFilterSpec`'s
/// doc comment); present-but-empty and present-and-too-long are both
/// `invalid_arguments`, matching the schema's `minItems`/`maxItems`.
fn parse_string_array(
    value: Option<&Value>,
    name: &'static str,
) -> Result<Vec<String>, ArgumentError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    if value.is_null() {
        return Ok(Vec::new());
    }
    let arr = value.as_array().ok_or(ArgumentError::WrongType(name))?;
    if arr.is_empty() {
        return Err(ArgumentError::EmptyArray(name));
    }
    if arr.len() > FILTER_ARRAY_MAX {
        return Err(ArgumentError::TooManyValues(name));
    }
    arr.iter()
        .map(|v| {
            v.as_str()
                .map(str::to_string)
                .ok_or(ArgumentError::WrongType(name))
        })
        .collect()
}

fn parse_bool(value: Option<&Value>, name: &'static str) -> Result<Option<bool>, ArgumentError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(ArgumentError::WrongType(name)),
    }
}

/// `{op: within_days|not_within_days, days: 1..3650}` or, when
/// `allow_never`, also bare `{op: never}` (docs/specs/SLICE_013.md §2).
/// `created` calls this with `allow_never: false` (§1 rule 5's mirror of
/// docs/specs/SLICE_011a.md §4b) so `created.never` is `InvalidOp`, never
/// silently accepted.
fn parse_age_condition(
    value: Option<&Value>,
    name: &'static str,
    allow_never: bool,
) -> Result<Option<AgeCondition>, ArgumentError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let obj = value.as_object().ok_or(ArgumentError::WrongType(name))?;
    let op = obj
        .get("op")
        .and_then(Value::as_str)
        .ok_or(ArgumentError::WrongType(name))?;
    match op {
        "within_days" | "not_within_days" => {
            let days = obj
                .get("days")
                .and_then(Value::as_i64)
                .ok_or(ArgumentError::WrongType(name))?;
            if !(FILTER_MIN_DAYS..=FILTER_MAX_DAYS).contains(&days) {
                return Err(ArgumentError::DaysOutOfRange(name));
            }
            let days = days as u32;
            Ok(Some(if op == "within_days" {
                AgeCondition::WithinDays(days)
            } else {
                AgeCondition::NotWithinDays(days)
            }))
        }
        "never" if allow_never => Ok(Some(AgeCondition::Never)),
        _ => Err(ArgumentError::InvalidOp(name)),
    }
}

fn parse_limit(value: Option<&Value>, min: u64, max: u64) -> Result<usize, ArgumentError> {
    parse_limit_with_default(value, min, max, max)
}

/// `parse_limit` with an independent default (docs/specs/SLICE_013.md §1
/// rule 4): `filter_people`/`run_saved_list` default an absent `limit` to
/// 10, not `max` — `search_people`/`get_today` keep defaulting to `max` via
/// `parse_limit` above, unchanged.
fn parse_limit_with_default(
    value: Option<&Value>,
    min: u64,
    max: u64,
    default: u64,
) -> Result<usize, ArgumentError> {
    let Some(value) = value else {
        return Ok(default as usize);
    };
    if value.is_null() {
        return Ok(default as usize);
    }
    let n = value
        .as_u64()
        .or_else(|| value.as_i64().map(|i| i.max(0) as u64))
        .or_else(|| {
            value
                .as_f64()
                .filter(|f| f.fract() == 0.0 && *f >= 0.0)
                .map(|f| f as u64)
        })
        .ok_or(ArgumentError::WrongType("limit"))?;
    Ok(n.clamp(min, max) as usize)
}

fn parse_uuid(value: Option<&Value>) -> Result<Uuid, ArgumentError> {
    parse_uuid_named(value, "person_id")
}

fn parse_uuid_named(value: Option<&Value>, name: &'static str) -> Result<Uuid, ArgumentError> {
    let raw = value
        .ok_or(ArgumentError::MissingProperty(name))?
        .as_str()
        .ok_or(ArgumentError::WrongType(name))?;
    Uuid::parse_str(raw.trim()).map_err(|_| ArgumentError::InvalidUuid(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The JSON schemas are a shared contract (docs/specs/SLICE_005.md §13
    /// item 1; §14 item 7): a changed snapshot is a declared change.
    #[test]
    fn tool_definitions_snapshot() {
        let actual = serde_json::to_string_pretty(&tool_definitions()).unwrap();
        let expected = include_str!("../tests/snapshots/tool_definitions.json");
        assert_eq!(
            actual.trim(),
            expected.trim(),
            "tool_definitions() changed; update tests/snapshots/tool_definitions.json deliberately"
        );
    }

    #[test]
    fn every_schema_forbids_additional_properties_and_has_no_trusted_ids() {
        // `person_id` (search_people/get_person/explain_priority/start_call)
        // and `contact_method_id` (start_call) are declared id-shaped
        // properties too, but they are not in the forbidden list below: a
        // *trusted* id (an Organization, user, or viewer id the model would
        // have to invent, AGENTS §5.2) is what this test forbids, not every
        // id — `person_id`/`contact_method_id` name a resource the model
        // was already handed by a previous tool result, and the backend
        // re-validates it through the caller's own scope on every use.
        // `run_saved_list`'s `list_id` (docs/specs/SLICE_013.md §2, §4)
        // joins that same declared, re-validated category, not this one:
        // it exists only for the duplicate-name clarification, and the
        // adapter re-validates it through the visibility predicate exactly
        // like `person_id`, so this test's forbidden list is unchanged.
        for def in tool_definitions() {
            assert_eq!(
                def.parameters["additionalProperties"], false,
                "{}",
                def.name
            );
            let props = def.parameters["properties"].as_object().unwrap();
            for forbidden in ["organization_id", "user_id", "actor_user_id", "viewer"] {
                assert!(!props.contains_key(forbidden), "{}: {forbidden}", def.name);
            }
        }
    }

    #[test]
    fn search_people_parses_and_clamps() {
        let inv = parse_invocation(SEARCH_PEOPLE, r#"{"query":"  Grace ","limit":99}"#).unwrap();
        assert_eq!(
            inv,
            ToolInvocation::SearchPeople {
                query: "Grace".into(),
                limit: 10
            }
        );
        let inv = parse_invocation(SEARCH_PEOPLE, r#"{"query":"g","limit":0}"#).unwrap();
        assert_eq!(
            inv,
            ToolInvocation::SearchPeople {
                query: "g".into(),
                limit: 1
            }
        );
        let inv = parse_invocation(SEARCH_PEOPLE, r#"{"query":"g"}"#).unwrap();
        assert!(matches!(
            inv,
            ToolInvocation::SearchPeople { limit: 10, .. }
        ));
    }

    #[test]
    fn search_query_is_clipped_to_100_chars() {
        let long = "x".repeat(150);
        let inv = parse_invocation(SEARCH_PEOPLE, &json!({ "query": long }).to_string()).unwrap();
        match inv {
            ToolInvocation::SearchPeople { query, .. } => assert_eq!(query.len(), 100),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn rejects_unknown_tool_non_json_and_extra_properties() {
        assert_eq!(
            parse_invocation("delete_person", "{}"),
            Err(ArgumentError::UnknownTool)
        );
        assert_eq!(
            parse_invocation(GET_TODAY, "not json"),
            Err(ArgumentError::NotJson)
        );
        assert_eq!(
            parse_invocation(GET_TODAY, "[1]"),
            Err(ArgumentError::NotAnObject)
        );
        assert_eq!(
            parse_invocation(
                GET_TODAY,
                r#"{"organization_id":"00000000-0000-0000-0000-000000000000"}"#
            ),
            Err(ArgumentError::UnknownProperty("organization_id".into()))
        );
        assert_eq!(
            parse_invocation(SEARCH_PEOPLE, r#"{"limit":3}"#),
            Err(ArgumentError::MissingProperty("query"))
        );
        assert_eq!(
            parse_invocation(SEARCH_PEOPLE, r#"{"query":"   "}"#),
            Err(ArgumentError::EmptyQuery)
        );
        assert_eq!(
            parse_invocation(SEARCH_PEOPLE, r#"{"query":5}"#),
            Err(ArgumentError::WrongType("query"))
        );
        assert_eq!(
            parse_invocation(SEARCH_PEOPLE, r#"{"query":"a","limit":"ten"}"#),
            Err(ArgumentError::WrongType("limit"))
        );
    }

    #[test]
    fn search_query_strips_control_and_invisible_chars() {
        let inv = parse_invocation(SEARCH_PEOPLE, r#"{"query":"a\u0000b\u200b\u202ec"}"#).unwrap();
        assert_eq!(
            inv,
            ToolInvocation::SearchPeople {
                query: "abc".into(),
                limit: 10
            }
        );
        assert_eq!(
            parse_invocation(SEARCH_PEOPLE, r#"{"query":"\u0000\t"}"#),
            Err(ArgumentError::EmptyQuery)
        );
    }

    #[test]
    fn unknown_property_name_is_clipped() {
        let key = "k".repeat(500);
        let err = parse_invocation(GET_TODAY, &json!({ key: 1 }).to_string()).unwrap_err();
        assert_eq!(err, ArgumentError::UnknownProperty("k".repeat(32)));
    }

    #[test]
    fn person_id_tools_require_a_uuid() {
        let id = Uuid::new_v4();
        assert_eq!(
            parse_invocation(GET_PERSON, &json!({ "person_id": id }).to_string()),
            Ok(ToolInvocation::GetPerson { person_id: id })
        );
        assert_eq!(
            parse_invocation(EXPLAIN_PRIORITY, &json!({ "person_id": id }).to_string()),
            Ok(ToolInvocation::ExplainPriority { person_id: id })
        );
        assert_eq!(
            parse_invocation(GET_PERSON, r#"{"person_id":"grace"}"#),
            Err(ArgumentError::InvalidUuid("person_id"))
        );
        assert_eq!(
            parse_invocation(GET_PERSON, "{}"),
            Err(ArgumentError::MissingProperty("person_id"))
        );
    }

    #[test]
    fn empty_arguments_are_an_empty_object() {
        assert_eq!(
            parse_invocation(GET_NEXT_WORK_ITEM, ""),
            Ok(ToolInvocation::GetNextWorkItem)
        );
        assert_eq!(
            parse_invocation(GET_TODAY, ""),
            Ok(ToolInvocation::GetToday { limit: 20 })
        );
        assert_eq!(
            parse_invocation(GET_TODAY, r#"{"limit": 5}"#),
            Ok(ToolInvocation::GetToday { limit: 5 })
        );
    }

    // --- filter_people / run_saved_list (docs/specs/SLICE_013.md §8.1-8.3)

    #[test]
    fn filter_people_rejects_an_empty_spec() {
        assert_eq!(
            parse_invocation(FILTER_PEOPLE, "{}"),
            Err(ArgumentError::EmptyFilterSpec)
        );
        // Booleans explicitly set to a value still count as a condition
        // even though the JSON is otherwise minimal.
        assert!(parse_invocation(FILTER_PEOPLE, r#"{"has_phone": true}"#).is_ok());
    }

    #[test]
    fn filter_people_rejects_more_than_fifty_values() {
        let too_many: Vec<String> = (0..51).map(|i| format!("stage{i}")).collect();
        let args = json!({ "stage_names": too_many }).to_string();
        assert_eq!(
            parse_invocation(FILTER_PEOPLE, &args),
            Err(ArgumentError::TooManyValues("stage_names"))
        );
        // Exactly fifty is fine.
        let fifty: Vec<String> = (0..50).map(|i| format!("stage{i}")).collect();
        let args = json!({ "stage_names": fifty }).to_string();
        assert!(parse_invocation(FILTER_PEOPLE, &args).is_ok());
    }

    #[test]
    fn filter_people_rejects_an_empty_array_for_a_present_property() {
        let args = json!({ "tag_names_any": [] }).to_string();
        assert_eq!(
            parse_invocation(FILTER_PEOPLE, &args),
            Err(ArgumentError::EmptyArray("tag_names_any"))
        );
    }

    #[test]
    fn filter_people_rejects_wrong_types() {
        assert_eq!(
            parse_invocation(FILTER_PEOPLE, r#"{"stage_names":"Lead"}"#),
            Err(ArgumentError::WrongType("stage_names"))
        );
        assert_eq!(
            parse_invocation(FILTER_PEOPLE, r#"{"stage_names":[1,2]}"#),
            Err(ArgumentError::WrongType("stage_names"))
        );
        assert_eq!(
            parse_invocation(FILTER_PEOPLE, r#"{"has_phone":"yes"}"#),
            Err(ArgumentError::WrongType("has_phone"))
        );
        assert_eq!(
            parse_invocation(FILTER_PEOPLE, r#"{"created":"within_days"}"#),
            Err(ArgumentError::WrongType("created"))
        );
        assert_eq!(
            parse_invocation(
                FILTER_PEOPLE,
                r#"{"created":{"op":"within_days","days":"5"}}"#
            ),
            Err(ArgumentError::WrongType("created"))
        );
    }

    #[test]
    fn filter_people_rejects_created_never() {
        assert_eq!(
            parse_invocation(FILTER_PEOPLE, r#"{"created":{"op":"never"}}"#),
            Err(ArgumentError::InvalidOp("created"))
        );
        // The same op is fine on the other three age axes.
        for field in ["last_inquiry", "last_contact", "last_inbound"] {
            let args = json!({ field: { "op": "never" } }).to_string();
            assert!(
                parse_invocation(FILTER_PEOPLE, &args).is_ok(),
                "{field} should allow never"
            );
        }
    }

    #[test]
    fn filter_people_rejects_days_outside_1_3650() {
        assert_eq!(
            parse_invocation(
                FILTER_PEOPLE,
                r#"{"created":{"op":"within_days","days":0}}"#
            ),
            Err(ArgumentError::DaysOutOfRange("created"))
        );
        assert_eq!(
            parse_invocation(
                FILTER_PEOPLE,
                r#"{"created":{"op":"within_days","days":3651}}"#
            ),
            Err(ArgumentError::DaysOutOfRange("created"))
        );
        assert!(parse_invocation(
            FILTER_PEOPLE,
            r#"{"created":{"op":"within_days","days":1}}"#
        )
        .is_ok());
        assert!(parse_invocation(
            FILTER_PEOPLE,
            r#"{"created":{"op":"within_days","days":3650}}"#
        )
        .is_ok());
    }

    #[test]
    fn filter_people_limit_clamps_to_1_25_and_defaults_to_10() {
        let spec = |args: &str| match parse_invocation(FILTER_PEOPLE, args).unwrap() {
            ToolInvocation::FilterPeople { spec } => spec,
            other => panic!("{other:?}"),
        };
        assert_eq!(spec(r#"{"has_phone":true}"#).limit, 10);
        assert_eq!(spec(r#"{"has_phone":true,"limit":0}"#).limit, 1);
        assert_eq!(spec(r#"{"has_phone":true,"limit":999}"#).limit, 25);
        assert_eq!(spec(r#"{"has_phone":true,"limit":25}"#).limit, 25);
        assert_eq!(spec(r#"{"has_phone":true,"limit":1}"#).limit, 1);
    }

    #[test]
    fn filter_people_unknown_property_and_extra_age_field_are_rejected() {
        assert_eq!(
            parse_invocation(FILTER_PEOPLE, r#"{"stage_id":["x"]}"#),
            Err(ArgumentError::UnknownProperty("stage_id".into()))
        );
    }

    #[test]
    fn run_saved_list_requires_exactly_one_of_name_or_list_id() {
        assert_eq!(
            parse_invocation(RUN_SAVED_LIST, "{}"),
            Err(ArgumentError::MissingOneOf("name", "list_id"))
        );
        let id = Uuid::new_v4();
        let both = json!({ "name": "Stale Zillow", "list_id": id }).to_string();
        assert_eq!(
            parse_invocation(RUN_SAVED_LIST, &both),
            Err(ArgumentError::ConflictingProperties("name", "list_id"))
        );
        assert!(parse_invocation(RUN_SAVED_LIST, r#"{"name":"Stale Zillow"}"#).is_ok());
        let only_id = json!({ "list_id": id }).to_string();
        assert!(parse_invocation(RUN_SAVED_LIST, &only_id).is_ok());
    }

    #[test]
    fn run_saved_list_limit_clamps_to_1_25_and_defaults_to_10() {
        let selector = |args: &str| match parse_invocation(RUN_SAVED_LIST, args).unwrap() {
            ToolInvocation::RunSavedList { selector } => selector,
            other => panic!("{other:?}"),
        };
        assert_eq!(selector(r#"{"name":"x"}"#).limit, 10);
        assert_eq!(selector(r#"{"name":"x","limit":0}"#).limit, 1);
        assert_eq!(selector(r#"{"name":"x","limit":999}"#).limit, 25);
    }

    #[test]
    fn run_saved_list_name_is_stripped_and_clipped() {
        let long = "z".repeat(200);
        let args = json!({ "name": long }).to_string();
        match parse_invocation(RUN_SAVED_LIST, &args).unwrap() {
            ToolInvocation::RunSavedList { selector } => {
                assert_eq!(
                    selector.name.unwrap().chars().count(),
                    SAVED_LIST_NAME_MAX_CHARS
                );
            }
            other => panic!("{other:?}"),
        }
    }

    /// docs/specs/SLICE_013.md §2: the fifteen `Clause` kinds this table
    /// mirrors. Kept here (rather than only in the crm-api-side coverage
    /// test that walks the real `Clause::kind_label()` values, which this
    /// crate cannot depend on per D-034) so a missing field in
    /// `known_properties`/`parse_people_filter_spec` fails fast in this
    /// crate too.
    const MIRRORED_CLAUSE_FIELDS: &[&str] = &[
        "stage_names",
        "assignees",
        "sources",
        "created",
        "last_inquiry",
        "last_contact",
        "last_inbound",
        "has_replied",
        "has_phone",
        "has_email",
        "awaiting_response",
        "client_replied_unanswered",
        "awaiting_call_outcome",
        "tag_names_any",
        "tag_names_none",
    ];

    #[test]
    fn filter_people_schema_declares_a_field_for_every_mirrored_clause() {
        let def = tool_definitions()
            .into_iter()
            .find(|d| d.name == FILTER_PEOPLE)
            .unwrap();
        let props = def.parameters["properties"].as_object().unwrap();
        for field in MIRRORED_CLAUSE_FIELDS {
            assert!(props.contains_key(*field), "missing schema field: {field}");
            assert!(
                known_properties(FILTER_PEOPLE).unwrap().contains(field),
                "missing known_properties entry: {field}"
            );
        }
    }
}
