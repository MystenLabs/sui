// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Memory usage tracking utilities for debugging LSP memory consumption
//!
//! Uses the `deepsize` crate to compute exact heap allocations recursively.

use std::{
    path::PathBuf,
    sync::Arc,
};

use deepsize::DeepSizeOf;
use lsp_types::Diagnostic;

use crate::{
    compiler_info::CompilerAutocompleteInfo,
    symbols::{
        compilation::{CachedPackages, CachedPkgInfo},
        Symbols,
    },
};

const BYTES_PER_MB: f64 = 1024.0 * 1024.0;

/// Average estimated size per Diagnostic (strings + metadata)
const AVG_DIAGNOSTIC_SIZE: usize = 256;

/// Manually estimate memory for Diagnostics
/// Cannot implement Allocative for external lsp_types::Diagnostic, so we approximate
fn estimate_diagnostics_size(diags: &std::collections::BTreeMap<PathBuf, Vec<Diagnostic>>) -> usize {
    let count: usize = diags.values().map(|v| v.len()).sum();
    count * AVG_DIAGNOSTIC_SIZE
}

/// Logs detailed breakdown of CompilerAutocompleteInfo memory usage
fn log_compiler_autocomplete_info_breakdown(compiler_info: &CompilerAutocompleteInfo, indent: &str) {
    let dot_autocomplete_bytes = (&compiler_info.dot_autocomplete_info).deep_size_of();
    let dot_autocomplete_mb = dot_autocomplete_bytes as f64 / BYTES_PER_MB;

    let path_autocomplete_bytes = (&compiler_info.path_autocomplete_info).deep_size_of();
    let path_autocomplete_mb = path_autocomplete_bytes as f64 / BYTES_PER_MB;

    eprintln!("{}  CompilerAutocompleteInfo breakdown:", indent);
    eprintln!("{}    dot_autocomplete_info: {:.2} MB ({} files)", indent, dot_autocomplete_mb, compiler_info.dot_autocomplete_info.len());
    eprintln!("{}    path_autocomplete_info: {:.2} MB ({} entries)", indent, path_autocomplete_mb, compiler_info.path_autocomplete_info.len());
}

/// Logs detailed memory breakdown for a single CachedPkgInfo
pub fn log_pkg_info_breakdown(pkg_path: &str, info: &CachedPkgInfo) {
    // Measure individual components - MUST use .as_ref() to dereference Arc and measure actual data
    let program_bytes = info.program.as_ref().deep_size_of();
    let program_mb = program_bytes as f64 / BYTES_PER_MB;

    let parsed_defs_bytes = (&info.program.parsed_definitions).deep_size_of();
    let parsed_defs_mb = parsed_defs_bytes as f64 / BYTES_PER_MB;

    let typed_modules_bytes = (&info.program.typed_modules).deep_size_of();
    let typed_modules_mb = typed_modules_bytes as f64 / BYTES_PER_MB;

    let deps_symbols_bytes = info.deps_symbols_data.as_ref().deep_size_of();
    let deps_symbols_mb = deps_symbols_bytes as f64 / BYTES_PER_MB;

    let file_paths_bytes = info.file_paths.as_ref().deep_size_of();
    let file_paths_mb = file_paths_bytes as f64 / BYTES_PER_MB;

    let user_file_hashes_bytes = info.user_file_hashes.as_ref().deep_size_of();
    let user_file_hashes_mb = user_file_hashes_bytes as f64 / BYTES_PER_MB;

    let deps_bytes = info.deps.as_ref().deep_size_of();
    let deps_mb = deps_bytes as f64 / BYTES_PER_MB;

    // Manual estimation for skipped lsp_types::Diagnostic (external type, cannot add Allocative)
    let lsp_diags_bytes = estimate_diagnostics_size(info.lsp_diags.as_ref());
    let lsp_diags_mb = lsp_diags_bytes as f64 / BYTES_PER_MB;
    let diag_count: usize = info.lsp_diags.values().map(|v| v.len()).sum();

    eprintln!("[MEMORY]     {} AST breakdown:", pkg_path);
    eprintln!("[MEMORY]       program total: {:.2} MB", program_mb);
    eprintln!("[MEMORY]         parsed_definitions (parser AST): {:.2} MB ({} source, {} lib)",
        parsed_defs_mb,
        info.program.parsed_definitions.source_definitions.len(),
        info.program.parsed_definitions.lib_definitions.len()
    );
    eprintln!("[MEMORY]         typed_modules (typing AST): {:.2} MB ({} modules)",
        typed_modules_mb,
        info.program.typed_modules.len()
    );
    eprintln!("[MEMORY]       deps (PreCompiledProgramInfo): {:.2} MB ({} modules)",
        deps_mb,
        info.deps.iter().count()
    );
    eprintln!("[MEMORY]       deps_symbols_data: {:.2} MB", deps_symbols_mb);
    eprintln!("[MEMORY]       file_paths: {:.2} MB ({} entries)", file_paths_mb, info.file_paths.len());
    eprintln!("[MEMORY]       user_file_hashes: {:.2} MB ({} entries)", user_file_hashes_mb, info.user_file_hashes.len());
    eprintln!("[MEMORY]       lsp_diags (estimated): {:.2} MB ({} diagnostics @ {} bytes each)",
        lsp_diags_mb, diag_count, AVG_DIAGNOSTIC_SIZE);
}

