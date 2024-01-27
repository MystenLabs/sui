// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::CommandFactory;
use std::{collections::BTreeMap, path::Path};
use sui::ptb::ptb::PTB;

fn test_parse_simple_ptb_files(path: &Path) -> datatest_stable::Result<()> {
    let ptb = PTB::default();
    let cmd = PTB::command();
    let file = path.to_str().unwrap();
    let args = cmd.get_matches_from(vec!["ptb", "--file", file]);
    assert!(ptb.from_matches(&args, None, &mut BTreeMap::new()).is_ok());

    Ok(())
}

datatest_stable::harness!(
    test_parse_simple_ptb_files,
    "tests/ptb_files/simple",
    r"^*.ptb"
);
