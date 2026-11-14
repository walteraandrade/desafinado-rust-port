use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use thiserror::Error;

pub const SUGGESTION_REPLACEMENT_DESCRIPTION: &str =
    "The new text that replaces the Target, in Markdown, in the language of the Target.";
pub const REVIEW_REPLACEMENT_DESCRIPTION: &str =
    "The complete new text of that Block, in Markdown, in the language of that Block.";
pub const RATIONALE_DESCRIPTION: &str =
    "One short sentence explaining the change, in the language of the Document.";
pub const BLOCK_ID_DESCRIPTION: &str = "The id of the Block to replace, such as \"p3\".";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuggestionOutput {
    pub replacement: String,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewEdit {
    pub block_id: String,
    pub replacement: String,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SchemaError {
    #[error("expected an object")]
    NotAnObject,
    #[error("missing or invalid field `{0}`")]
    Field(&'static str),
}

fn object(value: &Value) -> Result<&Map<String, Value>, SchemaError> {
    value.as_object().ok_or(SchemaError::NotAnObject)
}

fn string_field(map: &Map<String, Value>, key: &'static str) -> Result<String, SchemaError> {
    map.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or(SchemaError::Field(key))
}

pub fn validate_suggestion(value: &Value) -> Result<SuggestionOutput, SchemaError> {
    let map = object(value)?;
    Ok(SuggestionOutput {
        replacement: string_field(map, "replacement")?,
        rationale: string_field(map, "rationale")?,
    })
}

pub fn validate_review_edit(value: &Value) -> Result<ReviewEdit, SchemaError> {
    let map = object(value)?;
    Ok(ReviewEdit {
        block_id: string_field(map, "blockId")?,
        replacement: string_field(map, "replacement")?,
        rationale: string_field(map, "rationale")?,
    })
}

pub fn parse_suggestion(text: &str) -> Option<SuggestionOutput> {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| validate_suggestion(&value).ok())
}

pub fn suggestion_json_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "replacement": { "type": "string", "description": SUGGESTION_REPLACEMENT_DESCRIPTION },
            "rationale": { "type": "string", "description": RATIONALE_DESCRIPTION }
        },
        "required": ["replacement", "rationale"],
        "additionalProperties": false
    })
}

pub fn review_edit_json_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "blockId": { "type": "string", "description": BLOCK_ID_DESCRIPTION },
            "replacement": { "type": "string", "description": REVIEW_REPLACEMENT_DESCRIPTION },
            "rationale": { "type": "string", "description": RATIONALE_DESCRIPTION }
        },
        "required": ["blockId", "replacement", "rationale"],
        "additionalProperties": false
    })
}

pub fn review_array_json_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "elements": { "type": "array", "items": review_edit_json_schema() }
        },
        "required": ["elements"],
        "additionalProperties": false
    })
}

fn response_format(schema: Value) -> Value {
    json!({
        "type": "json_schema",
        "json_schema": { "name": "response", "strict": true, "schema": schema }
    })
}

pub fn suggestion_response_format() -> Value {
    response_format(suggestion_json_schema())
}

pub fn review_response_format() -> Value {
    response_format(review_array_json_schema())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn strict_object_shape(schema: &Value, keys: &[&str]) {
        assert_eq!(schema["type"], "object");
        let mut properties: Vec<&str> = schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        properties.sort_unstable();
        let mut expected = keys.to_vec();
        expected.sort_unstable();
        assert_eq!(properties, expected);
        let mut required: Vec<&str> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        required.sort_unstable();
        assert_eq!(required, expected);
        assert_eq!(schema["additionalProperties"], false);
        for key in keys {
            assert_eq!(schema["properties"][key]["type"], "string");
        }
    }

    fn description(schema: &Value, key: &str) -> String {
        schema["properties"][key]["description"]
            .as_str()
            .unwrap_or("")
            .to_owned()
    }

    #[test]
    fn suggestion_accepts_a_replacement_with_a_rationale() {
        assert!(
            validate_suggestion(&json!({"replacement": "Hi", "rationale": "Shorter."})).is_ok()
        );
    }

    #[test]
    fn suggestion_rejects_a_missing_rationale() {
        assert!(validate_suggestion(&json!({"replacement": "Hi"})).is_err());
    }

    #[test]
    fn suggestion_rejects_a_non_string_replacement() {
        assert!(validate_suggestion(&json!({"replacement": 1, "rationale": "x"})).is_err());
    }

    #[test]
    fn suggestion_rejects_arrays_and_ignores_extra_keys() {
        assert!(validate_suggestion(&json!(["Hi", "x"])).is_err());
        assert!(
            validate_suggestion(&json!({"replacement": "a", "rationale": "b", "extra": 1})).is_ok()
        );
    }

    #[test]
    fn suggestion_produces_a_strict_compatible_json_schema() {
        strict_object_shape(&suggestion_json_schema(), &["replacement", "rationale"]);
    }

    #[test]
    fn review_edit_accepts_a_complete_edit() {
        assert!(
            validate_review_edit(
                &json!({"blockId": "p3", "replacement": "New", "rationale": "Clearer."})
            )
            .is_ok()
        );
    }

    #[test]
    fn review_edit_rejects_an_edit_without_a_block_id() {
        assert!(validate_review_edit(&json!({"replacement": "New", "rationale": "x"})).is_err());
    }

    #[test]
    fn review_edit_produces_a_strict_compatible_json_schema() {
        strict_object_shape(
            &review_edit_json_schema(),
            &["blockId", "replacement", "rationale"],
        );
    }

    #[test]
    fn language_anchors() {
        let suggestion = suggestion_json_schema();
        let review = review_edit_json_schema();
        assert!(description(&suggestion, "replacement").contains("language of the Target"));
        assert!(description(&suggestion, "rationale").contains("language of the Document"));
        assert!(description(&review, "replacement").contains("language of that Block"));
        assert!(description(&review, "rationale").contains("language of the Document"));
    }

    #[test]
    fn response_format_wraps_the_schema_for_open_router() {
        let format = review_response_format();
        assert_eq!(format["type"], "json_schema");
        assert_eq!(format["json_schema"]["strict"], true);
        assert_eq!(
            format["json_schema"]["schema"]["properties"]["elements"]["type"],
            "array"
        );
    }
}
