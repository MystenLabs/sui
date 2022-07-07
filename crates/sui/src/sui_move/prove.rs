// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use move_cli::base::{self, prove};
use move_package::BuildConfig;
use std::path::{Path, PathBuf};

#[derive(Parser)]
pub struct Prove {
    #[clap(flatten)]
    pub prove: prove::Prove,
}

impl Prove {
    pub fn execute(self, path: Option<PathBuf>, build_config: BuildConfig) -> anyhow::Result<()> {
        let rerooted_path = base::reroot_path(path)?;
        let prove::Prove {
            target_filter,
            for_test,
            options,
        } = self.prove;
        let opts = match options {
            Some(prove::ProverOptions::Options(opts)) => opts,
            _ => vec![],
        };
        let mut args = vec!["package".to_string()];
        let prover_toml = Path::new(&rerooted_path).join("Prover.toml");
        if prover_toml.exists() {
            args.push(format!("--config={}", prover_toml.to_string_lossy()));
        }
        args.extend(opts.iter().cloned());
        let options = move_prover::cli::Options::create_from_args(&args)?;

        prove::run_move_prover(
            build_config,
            &rerooted_path,
            &target_filter,
            for_test,
            options,
        )?;

        Ok(())
    }
}
