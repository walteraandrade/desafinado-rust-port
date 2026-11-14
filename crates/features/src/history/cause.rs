use serde_json::Value;

fn is_change(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    matches!(
        fields.get("origin").and_then(Value::as_str),
        Some("ask" | "review")
    ) && fields.get("instruction").is_some_and(Value::is_string)
        && fields.get("rationale").is_some_and(Value::is_string)
}

fn is_integer(value: &Value) -> bool {
    value.is_i64()
        || value.is_u64()
        || value
            .as_f64()
            .is_some_and(|number| number.fract() == 0.0 && number.is_finite())
}

pub fn is_cause(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    match fields.get("kind").and_then(Value::as_str) {
        Some("started" | "edited" | "external") => true,
        Some("accepted") => fields
            .get("changes")
            .and_then(Value::as_array)
            .is_some_and(|changes| changes.iter().all(is_change)),
        Some("opened") => fields.get("name").is_some_and(Value::is_string),
        Some("restored") => fields.get("from").is_some_and(is_integer),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn accepts_every_cause() {
        let causes = [
            json!({"kind": "started"}),
            json!({"kind": "edited"}),
            json!({"kind": "external"}),
            json!({"kind": "opened", "name": "notes.md"}),
            json!({"kind": "restored", "from": 3}),
            json!({"kind": "accepted", "changes": [{"origin": "ask", "instruction": "Shorter", "rationale": "Why."}]}),
        ];
        assert!(causes.iter().all(is_cause));
    }

    #[test]
    fn rejects_unknown_kinds_and_malformed_fields() {
        let causes = [
            json!(null),
            json!("edited"),
            json!({}),
            json!({"kind": "merged"}),
            json!({"kind": "opened"}),
            json!({"kind": "restored", "from": 1.5}),
            json!({"kind": "accepted", "changes": "none"}),
            json!({"kind": "accepted", "changes": [{"origin": "chat", "instruction": "x", "rationale": "y"}]}),
        ];
        assert!(!causes.iter().any(is_cause));
    }
}
