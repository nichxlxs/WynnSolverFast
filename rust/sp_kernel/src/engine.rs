use serde_json::{json, Value};

pub const SEARCH_SCHEMA_VERSION: u64 = 1;

fn error_result(code: &str, message: impl Into<String>, details: Value) -> Value {
    json!({
        "schema_version": SEARCH_SCHEMA_VERSION,
        "status": "error",
        "exhaustive": false,
        "error": {
            "code": code,
            "message": message.into(),
            "supported": [SEARCH_SCHEMA_VERSION],
            "received": details
        }
    })
}

pub fn solve_json(input: &str) -> String {
    let job: Value = match serde_json::from_str(input) {
        Ok(job) => job,
        Err(error) => {
            return error_result(
                "invalid_search_job",
                format!("SearchJob is not valid JSON: {error}"),
                Value::Null,
            )
            .to_string()
        }
    };

    let received = job.get("schema_version").and_then(Value::as_u64);
    if received != Some(SEARCH_SCHEMA_VERSION) {
        return error_result(
            "unsupported_schema_version",
            format!(
                "SearchJob schema version {} is not supported",
                received.map_or_else(|| "missing".to_owned(), |value| value.to_string())
            ),
            received.map(Value::from).unwrap_or(Value::Null),
        )
        .to_string();
    }

    let data_version = match job.get("data_version").and_then(Value::as_str) {
        Some(value) if !value.is_empty() => value,
        _ => {
            return error_result(
                "missing_data_version",
                "SearchJob.data_version must be a non-empty string",
                Value::Null,
            )
            .to_string()
        }
    };
    let enumeration_fixture = match job.get("enumeration_fixture").and_then(Value::as_str) {
        Some(value) if !value.is_empty() => value,
        _ => {
            return error_result(
                "missing_enumeration_fixture",
                "SearchJob.enumeration_fixture must be a non-empty string",
                Value::Null,
            )
            .to_string()
        }
    };
    let scoring_fixture = job.get("scoring_fixture").filter(|value| !value.is_null());

    let result = match crate::search_core::solve_single(enumeration_fixture, scoring_fixture) {
        Ok(result) => result,
        Err(message) => {
            return error_result("unsupported_search_job", message, Value::Null).to_string()
        }
    };
    let top_n: Vec<Value> = result
        .top_n
        .into_iter()
        .map(|(score, item_names)| json!({ "score": score, "item_names": item_names }))
        .collect();

    json!({
        "schema_version": SEARCH_SCHEMA_VERSION,
        "status": "completed",
        "data_version": data_version,
        "engine": {
            "name": "sp_kernel",
            "version": env!("CARGO_PKG_VERSION"),
            "target": if cfg!(target_arch = "wasm32") { "wasm32" } else { "native" }
        },
        "exhaustive": result.exhaustive,
        "counters": {
            "checked": result.checked,
            "precheck_reject": result.precheck_reject,
            "precheck_pass": result.precheck_pass,
            "sp_leaf_reject": result.sp_leaf_reject,
            "feasible": result.feasible,
            "scored": result.scored,
            "gated": result.gated,
            "mana_reject": result.mana_reject,
            "threshold_reject": result.threshold_reject,
            "bound_pruned": result.bound_pruned
        },
        "top_n": top_n
    })
    .to_string()
}
