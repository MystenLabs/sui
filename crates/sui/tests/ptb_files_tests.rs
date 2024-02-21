// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use sui::client_ptb::ptb::PTB;
use sui_types::transaction::{CallArg, ObjectArg};
use test_cluster::TestClusterBuilder;

const TEST_DIR: &str = "tests";

#[cfg(not(msim))]
#[tokio::main]
async fn test_ptb_files(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use sui::client_ptb::{error::build_error_reports, ptb::PTBPreview, utils::to_source_string};
    use test_cluster::TestCluster;
    use tokio::sync::OnceCell;

    static CLUSTER: OnceCell<TestCluster> = OnceCell::const_new();

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
            insta::assert_display_snapshot!(fname(), results.join("\n"));
            return Ok(());
        }
    };

    // Preview (This is based on the parsed commands).
    let mut results = vec![];
    results.push(" === PREVIEW === ".to_string());
    results.push(format!("{}", PTBPreview { program: &program }));

    results.push(" === PROGRAM META === ".to_string());
    results.push(format!(
        "preview: {}\nsummary: {}\ngas_object: {}\njson: {}",
        program_meta.preview_set,
        program_meta.summary_set,
        program_meta
            .gas_object_id
            .map(|x| x.value.to_string())
            .unwrap_or("none".to_string()),
        program_meta.json_set
    ));

    // === BUILD PTB ===
    let test_cluster = CLUSTER
        .get_or_init(|| TestClusterBuilder::new().build())
        .await;

    let context = &test_cluster.wallet;
    let client = context.get_client().await?;

    let built_ptb = PTB::build_ptb(program, context, client).await;

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
    insta::assert_display_snapshot!(fname(), results.join("\n"));

    Ok(())
}

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

datatest_stable::harness!(test_ptb_files, TEST_DIR, r".*\.ptb$",);