/// Logs memory usage for a single Symbols entry using allocative with detailed breakdown
pub fn log_symbols_memory(pkg_path: &str, symbols: &Symbols) {
    // Measure total
    let total_bytes = (symbols).deep_size_of();
    let total_mb = total_bytes as f64 / BYTES_PER_MB;

    // Measure individual components
    let references_bytes = (&symbols.references).deep_size_of();
    let references_mb = references_bytes as f64 / BYTES_PER_MB;
    let references_count = symbols.references.len();

    let file_use_defs_bytes = (&symbols.file_use_defs).deep_size_of();
    let file_use_defs_mb = file_use_defs_bytes as f64 / BYTES_PER_MB;
    let file_use_defs_count = symbols.file_use_defs.len();

    let file_mods_bytes = (&symbols.file_mods).deep_size_of();
    let file_mods_mb = file_mods_bytes as f64 / BYTES_PER_MB;
    let file_mods_count = symbols.file_mods.len();

    let files_bytes = (&symbols.files).deep_size_of();
    let files_mb = files_bytes as f64 / BYTES_PER_MB;

    let def_info_bytes = (&symbols.def_info).deep_size_of();
    let def_info_mb = def_info_bytes as f64 / BYTES_PER_MB;
    let def_info_count = symbols.def_info.len();

    let compiler_autocomplete_info_bytes = (&symbols.compiler_autocomplete_info).deep_size_of();
    let compiler_autocomplete_info_mb = compiler_autocomplete_info_bytes as f64 / BYTES_PER_MB;

    let cursor_context_bytes = (&symbols.cursor_context).deep_size_of();
    let cursor_context_mb = cursor_context_bytes as f64 / BYTES_PER_MB;

    eprintln!("[MEMORY] symbols_map[\"{}\"]: {:.2} MB total", pkg_path, total_mb);
    eprintln!("[MEMORY]   Detailed breakdown:");
    eprintln!("[MEMORY]     references: {:.2} MB ({} entries)", references_mb, references_count);
    eprintln!("[MEMORY]     file_use_defs: {:.2} MB ({} files)", file_use_defs_mb, file_use_defs_count);
    eprintln!("[MEMORY]     file_mods: {:.2} MB ({} files)", file_mods_mb, file_mods_count);
    eprintln!("[MEMORY]     files (MappedFiles): {:.2} MB", files_mb);
    eprintln!("[MEMORY]     def_info: {:.2} MB ({} definitions)", def_info_mb, def_info_count);
    eprintln!("[MEMORY]     compiler_autocomplete_info: {:.2} MB", compiler_autocomplete_info_mb);
    if compiler_autocomplete_info_mb > 0.01 {
        log_compiler_autocomplete_info_breakdown(&symbols.compiler_autocomplete_info, "[MEMORY]    ");
    }
    eprintln!("[MEMORY]     cursor_context: {:.2} MB", cursor_context_mb);
}

