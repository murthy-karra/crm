//! The five read-only tools (docs/specs/SLICE_005.md §3, §7): their
//! JSON-schema definitions (a snapshot-pinned shared contract) and the
//! validation of model-supplied arguments against them. No schema carries
//! an Organization or user id; every object is `additionalProperties:
//! false`, so an invented trusted id is `invalid_arguments`.

use serde_json::{json, Value};
use uuid::Uuid;

use crate::provider::ToolDefinition;

pub const SEARCH_QUERY_MAX_CHARS: usize = 100;
pub const SEARCH_LIMIT_MIN: u64 = 1;
pub const SEARCH_LIMIT_MAX: u64 = 10;
pub const TODAY_LIMIT_MIN: u64 = 1;
pub const TODAY_LIMIT_MAX: u64 = 20;

pub const SEARCH_PEOPLE: &str = "search_people";
pub const GET_PERSON: &str = "get_person";
pub const GET_TODAY: &str = "get_today";
pub const GET_NEXT_WORK_ITEM: &str = "get_next_work_item";
pub const EXPLAIN_PRIORITY: &str = "explain_priority";

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
            description: "Get one Person's card, contact methods, latest inquiries, recent history, and whether they are on the user's Today list. Use an id from a previous tool result.",
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
            description: "The user's Today list: People assigned to them who are waiting for a contact attempt, in the exact order the CRM shows them. Report the order as given; never reorder.",
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
            description: "The first item on the user's Today list (who to contact next) and the total number of items.",
            parameters: json!({
                "type": "object",
                "properties": {},
                "required": [],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: EXPLAIN_PRIORITY,
            description: "Why a Person is (or is not) on the user's Today list and at what position: priority tier, reasons, the ordering rule, and how many People are ahead in each tier.",
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
    ]
}

/// A validated, bounded tool invocation — the only thing the loop passes to
/// a `ToolBackend`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolInvocation {
    SearchPeople { query: String, limit: usize },
    GetPerson { person_id: Uuid },
    GetToday { limit: usize },
    GetNextWorkItem,
    ExplainPriority { person_id: Uuid },
}

impl ToolInvocation {
    pub fn tool_name(&self) -> &'static str {
        match self {
            ToolInvocation::SearchPeople { .. } => SEARCH_PEOPLE,
            ToolInvocation::GetPerson { .. } => GET_PERSON,
            ToolInvocation::GetToday { .. } => GET_TODAY,
            ToolInvocation::GetNextWorkItem => GET_NEXT_WORK_ITEM,
            ToolInvocation::ExplainPriority { .. } => EXPLAIN_PRIORITY,
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
        }
    }
}

fn known_properties(name: &str) -> Option<&'static [&'static str]> {
    match name {
        SEARCH_PEOPLE => Some(&["query", "limit"]),
        GET_PERSON | EXPLAIN_PRIORITY => Some(&["person_id"]),
        GET_TODAY => Some(&["limit"]),
        GET_NEXT_WORK_ITEM => Some(&[]),
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
        _ => Err(ArgumentError::UnknownTool),
    }
}

fn parse_limit(value: Option<&Value>, min: u64, max: u64) -> Result<usize, ArgumentError> {
    let Some(value) = value else {
        return Ok(max as usize);
    };
    if value.is_null() {
        return Ok(max as usize);
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
    let raw = value
        .ok_or(ArgumentError::MissingProperty("person_id"))?
        .as_str()
        .ok_or(ArgumentError::WrongType("person_id"))?;
    Uuid::parse_str(raw.trim()).map_err(|_| ArgumentError::InvalidUuid("person_id"))
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
}
