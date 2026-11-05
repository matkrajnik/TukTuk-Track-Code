use serde_json::{json, Value};
use workflow_core::output::{
    evaluate, json_schema, parse_output, AgentOutput, ConditionError, MAX_OUTPUT_ITEMS,
    MAX_OUTPUT_TEXT_CHARS,
};
use workflow_core::{AgentRole, CompareOp, ConditionConfig, FieldRef, FieldType};

fn analysis() -> Value {
    json!({"summary": "s", "plan": ["a"], "acceptanceCriteria": ["k"], "risks": ["r1", "r2"]})
}
fn implementation() -> Value {
    json!({"summary": "s", "changedFiles": ["a.rs", "b.rs", "c.rs"],
           "checks": [{"name": "cargo test", "passed": true, "logRef": null}]})
}
fn review(verdict: &str) -> Value {
    json!({"verdict": verdict, "findings": [{"file": "a.rs", "location": "l1",
           "severity": "high", "description": "d"}], "checkedArtifactHash": "h"})
}

fn cond(op: CompareOp, value: Value) -> ConditionConfig {
    ConditionConfig {
        field: FieldRef {
            node_id: "n".into(),
            field: "f".into(),
        },
        op,
        value,
        repair_limit: None,
    }
}

#[test]
fn correct_output_parses_for_each_role() {
    assert!(matches!(
        parse_output(AgentRole::Analysis, &analysis()),
        Ok(AgentOutput::Analysis(_))
    ));
    assert!(matches!(
        parse_output(AgentRole::Implementation, &implementation()),
        Ok(AgentOutput::Implementation(_))
    ));
    assert!(matches!(
        parse_output(AgentRole::Fix, &implementation()),
        Ok(AgentOutput::Implementation(_))
    ));
    assert!(matches!(
        parse_output(AgentRole::Review, &review("approved")),
        Ok(AgentOutput::Review(_))
    ));
}

#[test]
fn missing_required_field_is_invalid() {
    let mut value = analysis();
    value.as_object_mut().unwrap().remove("risks");
    assert!(parse_output(AgentRole::Analysis, &value).is_err());
    assert!(parse_output(AgentRole::Review, &Value::Null).is_err());
}

#[test]
fn unknown_verdict_is_rejected() {
    assert!(parse_output(AgentRole::Review, &review("ok")).is_err());
}

#[test]
fn over_the_limit_is_rejected() {
    let mut value = analysis();
    value["risks"] = json!(vec!["x"; 201]);
    assert!(parse_output(AgentRole::Analysis, &value).is_err());
    let mut value = analysis();
    value["summary"] = json!("x".repeat(20_001));
    assert!(parse_output(AgentRole::Analysis, &value).is_err());
}

#[test]
fn fields_are_derived_from_the_output() {
    let a = parse_output(AgentRole::Analysis, &analysis()).unwrap();
    assert_eq!(a.field("riskCount"), Some(json!(2)));
    let i = parse_output(AgentRole::Implementation, &implementation()).unwrap();
    assert_eq!(i.field("changedFileCount"), Some(json!(3)));
    let r = parse_output(AgentRole::Review, &review("changes_requested")).unwrap();
    assert_eq!(r.field("verdict"), Some(json!("changes_requested")));
    assert_eq!(r.field("findingCount"), Some(json!(1)));
    assert_eq!(r.field("riskCount"), None);
}

#[test]
fn field_lists_match_agent_role_output_fields() {
    let samples = [
        (AgentRole::Analysis, analysis()),
        (AgentRole::Implementation, implementation()),
        (AgentRole::Fix, implementation()),
        (AgentRole::Review, review("approved")),
    ];
    for (role, value) in samples {
        let output = parse_output(role, &value).unwrap();
        for (name, kind) in role.output_fields() {
            let field = output
                .field(name)
                .unwrap_or_else(|| panic!("{role:?}.{name}"));
            match kind {
                FieldType::Number => assert!(field.is_number()),
                FieldType::Text | FieldType::Choice(_) => assert!(field.is_string()),
                FieldType::Boolean => assert!(field.is_boolean()),
            }
        }
        for name in [
            "summary",
            "riskCount",
            "changedFileCount",
            "verdict",
            "findingCount",
        ] {
            assert_eq!(
                output.field(name).is_some(),
                role.output_field(name).is_some(),
                "{role:?}.{name}"
            );
        }
    }
}