/// Logs memory usage for entire symbols_map using allocative
pub fn log_symbols_map_total<'a, I>(symbols_map_iter: I) -> f64
where
    I: Iterator<Item = (&'a PathBuf, &'a Symbols)>,
{
    let mut total_bytes: usize = 0;
    let mut count = 0;

    for (path, symbols) in symbols_map_iter {
        let bytes = (symbols).deep_size_of();
        total_bytes += bytes;
        count += 1;

        // Also log individual package for detailed view
        log_symbols_memory(&path.display().to_string(), symbols);
    }

    let total_mb = total_bytes as f64 / BYTES_PER_MB;
    eprintln!(
        "[MEMORY] symbols_map total: {:.2} MB across {} package(s)",
        total_mb, count
    );

    total_mb
}

/// Logs memory usage for CachedPackages using allocative
pub fn log_cached_packages_memory(cached: &CachedPackages) {
    let mut total_pkg_info_bytes = 0usize;
    let mut total_compiled_dep_pkgs_bytes = 0usize;

    // Log individual package sizes and Arc reference counts
    for (path, opt_info) in &cached.pkg_info {
        if let Some(info) = opt_info {
            // Measure each Arc component individually and sum them
            let program_bytes = info.program.as_ref().deep_size_of();
            let deps_bytes = info.deps.as_ref().deep_size_of();
            let deps_symbols_bytes = info.deps_symbols_data.as_ref().deep_size_of();
            let file_paths_bytes = info.file_paths.as_ref().deep_size_of();
            let user_file_hashes_bytes = info.user_file_hashes.as_ref().deep_size_of();

            // Manual estimation for external types
            let lsp_diags_bytes = estimate_diagnostics_size(info.lsp_diags.as_ref());

            let info_bytes = program_bytes + deps_bytes + deps_symbols_bytes + file_paths_bytes + user_file_hashes_bytes + lsp_diags_bytes;
            let info_mb = info_bytes as f64 / BYTES_PER_MB;
            total_pkg_info_bytes += info_bytes;

            let program_refs = Arc::strong_count(&info.program);
            let deps_symbols_refs = Arc::strong_count(&info.deps_symbols_data);
            let deps_refs = Arc::strong_count(&info.deps);

            eprintln!(
                "[MEMORY]   pkg_info[\"{}\"]: {:.2} MB (program: {} refs, deps_symbols_data: {} refs, deps: {} refs)",
                path.display(),
                info_mb,
                program_refs,
                deps_symbols_refs,
                deps_refs
            );

            // Log detailed breakdown of AST memory
            log_pkg_info_breakdown(&path.display().to_string(), info);
        }
    }

    for (path, arc_prog) in &cached.compiled_dep_pkgs {
        let prog_bytes = arc_prog.as_ref().deep_size_of();
        let prog_mb = prog_bytes as f64 / BYTES_PER_MB;
        total_compiled_dep_pkgs_bytes += prog_bytes;

        let refs = Arc::strong_count(arc_prog);
        let module_count = arc_prog.iter().count();
        eprintln!(
            "[MEMORY]   compiled_dep_pkgs[\"{}\"]: {:.2} MB ({} refs, {} modules)",
            path.display(),
            prog_mb,
            refs,
            module_count
        );
    }

    let total_bytes = total_pkg_info_bytes + total_compiled_dep_pkgs_bytes;
    let total_mb = total_bytes as f64 / BYTES_PER_MB;
    eprintln!(
        "[MEMORY] pkg_deps TOTAL: {:.2} MB ({} pkg_info entries = {:.2} MB, {} compiled_dep_pkgs entries = {:.2} MB)",
        total_mb,
        cached.pkg_info.len(),
        total_pkg_info_bytes as f64 / BYTES_PER_MB,
        cached.compiled_dep_pkgs.len(),
        total_compiled_dep_pkgs_bytes as f64 / BYTES_PER_MB
    );
}
