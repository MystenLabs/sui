// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Tracing utilities.
//! Mostly deals with directory/file saving and what gets saved in the trace output.

use crate::execution::TxnContextAndEffects;
use anyhow::Context;
use move_binary_format::CompiledModule;
use move_bytecode_source_map::utils::serialize_to_json_string;
use move_command_line_common::files::MOVE_BYTECODE_EXTENSION;
use move_disassembler::disassembler::Disassembler;
use move_ir_types::location::Spanned;
use move_trace_format::format::MoveTraceBuilder;
use std::{
    env, fs,
    path::{Path, PathBuf},
};
use sui_types::object::Data;

const DEFAULT_TRACE_OUTPUT_DIR: &str = ".replay";
const TRACE_FILE_NAME: &str = "trace.json.zst";
const BCODE_DIR: &str = "bytecode";
const SOURCE_DIR: &str = "source";

/// Gets the path to store trace output (either the default one './replay' or user-specified).
/// Upon success, the path will exist in the file system.
pub fn get_trace_output_path(trace_execution: Option<PathBuf>) -> Result<PathBuf, anyhow::Error> {
    match trace_execution {
        Some(path) => {
            if !path.exists() {
                return Err(anyhow::anyhow!(format!(
                    "User-specified path to store trace output does not exist: {:?}",
                    path
                )));
            }
            if !path.is_dir() {
                return Err(anyhow::anyhow!(format!(
                    "User-specified path to store trace output is not a directory: {:?}",
                    path
                )));
            }
            Ok(path)
        }
        None => {
            let current_dir = env::current_dir().context("Failed to get current directory")?;
            let path = current_dir.join(DEFAULT_TRACE_OUTPUT_DIR);
            if path.exists() && path.is_file() {
                return Err(anyhow::anyhow!(
                    format!(
                        "Default path to store trace output already exists and is a file, not a directory: {:?}",
                        path
                    )
                ));
            }
            fs::create_dir_all(&path).context("Failed to create default trace output directory")?;
            Ok(path)
        }
    }
}

/// Saves the trace and additional metadata needed to analyze the trace
/// to a subderectory named after the transaction digest.
pub fn save_trace_output(
    output_path: &Path,
    digest: &str,
    trace_builder: MoveTraceBuilder,
    context_and_effects: &TxnContextAndEffects,
) -> Result<(), anyhow::Error> {
    let txn_output_path = output_path.join(digest);
    if txn_output_path.exists() {
        return Err(anyhow::anyhow!(
            "Trace output directory for transaction {} already exists: {:?}",
            digest,
            txn_output_path,
        ));
    }
    fs::create_dir_all(&txn_output_path).context(format!(
        "Failed to create trace output directory for transaction {}",
        digest,
    ))?;
    let trace = trace_builder.into_trace();
    let json = trace.into_compressed_json_bytes();
    let trace_file_path = txn_output_path.join(TRACE_FILE_NAME);
    fs::write(&trace_file_path, json).context(format!(
        "Failed to write trace output to {:?}",
        trace_file_path,
    ))?;
    let bcode_dir = txn_output_path.join(BCODE_DIR);
    fs::create_dir(&bcode_dir).context(format!(
        "Failed to create bytecode output directory '{:?}'",
        bcode_dir,
    ))?;

    let TxnContextAndEffects {
        execution_effects: _,
        expected_effects: _,
        gas_status: _,
        object_cache,
        inner_store: tmp_store,
    } = context_and_effects;

    // grab all packages from the transaction and save them locally for debug
    let mut pkgs = object_cache
        .values()
        .flat_map(|versions| versions.values())
        .filter_map(|obj| {
            if let Data::Package(pkg) = &obj.data {
                Some(pkg)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    for obj in tmp_store.written.values() {
        if let Data::Package(pkg) = &obj.data {
            pkgs.push(pkg);
        }
    }
    for pkg in pkgs {
        let pkg_addr = format!("{:?}", pkg.id());
        let bcode_pkg_dir = bcode_dir.join(&pkg_addr);
        fs::create_dir(&bcode_pkg_dir).context("Failed to create bytecode package directory")?;
        for (mod_name, serialized_mod) in pkg.serialized_module_map() {
            let compiled_mod =
                CompiledModule::deserialize_with_defaults(serialized_mod).context(format!(
                    "Failed to deserialize module {:?} in package {}",
                    mod_name, &pkg_addr,
                ))?;
            let d = Disassembler::from_module(&compiled_mod, Spanned::unsafe_no_loc(()).loc)
                .context(format!(
                    "Failed to create disassembler for module {:?} in package {}",
                    mod_name, &pkg_addr,
                ))?;
            let (disassemble_string, bcode_map) =
                d.disassemble_with_source_map().context(format!(
                    "Failed to disassemble module {:?} in package {}",
                    mod_name, &pkg_addr,
                ))?;
            let bcode_map_json = serialize_to_json_string(&bcode_map).context(format!(
                "Failed to serialize bytecode source map for module {:?} in package {}",
                mod_name, &pkg_addr,
            ))?;
            fs::write(
                bcode_pkg_dir.join(format!("{}.{}", mod_name, MOVE_BYTECODE_EXTENSION)),
                disassemble_string,
            )
            .context(format!(
                "Failed to write disassembled bytecode for module {:?} in package {}",
                mod_name, &pkg_addr,
            ))?;
            fs::write(
                bcode_pkg_dir.join(format!("{}.json", mod_name)),
                bcode_map_json,
            )
            .context(format!(
                "Failed to write bytecode source map for module {:?} in package {}",
                mod_name, &pkg_addr,
            ))?;
        }
    }
    // create empty sources directory as a known placeholder for the users
    // to put optional source files there
    let src_dir = txn_output_path.join(SOURCE_DIR);
    fs::create_dir(&src_dir).context(format!(
        "Failed to create source output directory '{:?}'",
        src_dir,
    ))?;

    Ok(())
}
