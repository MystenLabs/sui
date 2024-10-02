// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use codespan_reporting::term::termcolor::{ColorChoice, StandardStream};

use move_prover::{cli::Options, run_move_prover};
use std::env;
use std::io::IsTerminal;

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
    let args: Vec<String> = env::args().collect();
    let options = Options::create_from_args(&args)?;
    let color = if std::io::stderr().is_terminal() && std::io::stdout().is_terminal() {
        ColorChoice::Auto
    } else {
        ColorChoice::Never
    };
    options.setup_logging();
    let mut error_writer = StandardStream::stderr(color);
    run_move_prover(&mut error_writer, options)
}
