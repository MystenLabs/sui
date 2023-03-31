// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::experiments::Experiment;
use clap::Parser;
use codespan_reporting::diagnostic::Severity;

/// Options for a run of the compiler.
#[derive(Parser, Debug)]
#[clap(author, version, about)]
pub struct Options {
    /// Directories where to lookup dependencies.
    #[clap(
        short,
        takes_value(true),
        multiple_values(true),
        multiple_occurrences(true)
    )]
    pub dependencies: Vec<String>,
    /// Named address mapping.
    #[clap(
        short,
        takes_value(true),
        multiple_values(true),
        multiple_occurrences(true)
    )]
    pub named_address_mapping: Vec<String>,
    /// Output file name.
    #[clap(short)]
    #[clap(long, default_value = "output.yul")]
    pub output: String,
    /// Solc executable
    #[clap(long, env = "SOLC_EXE", default_value = "solc")]
    pub solc_exe: String,
    /// Whether to dump bytecode to a file.
    #[clap(long = "dump-bytecode")]
    pub dump_bytecode: bool,
    /// Whether we generate code for tests.
    #[clap(long)]
    pub testing: bool,
    /// Active experiments.
    #[clap(short)]
    #[clap(
        long = "experiment",
        takes_value(true),
        multiple_values(true),
        multiple_occurrences(true)
    )]
    pub experiments: Vec<String>,
    /// Sources to compile (positional arg)
    pub sources: Vec<String>,
    #[clap(long = "abi-output", default_value = "output.abi.json")]
    pub abi_output: String,
}

impl Default for Options {
    fn default() -> Self {
        Parser::parse_from(std::iter::empty::<String>())
    }
}

impl Options {
    /// Returns the least severity of diagnosis which shall be reported.
    pub fn report_severity(&self) -> Severity {
        Severity::Warning
    }

    /// Returns the version of the tool.
    pub fn version(&self) -> &str {
        "0.0"
    }

    /// Returns true if an experiment is on.
    pub fn experiment_on(&self, name: &str) -> bool {
        self.experiments.iter().any(|s| s == name)
    }

    /// Returns true if source info should be generated during tests.
    pub fn generate_source_info(&self) -> bool {
        !self.testing || self.experiment_on(Experiment::CAPTURE_SOURCE_INFO)
    }
}
