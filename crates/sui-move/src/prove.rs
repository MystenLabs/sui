// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use move_cli::base::{self, prove};
use move_package::BuildConfig;
use std::path::{Path, PathBuf};
use sui_types::sui_framework_address_concat_string;

const SUI_NATIVE_TEMPLATE: &[u8] = include_bytes!("sui-natives.bpl");

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

        let mut options = move_prover::cli::Options::create_from_args(&args)?;

        // it requires custom treatment due to it implementing custom borrow semantics
        options
            .prover
            .borrow_natives
            .push("dynamic_field::borrow_child_object_mut".to_string());

        // provide Sui-specific Boogie template for the native functions to the prover.
        if options.backend.custom_natives.is_none() {
            options.backend.custom_natives =
                Some(move_prover_boogie_backend::options::CustomNativeOptions {
                    template_bytes: SUI_NATIVE_TEMPLATE.to_vec(),
                    module_instance_names: vec![
                        (
                            sui_framework_address_concat_string("::transfer"),
                            "transfer_instances".to_string(),
                            true,
                        ),
                        (
                            sui_framework_address_concat_string("::object"),
                            "object_instances".to_string(),
                            true,
                        ),
                        (
                            sui_framework_address_concat_string("::event"),
                            "sui_event_instances".to_string(),
                            true,
                        ),
                        (
                            sui_framework_address_concat_string("::types"),
                            "sui_types_instances".to_string(),
                            true,
                        ),
                        (
                            sui_framework_address_concat_string("::dynamic_field"),
                            "dynamic_field_instances".to_string(),
                            false,
                        ),
                        (
                            sui_framework_address_concat_string("::prover"),
                            "prover_instances".to_string(),
                            true,
                        ),
                    ],
                });
        }
        // tell the backend what the names of aggregates implementing custom borrow semantics in
        // Boogie are
        options.backend.borrow_aggregates.push(
            move_prover_boogie_backend::options::BorrowAggregate::new(
                "dynamic_field::borrow_child_object_mut".to_string(),
                "GetDynField".to_string(),
                "UpdateDynField".to_string(),
            ),
        );

        let prover_result = std::thread::spawn(move || {
            prove::run_move_prover(
                build_config,
                &rerooted_path,
                &target_filter,
                for_test,
                options,
            )
        });
        prover_result
            .join()
            .unwrap_or_else(|err| Err(anyhow::anyhow!("{:?}", err)))
    }
}
