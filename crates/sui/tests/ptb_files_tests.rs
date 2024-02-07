// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::CommandFactory;
use std::{collections::BTreeMap, path::Path};
use sui::ptb::{
    ptb::PTB,
    ptb_parser::{errors::render_errors, parser::PTBParser},
};
use test_cluster::TestClusterBuilder;

const TEST_DIR: &str = "tests";

fn test_ptb_preview(path: &Path) -> datatest_stable::Result<()> {
    let ptb = PTB::default();
    let cmd = PTB::command();
    let file = path.to_str().unwrap();
    let args = cmd.get_matches_from(vec!["ptb", "--file", file, "--preview"]);
    let cwd = std::env::current_dir().unwrap();
    let commands = ptb.from_matches(cwd, &args, &mut BTreeMap::new()).unwrap();
    let ptb_preview = ptb.preview(&commands);
    let results = if let Some(ptb_preview) = ptb_preview {
        ptb_preview.to_string()
    } else {
        "".to_string()
    };
    insta::assert_display_snapshot!(
        format!(
            "preview_{}",
            path.file_name().unwrap().to_string_lossy().to_string()
        ),
        results
    );
    Ok(())
}

// check if all good ptb files can be parsed OK

#[cfg_attr(not(msim), tokio::main)]
async fn test_parse_all_ptb_files(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let ptb = PTB::default();
    let cmd = PTB::command();
    let file = path.to_str().unwrap();
    let args = cmd.get_matches_from(vec!["ptb", "--file", file]);
    let cwd = std::env::current_dir().unwrap();
    let commands = ptb.from_matches(cwd, &args, &mut BTreeMap::new()).unwrap();

    let mut parser = PTBParser::new();
    for (_, cmd) in commands {
        parser.parse(cmd.clone());
    }
    let (parsed, _errors) = parser.finish();

    let mut cmds = vec![];

    for c in parsed {
        let values = c
            .args
            .iter()
            .map(|x| x.value.to_string())
            .collect::<Vec<_>>();
        cmds.push(format!(
            "cmd: {}, value: {:?}",
            c.name.value.to_string(),
            values
        ));
    }

    let results = cmds.join("\n");

    insta::assert_display_snapshot!(
        format!(
            "parsed_{}",
            path.file_name().unwrap().to_string_lossy().to_string()
        ),
        results
    );
    Ok(())
}

// check if the bad ptb files return errors when parsed
fn test_parse_bad_ptb_files(path: &Path) -> datatest_stable::Result<()> {
    std::env::set_var("NO_COLOR", "true");
    let ptb = PTB::default();
    let cmd = PTB::command();
    let file = path.to_str().unwrap();
    let args = cmd.get_matches_from(vec!["ptb", "--file", file]);
    let cwd = std::env::current_dir().unwrap();
    let commands = ptb.from_matches(cwd, &args, &mut BTreeMap::new()).unwrap();
    let mut parser = PTBParser::new();
    for (_, cmd) in commands.clone() {
        parser.parse(cmd.clone());
    }
    let (_parsed_commands, errors) = parser.finish();
    assert!(!errors.is_empty());
    let rendered = render_errors(commands, errors.clone());
    let mut results = vec![];
    for e in rendered.iter() {
        results.push(format!("{:?}", e));
    }

    insta::assert_display_snapshot!(
        format!(
            "errors_{}",
            path.file_name().unwrap().to_string_lossy().to_string()
        ),
        results.join("\n")
    );
    Ok(())
}

// parse, build PTB, and run it locally in a network
async fn test_build_and_run_ptb_from_files(path: &Path) -> datatest_stable::Result<()> {
    let ptb = PTB::default();
    let cmd = PTB::command();
    let file = path.to_str().unwrap();
    let args = cmd.get_matches_from(vec!["ptb", "--file", file]);
    let test_cluster = TestClusterBuilder::new().build().await;
    let context = test_cluster.wallet;

    ptb.execute(args, Some(context)).await.unwrap();
    Ok(())
}

#[cfg_attr(not(msim), tokio::main)]
async fn test_no_gas_picker_from_files(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let ptb = PTB::default();
    let cmd = PTB::command();
    let file = path.to_str().unwrap();
    let args = cmd.get_matches_from(vec!["ptb", "--file", file]);
    let cwd = std::env::current_dir().unwrap();
    let commands = ptb.from_matches(cwd, &args, &mut BTreeMap::new()).unwrap();
    let mut parser = PTBParser::new();
    for (_, cmd) in commands.clone() {
        parser.parse(cmd.clone());
    }
    let (parsed, _errors) = parser.finish();

    let test_cluster = TestClusterBuilder::new().build().await;
    let context = test_cluster.wallet;
    let client = context.get_client().await?;

    let built_ptb = ptb.parse_and_build_ptb(parsed, &context, client).await;
    assert!(built_ptb.is_err());

    let rendered = render_errors(commands, built_ptb.err().unwrap());
    let mut results = vec![];
    for e in rendered.iter() {
        results.push(format!("{}", e));
    }

    insta::assert_display_snapshot!(format!("error_gas_picker"), results.join("\n"));
    Ok(())
}

datatest_stable::harness!(
    test_ptb_preview,
    TEST_DIR,
    r".*\.ptb$",
    test_parse_all_ptb_files,
    TEST_DIR,
    r".*\.ptb$",
    test_parse_bad_ptb_files,
    TEST_DIR,
    r"^.*\.bad$",
    test_no_gas_picker_from_files,
    TEST_DIR,
    r"no_gas_picker"
);
