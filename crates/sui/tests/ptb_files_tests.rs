// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::CommandFactory;
use std::{collections::BTreeMap, path::Path};
use sui::ptb::{
    ptb::PTB,
    ptb_parser::{errors::render_errors, parser::PTBParser},
};
use test_cluster::TestClusterBuilder;

// check if all good ptb files can be parsed OK
fn test_parse_all_ptb_files(path: &Path) -> datatest_stable::Result<()> {
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
    let (_parsed_commands, errors) = parser.finish();
    assert!(errors.is_empty());

    Ok(())
}

// check if the bad ptb files return errors when parsed
fn test_parse_bad_ptb_files(path: &Path) -> datatest_stable::Result<()> {
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
    let rendered = render_errors(commands, errors.clone());
    for e in rendered.iter() {
        println!("{:?}", e);
    }
    println!("{:?}", errors.len());
    assert!(!errors.is_empty());
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

async fn test_no_gas_picker_from_files(path: &Path) -> datatest_stable::Result<()> {
    let ptb = PTB::default();
    let cmd = PTB::command();
    let file = path.to_str().unwrap();
    let args = cmd.get_matches_from(vec!["ptb", "--file", file]);
    let test_cluster = TestClusterBuilder::new().build().await;
    let context = test_cluster.wallet;

    let execute = ptb.execute(args, Some(context)).await;
    assert!(execute.is_err());
    Ok(())
}

datatest_stable::harness!(
    test_parse_all_ptb_files,
    "tests/ptb_files",
    r"^*.ptb$",
    test_parse_bad_ptb_files,
    "tests/ptb_files",
    r"^*.bad",
    // test_build_and_run_ptb_from_files,
    // "tests/ptb_files",
    // r"^run_*.sh",
    // test_no_gas_picker_from_files
    // "tests/ptb_files",
    // "no_gas_picker",
);
