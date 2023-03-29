// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use clap::Parser;
use codespan_reporting::term::termcolor::{ColorChoice, StandardStream};
use move_to_yul::{options::Options, parse_metadata_to_move_sig, run_to_abi_metadata};

fn main() {
    if let Err(e) = run() {
        eprintln!("{}", e);
        let mut c = e.source();
        while let Some(s) = c {
            eprintln!("caused by: {}", s);
            c = s.source();
        }
        std::process::exit(1)
    }
}

fn run() -> anyhow::Result<()> {
    let options = Options::parse();
    let color = if atty::is(atty::Stream::Stderr) && atty::is(atty::Stream::Stdout) {
        ColorChoice::Auto
    } else {
        ColorChoice::Never
    };
    let mut error_writer = StandardStream::stderr(color);
    let metadata_vec = run_to_abi_metadata(&mut error_writer, options).unwrap();
    for metadata in metadata_vec {
        let move_sig_opt = parse_metadata_to_move_sig(&metadata);
        if let Some(move_sig) = move_sig_opt {
            println!("{}", serde_json::to_string_pretty(&move_sig).unwrap());
        }
    }
    Ok(())
}
