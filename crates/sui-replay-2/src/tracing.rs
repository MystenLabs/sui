// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Tracing utilities.
//! Mostly deals with directory/file saving and what gets saved in the trace output.

use crate::{
    artifacts::{Artifact, ArtifactManager},
    execution::TxnContextAndEffects,
};
use anyhow::Context;
use move_binary_format::CompiledModule;
use move_bytecode_source_map::utils::serialize_to_json_string;
use move_command_line_common::files::MOVE_BYTECODE_EXTENSION;
use move_disassembler::disassembler::Disassembler;
use move_ir_types::location::Spanned;
use move_trace_format::format::MoveTraceBuilder;
use std::fs;
use sui_types::object::Data;

const BCODE_DIR: &str = "bytecode";
const SOURCE_DIR: &str = "source";

/// Saves the trace and additional metadata needed to analyze the trace
/// to a subderectory named after the transaction digest.
pub fn save_trace_output(
    artifact_manager: &ArtifactManager<'_>,
    trace_builder: MoveTraceBuilder,
    context_and_effects: &TxnContextAndEffects,
) -> Result<(), anyhow::Error> {
    let trace = trace_builder.into_trace();
    let trace_member = artifact_manager.member(Artifact::Trace);
    trace_member
        .serialize_move_trace(trace)
        .transpose()?
        .unwrap();

    // TODO: have this use the artifact manager as well.
    let bcode_dir = artifact_manager.base_path.join(BCODE_DIR);
    fs::create_dir_all(&bcode_dir).context(format!(
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
        fs::create_dir_all(&bcode_pkg_dir)
            .context("Failed to create bytecode package directory")?;
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
    let src_dir = artifact_manager.base_path.join(SOURCE_DIR);
    fs::create_dir_all(&src_dir).context(format!(
        "Failed to create source output directory '{:?}'",
        src_dir,
    ))?;

    Ok(())
}
