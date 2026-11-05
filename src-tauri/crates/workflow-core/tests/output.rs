use serde_json::{json, Value};
use workflow_core::output::{evaluate, parse_output, AgentOutput, ConditionError};
use workflow_core::{AgentRole, CompareOp, ConditionConfig, FieldRef, FieldType};

fn analysis() -> Value {
    json!({"summary": "s", "plan": ["a"], "acceptanceCriteria": ["k"], "risks": ["r1", "r2"]})
}
fn implementation() -> Value {
    json!({"summary": "s", "changedFiles": ["a.rs", "b.rs", "c.rs"],
           "checks": [{"name": "cargo test", "passed": true}]})
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
