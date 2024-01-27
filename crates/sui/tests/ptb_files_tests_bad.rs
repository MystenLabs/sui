// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::CommandFactory;
use std::{collections::BTreeMap, path::Path};
use sui::ptb::{ptb::PTB, ptb_parser::parser::PTBParser};

// the PTB parser will complain, but clap will be fine
fn test_bad_ptb(path: &Path) -> datatest_stable::Result<()> {
    let ptb = PTB::default();
    let cmd = PTB::command();
    let file = path.to_str().unwrap();
    let args = cmd.get_matches_from(vec!["ptb", "--file", file]);
    let commands = ptb.from_matches(&args, None, &mut BTreeMap::new());
    assert!(commands.is_ok());

    let mut parser = PTBParser::new();
    for command in commands.unwrap() {
        parser.parse(command.1);
    }
    let (parsed, errors) = parser.finish();
    assert!(!errors.is_empty());

    Ok(())
}

datatest_stable::harness!(test_bad_ptb, "tests/ptb_files/bad_ptbs", r"^*.ptb");
