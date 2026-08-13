use serde_json::{json, Value};

#[test]
fn search_job_rejects_unknown_schema_version() {
    let result: Value = serde_json::from_str(&sp_kernel::solve_json(
        &json!({
            "schema_version": 99,
            "data_version": "test-data",
            "enumeration_fixture": "",
            "scoring_fixture": {}
        })
        .to_string(),
    ))
    .expect("engine must always return a JSON SearchResult envelope");

    assert_eq!(result["schema_version"], 1);
    assert_eq!(result["status"], "error");
    assert_eq!(result["error"]["code"], "unsupported_schema_version");
    assert_eq!(result["error"]["supported"], json!([1]));
    assert_eq!(result["error"]["received"], 99);
}

#[test]
fn search_job_rejects_missing_data_version() {
    let result: Value = serde_json::from_str(&sp_kernel::solve_json(
        &json!({
            "schema_version": 1,
            "enumeration_fixture": include_str!("fixtures/oracle_armor2.enum.txt")
        })
        .to_string(),
    ))
    .expect("engine must always return a JSON SearchResult envelope");

    assert_eq!(result["status"], "error");
    assert_eq!(result["error"]["code"], "missing_data_version");
    assert_eq!(result["exhaustive"], false);
}

#[test]
fn search_job_returns_structured_exhaustive_counters() {
    let result: Value = serde_json::from_str(&sp_kernel::solve_json(
        &json!({
            "schema_version": 1,
            "data_version": "baseline-test",
            "enumeration_fixture": include_str!("fixtures/oracle_armor2.enum.txt")
        })
        .to_string(),
    ))
    .expect("engine must return a JSON SearchResult envelope");

    assert_eq!(result["status"], "completed");
    assert_eq!(result["data_version"], "baseline-test");
    assert_eq!(result["exhaustive"], true);
    assert_eq!(result["counters"]["checked"], 36.0);
    assert_eq!(result["counters"]["feasible"], 31);
    assert_eq!(result["counters"]["scored"], 0);
    assert_eq!(result["top_n"], json!([]));
}
