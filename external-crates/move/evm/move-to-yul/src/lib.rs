// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

mod abi_move_metadata;
pub use abi_move_metadata::parse_metadata_to_move_sig;
mod abi_native_functions;
mod abi_signature;
mod attributes;
mod context;
mod dispatcher_generator;
mod events;
mod evm_transformation;
mod experiments;
mod external_functions;
mod functions;
pub mod generator;
mod native_functions;
pub mod options;
mod solidity_ty;
mod storage;
mod tables;
mod vectors;
mod yul_functions;

use crate::{generator::Generator, options::Options};
use anyhow::anyhow;
use codespan_reporting::{
    diagnostic::Severity,
    term::termcolor::{ColorChoice, StandardStream, WriteColor},
};
use move_compiler::{shared::PackagePaths, Flags};
use move_core_types::metadata::Metadata;
use move_model::{
    model::GlobalEnv, options::ModelBuilderOptions, parse_addresses_from_options,
    run_model_builder_with_options_and_compilation_flags,
};
use std::fs;

/// Run move-to-yul compiler and print errors to stderr.
pub fn run_to_yul_errors_to_stderr(options: Options) -> anyhow::Result<()> {
    let mut error_writer = StandardStream::stderr(ColorChoice::Auto);
    run_to_yul(&mut error_writer, options)
}

/// Run move-to-yul compiler and print errors to given writer.
pub fn run_to_yul<W: WriteColor>(error_writer: &mut W, mut options: Options) -> anyhow::Result<()> {
    // Run the model builder.
    let addrs = parse_addresses_from_options(options.named_address_mapping.clone())?;
    let env = run_model_builder_with_options_and_compilation_flags(
        vec![PackagePaths {
            name: None,
            paths: options.sources.clone(),
            named_address_map: addrs.clone(),
        }],
        vec![PackagePaths {
            name: None,
            paths: options.dependencies.clone(),
            named_address_map: addrs,
        }],
        ModelBuilderOptions::default(),
        Flags::empty().set_flavor("async"),
    )?;
    // If the model contains any errors, report them now and exit.
    check_errors(
        &env,
        &options,
        error_writer,
        "exiting with Move build errors",
    )?;
    let mut contracts = Generator::run(&options, &env);
    if contracts.len() > 1 {
        env.diag(
            Severity::Warning,
            &env.unknown_loc(),
            "current restriction: \
        only one contract per compiler run (additional contracts ignored)",
        );
    }
    check_errors(
        &env,
        &options,
        error_writer,
        "exiting with Yul generation errors",
    )?;
    if let Some((_, content, abi_content)) = contracts.pop() {
        if let Some(i) = options.output.rfind('.') {
            options.abi_output = format!("{}.abi.json", &options.output[..i]);
        }
        fs::write(options.output, &content)?;
        fs::write(options.abi_output, &abi_content)?;
    }
    Ok(())
}

/// Generate metadata for move-ethereum-abi
pub fn run_to_abi_metadata<W: WriteColor>(
    error_writer: &mut W,
    options: Options,
) -> anyhow::Result<Vec<Metadata>> {
    // Run the model builder.
    let addrs = parse_addresses_from_options(options.named_address_mapping.clone())?;
    let env = run_model_builder_with_options_and_compilation_flags(
        vec![PackagePaths {
            name: None,
            paths: options.sources.clone(),
            named_address_map: addrs.clone(),
        }],
        vec![PackagePaths {
            name: None,
            paths: options.dependencies.clone(),
            named_address_map: addrs,
        }],
        ModelBuilderOptions::default(),
        Flags::empty().set_flavor("async"),
    )?;
    // If the model contains any errors, report them now and exit.
    check_errors(
        &env,
        &options,
        error_writer,
        "exiting with Move build errors",
    )?;
    let metadata_vec = Generator::generate_abi_metadata(&options, &env);
    check_errors(
        &env,
        &options,
        error_writer,
        "exiting with Yul generation errors",
    )?;
    Ok(metadata_vec)
}

pub fn check_errors<W: WriteColor>(
    env: &GlobalEnv,
    options: &Options,
    error_writer: &mut W,
    msg: &'static str,
) -> anyhow::Result<()> {
    env.report_diag(error_writer, options.report_severity());
    if env.has_errors() {
        Err(anyhow!(msg))
    } else {
        Ok(())
    }
}
