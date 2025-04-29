// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use clap::{Command, Parser, Subcommand};

/// Build the package
#[derive(Debug, Clone, Parser)]
pub struct Build {
    /// Path to the project
    #[arg(name = "path", short = 'p', long = "path", default_value = ".")]
    path: Option<PathBuf>,
}

impl Build {
    pub fn execute(&self) {
        let path = self.path.clone().unwrap_or_else(|| PathBuf::from("."));
        println!("Building package at {:?}", path);
    }
}
