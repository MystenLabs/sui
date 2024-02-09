// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::CommandFactory;
use std::{collections::BTreeMap, path::Path};
use sui::ptb::{
    ptb::PTB,
    ptb_builder::{errors::render_errors, parse_ptb::PTBParser},
};
use sui_types::transaction::{CallArg, ObjectArg};
use test_cluster::TestClusterBuilder;

const TEST_DIR: &str = "tests";

#[cfg_attr(not(msim), tokio::main)]
async fn test_ptb_files(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let fname = || {
        format!(
            "{}",
            path.file_name().unwrap().to_string_lossy().to_string()
        )
    };
    std::env::set_var("NO_COLOR", "true"); // we need this for the miette errors
    let ptb = PTB::default();
    let cmd = PTB::command();
    let file = path.to_str().unwrap();
    let args = cmd.get_matches_from(vec!["ptb", "--file", file, "--preview"]);
    let cwd = std::env::current_dir().unwrap();
    let commands = match ptb.from_matches(cwd, &args, &mut BTreeMap::new()) {
        Ok(commands) => commands,
        Err(e) => {
            insta::assert_display_snapshot!(
                fname(),
                format!("=== FILE EXPANSION ERROR === \n {:?}", e)
            );
            return Ok(());
        }
    };

    // === PREVIEW ===
    let ptb_preview = ptb.preview(&commands);
    let mut results = vec![];
    let preview_string = if let Some(ptb_preview) = ptb_preview {
        ptb_preview.to_string()
    } else {
        "".to_string()
    };
    results.push(" === PREVIEW === ".to_string());
    results.push(preview_string);

    // === PARSE COMMANDS ===
    let mut parser = PTBParser::new();
    for (_, cmd) in &commands {
        parser.parse(cmd.clone());
    }

    let parsed = match parser.finish() {
        Ok(parsed) => parsed,
        Err(errors) => {
            let rendered = render_errors(commands.clone(), errors);
            results.push(" === ERRORS AFTER PARSING INPUT COMMANDS === ".to_string());
            for e in rendered.iter() {
                results.push(format!("{:?}", e));
            }
            insta::assert_display_snapshot!(fname(), results.join("\n"));
            return Ok(());
        }
    };

    results.push(" === PARSED INPUT COMMANDS === ".to_string());

    for c in &parsed {
        let values = c
            .args
            .iter()
            .map(|x| x.value.to_string())
            .collect::<Vec<_>>();
        results.push(format!(
            "cmd: {}, value: {:?}",
            c.name.value.to_string(),
            values
        ));
    }

    // === BUILD PTB ===
    let test_cluster = TestClusterBuilder::new().build().await;
    let context = test_cluster.wallet;
    let client = context.get_client().await?;

    let built_ptb = ptb.parse_and_build_ptb(parsed, &context, client).await;

    if let Ok(ref ptb) = built_ptb {
        results.push(" === BUILT PTB === ".to_string());
        for (i, ca) in ptb.0.inputs.iter().enumerate() {
            results.push(format!("Input {}: {}", i, stable_call_arg_display(ca)));
        }
        for (i, c) in ptb.0.commands.iter().enumerate() {
            results.push(format!("Command {}: {}", i, c.to_string()));
        }
    }

    // === BUILDING PTB ERRORS ===
    if let Err(e) = built_ptb {
        let rendered = render_errors(commands, e);

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