#[test]
fn evaluate_compares_typed_values() {
    let choice = cond(CompareOp::Equals, json!("approved"));
    assert_eq!(evaluate(&choice, Some(&json!("approved"))), Ok(true));
    assert_eq!(
        evaluate(&choice, Some(&json!("changes_requested"))),
        Ok(false)
    );
    let gt = cond(CompareOp::GreaterThan, json!(1));
    assert_eq!(evaluate(&gt, Some(&json!(2))), Ok(true));
    assert_eq!(evaluate(&gt, Some(&json!(1))), Ok(false));
    let lt = cond(CompareOp::LessThan, json!(1));
    assert_eq!(evaluate(&lt, Some(&json!(0))), Ok(true));
    let ne = cond(CompareOp::NotEquals, json!(true));
    assert_eq!(evaluate(&ne, Some(&json!(false))), Ok(true));
}

#[test]
fn evaluate_errors_instead_of_defaulting() {
    let gt = cond(CompareOp::GreaterThan, json!(1));
    assert_eq!(
        evaluate(&gt, Some(&json!("text"))),
        Err(ConditionError::TypeMismatch)
    );
    assert_eq!(evaluate(&gt, None), Err(ConditionError::MissingValue));
    let eq = cond(CompareOp::Equals, json!("approved"));
    assert_eq!(
        evaluate(&eq, Some(&json!(1))),
        Err(ConditionError::TypeMismatch)
    );
    assert_eq!(evaluate(&eq, None), Err(ConditionError::MissingValue));
}

fn sample_for(role: AgentRole) -> Value {
    match role {
        AgentRole::Analysis => analysis(),
        AgentRole::Implementation | AgentRole::Fix => implementation(),
        AgentRole::Review => review("approved"),
    }
}

const ROLES: [AgentRole; 4] = [
    AgentRole::Analysis,
    AgentRole::Implementation,
    AgentRole::Fix,
    AgentRole::Review,
];

#[test]
fn schema_requires_every_field_of_the_sample() {
    for role in ROLES {
        let schema = json_schema(role);
        let required: Vec<&str> = schema["required"]
            .as_array()
            .expect("required")
            .iter()
            .map(|v| v.as_str().expect("str"))
            .collect();
        let sample = sample_for(role);
        for key in sample.as_object().expect("obiekt").keys() {
            assert!(required.contains(&key.as_str()), "{role:?}: {key}");
            assert!(schema["properties"].get(key).is_some(), "{role:?}: {key}");
        }
        assert_eq!(required.len(), sample.as_object().expect("obiekt").len());
    }
}

#[test]
fn schema_forbids_additional_properties_and_limits_sizes() {
    for role in ROLES {
        let schema = json_schema(role);
        assert_eq!(schema["additionalProperties"], json!(false), "{role:?}");
        assert_eq!(schema["type"], json!("object"));
    }
    let analysis = json_schema(AgentRole::Analysis);
    assert_eq!(
        analysis["properties"]["plan"]["maxItems"],
        json!(MAX_OUTPUT_ITEMS)
    );
    assert_eq!(
        analysis["properties"]["summary"]["maxLength"],
        json!(MAX_OUTPUT_TEXT_CHARS)
    );
}

#[test]
fn review_schema_verdict_has_exactly_two_values() {
    let schema = json_schema(AgentRole::Review);
    assert_eq!(
        schema["properties"]["verdict"]["enum"],
        json!(["approved", "changes_requested"])
    );
}

#[test]
fn fix_uses_the_implementation_schema() {
    assert_eq!(
        json_schema(AgentRole::Fix),
        json_schema(AgentRole::Implementation)
    );
}

