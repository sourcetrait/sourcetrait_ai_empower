//! The two nubed channels end to end: a `$in` pipeline filter from source,
//! and a `main`-with-parameters script file.
//!
//! Run: `cargo run -p sourcetrait_grammar_nubed --example filter`

use sourcetrait_grammar_nubed::{NuBed, NuBedConfig, Value, record};
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bed = NuBed::new(
        NuBedConfig::builder()
            .env("FILTER_MODE", Value::test_string("example"))
            .timeout(Duration::from_secs(5))
            .build(),
    )?;

    // Channel 1: pipeline input. The script is a plain filter over $in.
    let input = Value::test_record(record! {
        "name" => Value::test_string("instructions"),
        "mode" => Value::test_string("draft"),
    });
    let out = bed.run_script(
        "$in | update name {|row| $row.name | str upcase } | insert mode_env { $env.FILTER_MODE }",
        "inline-filter.nu",
        Some(input),
        &[],
    )?;
    println!("filter out: {out:?}");

    // Channel 2: main with parameters, from a script file.
    let dir = tempfile::tempdir()?;
    let script = dir.path().join("soak.after.nu");
    std::fs::write(
        &script,
        r#"
export def main [fill: record<agent: record<kind: string>>] {
    let instructions_filename = match $fill.agent.kind {
        "claude" => "CLAUDE.md"
        _ => "AGENTS.md"
    }

    { mv: [ { from: "INSTRUCTIONS.md", to: $instructions_filename } ] }
}
"#,
    )?;
    let fill = Value::test_record(record! {
        "agent" => Value::test_record(record! { "kind" => Value::test_string("claude") }),
    });
    let out = bed.run_script_file(&script, None, &[fill])?;
    println!("main out: {out:?}");

    Ok(())
}
