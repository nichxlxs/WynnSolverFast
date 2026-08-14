use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const CONSTRAINED_FIXTURE: &str = "BUDGET 0
PRECHECKS 1
PC raw 1 0
EHP 0 0 0 0
EHPNA 0 0 0 0
THP 0 0 0
HPSTART 0
WEAPON 0 0 0 0 0 0 0 0 0 0 -1 -1
GUILD 0
NFIXED 0
NSLOTS 0
NSETS 0
";

const FIVE_LEAF_FIXTURE: &str = "BUDGET 0
PRECHECKS 0
EHP 0 0 0 0
EHPNA 0 0 0 0
THP 0 0 0
HPSTART 0
WEAPON 0 0 0 0 0 0 0 0 0 0 -1 -1
GUILD 0
NFIXED 0
NSLOTS 1
SLOT helmet 0 0 0 5
ITEM 0 0 0 0 0 0 0 0 0 0 0 -1 -1 0
ITEM 0 0 0 0 0 0 0 0 0 0 0 -1 -1 0
ITEM 0 0 0 0 0 0 0 0 0 0 0 -1 -1 0
ITEM 0 0 0 0 0 0 0 0 0 0 0 -1 -1 0
ITEM 0 0 0 0 0 0 0 0 0 0 0 -1 -1 0
NSETS 0
";

fn temp_fixture(label: &str, contents: &str) -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "wynnsolver-{label}-{}-{nonce}.txt",
        std::process::id(),
    ));
    fs::write(&path, contents).expect("write temporary enum fixture");
    path
}

#[test]
fn cli_rejects_constrained_scoreless_fixture_but_allows_explicit_relaxed_run() {
    let path = temp_fixture("enum-cli", CONSTRAINED_FIXTURE);

    let rejected = Command::new(env!("CARGO_BIN_EXE_enum_kernel"))
        .arg(&path)
        .arg("1")
        .env_remove("ALLOW_RELAXED_CONSTRAINTS")
        .env_remove("UNSAFE_PRECHECKS")
        .output()
        .expect("run enum_kernel");
    assert_eq!(rejected.status.code(), Some(2));
    let rejected_stderr = String::from_utf8_lossy(&rejected.stderr);
    assert!(rejected_stderr.contains("refusing constrained enum-only run"));
    assert!(rejected_stderr.contains("score fixture"));

    let relaxed = Command::new(env!("CARGO_BIN_EXE_enum_kernel"))
        .arg(&path)
        .arg("1")
        .env("ALLOW_RELAXED_CONSTRAINTS", "1")
        .env_remove("UNSAFE_PRECHECKS")
        .output()
        .expect("run relaxed enum_kernel");
    assert!(relaxed.status.success());
    assert!(String::from_utf8_lossy(&relaxed.stderr).contains("throughput-only"));
    assert!(String::from_utf8_lossy(&relaxed.stdout).contains("feasible 1"));

    fs::remove_file(path).expect("remove temporary enum fixture");
}

#[test]
fn cli_leaf_budget_is_enforced_and_rejects_nonaggregate_multithread_use() {
    let path = temp_fixture("leaf-budget", FIVE_LEAF_FIXTURE);
    let limited = Command::new(env!("CARGO_BIN_EXE_enum_kernel"))
        .arg(&path)
        .arg("1")
        .env("ENUM_LEAF_BUDGET", "2")
        .output()
        .expect("run budgeted enum_kernel");
    assert!(limited.status.success());
    let limited_stdout = String::from_utf8_lossy(&limited.stdout);
    // The current geometric band contains offsets 1 and 2, so the kernel
    // finishes that tiny band and reports one credited-leaf overshoot. The
    // evidence harness validates this overshoot against explicit limits.
    assert!(limited_stdout.contains("checked 3 |"), "stdout was: {limited_stdout}");

    let multithread = Command::new(env!("CARGO_BIN_EXE_enum_kernel"))
        .arg(&path)
        .arg("2")
        .env("ENUM_LEAF_BUDGET", "2")
        .output()
        .expect("run multithread budgeted enum_kernel");
    assert_eq!(multithread.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&multithread.stderr)
        .contains("requires one thread"));

    fs::remove_file(path).expect("remove temporary enum fixture");
}
