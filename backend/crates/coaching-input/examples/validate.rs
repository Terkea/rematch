//! `cargo run -p coaching-input --example validate -- <file.json>...` (R2 Task 35): checks each
//! document against the TS zod export (`fixtures/coaching-input/ts-schema.json`) and the
//! contract's own rules (`CoachingInput::validate`). The zod export is v1's, so a v2 document
//! (ADR 0085) is checked against the contract's own schema (`json_schema()`) instead. Prints
//! `valid` per file, or every issue and exits 1.

use std::process::ExitCode;

use coaching_input::{CoachingInput, INPUT_VERSION_V1, json_schema};
use serde_json::Value;

const TS_SCHEMA: &str = include_str!("../../../fixtures/coaching-input/ts-schema.json");

fn check(
    path: &str,
    ts: &jsonschema::Validator,
    rs: &jsonschema::Validator,
) -> Result<(), Vec<String>> {
    let text = std::fs::read_to_string(path).map_err(|e| vec![format!("read: {e}")])?;
    let doc: Value = serde_json::from_str(&text).map_err(|e| vec![format!("JSON: {e}")])?;
    let (name, schema) = if doc["inputVersion"] == INPUT_VERSION_V1 {
        ("TS schema", ts)
    } else {
        ("contract schema", rs)
    };
    let mut issues: Vec<String> = schema
        .iter_errors(&doc)
        .map(|e| format!("{name}: {e} at {}", e.instance_path()))
        .collect();
    match serde_json::from_value::<CoachingInput>(doc) {
        Ok(input) => {
            if let Err(found) = input.validate() {
                issues.extend(found.iter().map(|i| format!("contract: {i:?}")));
            }
        }
        Err(e) => issues.push(format!("contract type: {e}")),
    }
    if issues.is_empty() {
        Ok(())
    } else {
        Err(issues)
    }
}

fn main() -> ExitCode {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        eprintln!("usage: validate <coaching-input.json>...");
        return ExitCode::from(2);
    }
    let schema: Value = serde_json::from_str(TS_SCHEMA).expect("ts-schema.json is JSON");
    let ts = jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)
        .expect("the TS schema compiles");
    let rs = jsonschema::options()
        .should_validate_formats(true)
        .build(&json_schema())
        .expect("the contract schema compiles");
    let mut ok = true;
    for path in &paths {
        match check(path, &ts, &rs) {
            Ok(()) => println!("{path}: valid"),
            Err(issues) => {
                ok = false;
                println!("{path}: invalid");
                for i in issues {
                    println!("  {i}");
                }
            }
        }
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
