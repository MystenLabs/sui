// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(not(msim))]
use std::path::Path;
#[cfg(not(msim))]
use sui_types::transaction::{CallArg, ObjectArg};

#[cfg(not(msim))]
const TEST_DIR: &str = "tests";

#[cfg(not(msim))]
#[tokio::main]
async fn test_ptb_files(path: &Path) -> datatest_stable::Result<()> {
    use sui::client_ptb::ptb::{to_source_string, PTB};
    use sui::client_ptb::{error::build_error_reports, ptb::PTBPreview};
    use test_cluster::TestClusterBuilder;

    let _ = miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .color(false)
                .width(80)
                .build(),
        )
    }));

    let fname = || path.file_name().unwrap().to_string_lossy().to_string();
    let file_contents = std::fs::read_to_string(path).unwrap();
    let shlexed = shlex::split(&file_contents).unwrap();
    let file_contents = to_source_string(shlexed.clone());

    // Parsing
    let program = PTB::parse_ptb_commands(shlexed);
    let (program, program_meta) = match program {
        Ok(program) => program,
        Err(errors) => {
            let rendered = build_error_reports(&file_contents, errors);
            let mut results = vec![];
            results.push(" === ERRORS AFTER PARSING INPUT COMMANDS === ".to_string());
            for e in rendered.iter() {
                results.push(format!("{:?}", e));
            }
            insta::assert_snapshot!(fname(), results.join("\n"));
            return Ok(());
        }
    };

    // Preview (This is based on the parsed commands).
    let mut results = vec![];
    results.push(" === PREVIEW === ".to_string());
    results.push(format!(
        "{}",
        PTBPreview {
            program: &program,
            program_metadata: &program_meta
        }
    ));

    // === BUILD PTB ===
    let test_cluster = TestClusterBuilder::new().build().await;

    let context = &test_cluster.wallet;
    let client = context.get_client().await?;

    let (built_ptb, warnings) = PTB::build_ptb(program, context, client).await;

    if !warnings.is_empty() {
        let rendered = build_error_reports(&file_contents, warnings);
        results.push(" === WARNINGS === ".to_string());
        for warning in rendered.iter() {
            results.push(format!("{:?}", warning));
        }
    }

    if let Ok(ref ptb) = built_ptb {
        results.push(" === BUILT PTB === ".to_string());
        for (i, ca) in ptb.inputs.iter().enumerate() {
            results.push(format!("Input {}: {}", i, stable_call_arg_display(ca)));
        }
        for (i, c) in ptb.commands.iter().enumerate() {
            results.push(format!("Command {}: {}", i, c));
        }
    }

    // === BUILDING PTB ERRORS ===
    if let Err(e) = built_ptb {
        let rendered = build_error_reports(&file_contents, e);

        results.push(" === BUILDING PTB ERRORS === ".to_string());
        for e in rendered.iter() {
            results.push(format!("{:?}", e));
        }
    }

    // === FINALLY DO THE ASSERTION ===
    insta::assert_snapshot!(fname(), results.join("\n"));

    Ok(())
}

#[cfg(not(msim))]
fn stable_call_arg_display(ca: &CallArg) -> String {
    match ca {
        CallArg::Pure(v) => format!("Pure({:?})", v),
        CallArg::Object(oa) => match oa {
            ObjectArg::ImmOrOwnedObject(_) => "ImmutableOrOwnedObject".to_string(),
            ObjectArg::SharedObject { mutable, .. } => {
                format!("SharedObject(mutable: {})", mutable)
            }
            ObjectArg::Receiving(_) => "Receiving".to_string(),
        },
    }
}

#[cfg(not(msim))]
datatest_stable::harness!(test_ptb_files, TEST_DIR, r".*\.ptb$",);

#[cfg(msim)]
fn main() {}
