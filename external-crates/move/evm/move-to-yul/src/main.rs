// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use clap::Parser;
use codespan_reporting::term::termcolor::{ColorChoice, StandardStream};
use move_to_yul::{options::Options, run_to_yul};

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
    run_to_yul(&mut error_writer, options)
}
