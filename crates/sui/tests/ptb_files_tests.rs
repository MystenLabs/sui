// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::Path};
use sui::client_ptb::{ptb::PTB, ptb_builder::errors::render_errors};
use sui_types::transaction::{CallArg, ObjectArg};
use test_cluster::TestClusterBuilder;

const TEST_DIR: &str = "tests";

#[cfg(not(msim))]
#[tokio::main]
async fn test_ptb_files(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let _ = miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .color(false)
                .width(80)
                .build(),
        )
    }));

    let fname = || path.file_name().unwrap().to_string_lossy().to_string();
    let file = path.to_str().unwrap().to_string();
    let args = vec!["--file".to_string(), file, "--preview".to_string()];
    let cwd = std::env::current_dir().unwrap();
    let mut commands = Vec::new();
    let ptb = PTB { args: args.clone() };
    ptb.parse_args(cwd, args, &mut BTreeMap::new(), &mut commands)?;

    // === PREVIEW ===
    let ptb_preview = ptb.preview(&commands.clone());
    let mut results = vec![];
    let preview_string = if let Some(ptb_preview) = ptb_preview {
        ptb_preview.to_string()
    } else {
        "".to_string()
    };
    results.push(" === PREVIEW === ".to_string());
    results.push(preview_string);

    // === PARSE COMMANDS ===
    let parsed_ptb_commands = ptb.parse_ptb_commands(commands.clone());
    match parsed_ptb_commands {
        Ok(ref commands) => commands,
        Err(e) => {
            // insta::assert_display_snapshot!(
            //     fname(),
            //     format!("=== FILE EXPANSION ERROR === \n {:?}", e)
            // );
            return Ok(());
        }
    };
    let parsed_ptb_commands = match parsed_ptb_commands {
        Ok(parsed) => parsed,
        Err(errors) => {
            let rendered = render_errors(&commands, errors);
            results.push(" === ERRORS AFTER PARSING INPUT COMMANDS === ".to_string());
            for e in rendered.iter() {
                results.push(format!("{:?}", e));
            }
            insta::assert_display_snapshot!(fname(), results.join("\n"));
            return Ok(());
        }
    };

    results.push(" === PARSED INPUT COMMANDS === ".to_string());

    for (_, c) in &parsed_ptb_commands {
        results.push(format!("cmd: {:?}", c));
    }

    // === BUILD PTB ===
    let test_cluster = TestClusterBuilder::new().build().await;
    let context = test_cluster.wallet;
    let client = context.get_client().await?;

    let built_ptb = ptb
        .parse_and_build_ptb(parsed_ptb_commands, &context, client)
        .await;

    if let Ok(ref ptb) = built_ptb {
        results.push(" === BUILT PTB === ".to_string());
        for (i, ca) in ptb.0.inputs.iter().enumerate() {
            results.push(format!("Input {}: {}", i, stable_call_arg_display(ca)));
        }
        for (i, c) in ptb.0.commands.iter().enumerate() {
            results.push(format!("Command {}: {}", i, c));
        }
    }

    // === BUILDING PTB ERRORS ===
    if let Err(e) = built_ptb {
        let rendered = render_errors(&commands, e);

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
