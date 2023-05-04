// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod ast;
pub mod comments;
pub(crate) mod filter;
pub mod keywords;
pub mod lexer;
pub(crate) mod merge_spec_modules;
pub mod syntax;

use crate::{
    attr_derivation,
    diagnostics::{codes::Severity, Diagnostics, FilesSourceText},
    parser::{self, ast::PackageDefinition, syntax::parse_file_string},
    shared::{CompilationEnv, IndexedPackagePath, NamedAddressMaps},
};
use anyhow::anyhow;
use comments::*;
use move_command_line_common::files::{find_move_filenames, FileHash};
use move_symbol_pool::Symbol;
use std::{
    collections::{BTreeSet, HashMap},
    fs::File,
    io::Read,
};

pub(crate) fn parse_program(
    compilation_env: &mut CompilationEnv,
    named_address_maps: NamedAddressMaps,
    targets: Vec<IndexedPackagePath>,
    deps: Vec<IndexedPackagePath>,
) -> anyhow::Result<(
    FilesSourceText,
    Result<(parser::ast::Program, CommentMap), Diagnostics>,
)> {
    fn find_move_filenames_with_address_mapping(
        paths_with_mapping: Vec<IndexedPackagePath>,
    ) -> anyhow::Result<Vec<IndexedPackagePath>> {
        let mut res = vec![];
        for IndexedPackagePath {
            package,
            path,
            named_address_map: named_address_mapping,
        } in paths_with_mapping
        {
            res.extend(
                find_move_filenames(&[path.as_str()], true)?
                    .into_iter()
                    .map(|s| IndexedPackagePath {
                        package,
                        path: Symbol::from(s),
                        named_address_map: named_address_mapping,
                    }),
            );
        }
        // sort the filenames so errors about redefinitions, or other inter-file conflicts, are
        // deterministic
        res.sort_by(|p1, p2| p1.path.cmp(&p2.path));
        Ok(res)
    }

    let targets = find_move_filenames_with_address_mapping(targets)?;
    let mut deps = find_move_filenames_with_address_mapping(deps)?;
    ensure_targets_deps_dont_intersect(compilation_env, &targets, &mut deps)?;
    let mut files: FilesSourceText = HashMap::new();
    let mut source_definitions = Vec::new();
    let mut source_comments = CommentMap::new();
    let mut lib_definitions = Vec::new();
    let mut diags: Diagnostics = Diagnostics::new();

    for IndexedPackagePath {
        package,
        path,
        named_address_map,
    } in targets
    {
        let (defs, comments, ds, file_hash) = parse_file(compilation_env, &mut files, path)?;
        source_definitions.extend(defs.into_iter().map(|def| PackageDefinition {
            package,
            named_address_map,
            def,
        }));
        source_comments.insert(file_hash, comments);
        diags.extend(ds);
    }

    for IndexedPackagePath {
        package,
        path,
        named_address_map,
    } in deps
    {
        let (defs, _, ds, _) = parse_file(compilation_env, &mut files, path)?;
        lib_definitions.extend(defs.into_iter().map(|def| PackageDefinition {
            package,
            named_address_map,
            def,
        }));
        diags.extend(ds);
    }

    // TODO fix this so it works likes other passes and the handling of errors is done outside of
    // this function
    let env_result = compilation_env.check_diags_at_or_above_severity(Severity::BlockingError);
    if let Err(env_diags) = env_result {
        diags.extend(env_diags)
    }

    // Run attribute expansion on all source definitions, passing in the matching named address map.
    for PackageDefinition {
        named_address_map: idx,
        def,
        ..
    } in source_definitions.iter_mut()
    {
        attr_derivation::derive_from_attributes(compilation_env, named_address_maps.get(*idx), def);
    }

    let res = if diags.is_empty() {
        let pprog = parser::ast::Program {
            named_address_maps,
            source_definitions,
            lib_definitions,
        };
        Ok((pprog, source_comments))
    } else {
        Err(diags)
    };
    Ok((files, res))
}

fn ensure_targets_deps_dont_intersect(
    compilation_env: &CompilationEnv,
    targets: &[IndexedPackagePath],
    deps: &mut Vec<IndexedPackagePath>,
) -> anyhow::Result<()> {
    /// Canonicalize a file path.
    fn canonicalize(path: &Symbol) -> String {
        let p = path.as_str();
        match std::fs::canonicalize(p) {
            Ok(s) => s.to_string_lossy().to_string(),
            Err(_) => p.to_owned(),
        }
    }
    let target_set = targets
        .iter()
        .map(|p| canonicalize(&p.path))
        .collect::<BTreeSet<_>>();
    let dep_set = deps
        .iter()
        .map(|p| canonicalize(&p.path))
        .collect::<BTreeSet<_>>();
    let intersection = target_set.intersection(&dep_set).collect::<Vec<_>>();
    if intersection.is_empty() {
        return Ok(());
    }
    if compilation_env.flags().sources_shadow_deps() {
        deps.retain(|p| !intersection.contains(&&canonicalize(&p.path)));
        return Ok(());
    }
    let all_files = intersection
        .into_iter()
        .map(|s| format!("    {}", s))
        .collect::<Vec<_>>()
        .join("\n");
    Err(anyhow!(
        "The following files were marked as both targets and dependencies:\n{}",
        all_files
    ))
}

fn parse_file(
    compilation_env: &mut CompilationEnv,
    files: &mut FilesSourceText,
    fname: Symbol,
) -> anyhow::Result<(
    Vec<parser::ast::Definition>,
    MatchedFileCommentMap,
    Diagnostics,
    FileHash,
)> {
    let mut diags = Diagnostics::new();
    let mut f = File::open(fname.as_str())
        .map_err(|err| std::io::Error::new(err.kind(), format!("{}: {}", err, fname)))?;
    let mut source_buffer = String::new();
    f.read_to_string(&mut source_buffer)?;
    let file_hash = FileHash::new(&source_buffer);
    let buffer = match verify_string(file_hash, &source_buffer) {
        Err(ds) => {
            diags.extend(ds);
            files.insert(file_hash, (fname, source_buffer));
            return Ok((vec![], MatchedFileCommentMap::new(), diags, file_hash));
        }
        Ok(()) => &source_buffer,
    };
    let (defs, comments) = match parse_file_string(compilation_env, file_hash, buffer) {
        Ok(defs_and_comments) => defs_and_comments,
        Err(ds) => {
            diags.extend(ds);
            (vec![], MatchedFileCommentMap::new())
        }
    };
    files.insert(file_hash, (fname, source_buffer));
    Ok((defs, comments, diags, file_hash))
}