/// Mały walidator podzbioru JSON Schema używanego przez `json_schema`: `type`
/// (także lista typów), `enum`, `required`, `additionalProperties: false`,
/// `properties`, `items`, `maxItems`, `maxLength`. Zwraca ścieżkę pierwszego błędu.
fn validate(schema: &Value, value: &Value, path: &str) -> Result<(), String> {
    let type_ok = |name: &str| match name {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "number" => value.is_number(),
        "integer" => value.is_i64() || value.is_u64(),
        "null" => value.is_null(),
        other => panic!("nieobsługiwany typ schematu {other}"),
    };
    match &schema["type"] {
        Value::String(t) if !type_ok(t) => return Err(format!("{path}: oczekiwano {t}")),
        Value::Array(ts) if !ts.iter().any(|t| type_ok(t.as_str().expect("typ"))) => {
            return Err(format!("{path}: oczekiwano jednego z {ts:?}"))
        }
        _ => {}
    }
    if let Some(allowed) = schema["enum"].as_array() {
        if !allowed.contains(value) {
            return Err(format!("{path}: wartość spoza enum"));
        }
    }
    if let (Some(max), Some(text)) = (schema["maxLength"].as_u64(), value.as_str()) {
        if text.chars().count() as u64 > max {
            return Err(format!("{path}: za długi tekst"));
        }
    }
    if let Some(items) = value.as_array() {
        if let Some(max) = schema["maxItems"].as_u64() {
            if items.len() as u64 > max {
                return Err(format!("{path}: za dużo elementów"));
            }
        }
        for (i, item) in items.iter().enumerate() {
            validate(&schema["items"], item, &format!("{path}[{i}]"))?;
        }
    }
    if let Some(object) = value.as_object() {
        let properties = schema["properties"].as_object().expect("properties");
        for name in schema["required"].as_array().expect("required") {
            let name = name.as_str().expect("nazwa");
            if !object.contains_key(name) {
                return Err(format!("{path}: brak wymaganego pola {name}"));
            }
        }
        for (key, item) in object {
            match properties.get(key) {
                Some(sub) => validate(sub, item, &format!("{path}.{key}"))?,
                None if schema["additionalProperties"] == json!(false) => {
                    return Err(format!("{path}: niedozwolone pole {key}"))
                }
                None => {}
            }
        }
    }
    Ok(())
}

fn engine_samples() -> Vec<(AgentRole, Value)> {
    use workflow_core::samples;
    vec![
        (AgentRole::Analysis, samples::analysis()),
        (AgentRole::Implementation, samples::implementation()),
        (AgentRole::Fix, samples::implementation()),
        (AgentRole::Review, samples::review_approved()),
        (AgentRole::Review, samples::review_changes()),
    ]
}

#[test]
fn engine_samples_satisfy_the_cli_schema_recursively_and_parse() {
    for (role, sample) in engine_samples() {
        validate(&json_schema(role), &sample, "$")
            .unwrap_or_else(|e| panic!("{role:?}: schemat odrzuca próbkę: {e}"));
        assert!(parse_output(role, &sample).is_ok(), "{role:?}");
    }
}

#[test]
fn schema_is_strict_at_the_nested_level_too() {
    let schema = json_schema(AgentRole::Implementation);
    // Tryb ścisły: logRef jest wymagany, ale może być null.
    let mut missing = workflow_core::samples::implementation();
    missing["checks"][0]
        .as_object_mut()
        .expect("check")
        .remove("logRef");
    assert!(validate(&schema, &missing, "$")
        .unwrap_err()
        .contains("logRef"));
    let mut text = workflow_core::samples::implementation();
    text["checks"][0]["logRef"] = json!("logs/test.txt");
    assert_eq!(validate(&schema, &text, "$"), Ok(()));
    let mut extra = workflow_core::samples::implementation();
    extra["checks"][0]["extra"] = json!(1);
    assert!(validate(&schema, &extra, "$").is_err());
    let mut finding = workflow_core::samples::review_changes();
    finding["findings"][0]["severity"] = json!(3);
    assert!(validate(&json_schema(AgentRole::Review), &finding, "$").is_err());
}
