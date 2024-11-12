// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod ast;
pub mod comments;
pub(crate) mod filter;
pub mod keywords;
pub mod lexer;
pub(crate) mod syntax;
mod token_set;
pub(crate) mod verification_attribute_filter;

use crate::{
    parser::{self, ast::PackageDefinition, syntax::parse_file_string},
    shared::{
        files::MappedFiles, CompilationEnv, IndexedVfsPackagePath, NamedAddressMapIndex,
        NamedAddressMaps,
    },
};
use anyhow::anyhow;
use comments::*;
use move_command_line_common::files::FileHash;
use move_symbol_pool::Symbol;
use rayon::iter::*;
use std::{collections::BTreeSet, sync::Arc};
use vfs::VfsPath;

struct ParsedFile {
    fname: Symbol,
    defs: Vec<parser::ast::Definition>,
    comments: MatchedFileCommentMap,
    hash: FileHash,
    text: Arc<str>,
}

struct ParsedPackageFile {
    is_dep: bool,
    package: Option<Symbol>,
    named_address_map: NamedAddressMapIndex,
    file: ParsedFile,
}

/// Parses program's targets and dependencies, both of which are read from different virtual file
/// systems (vfs and deps_out_vfs, respectively).
pub(crate) fn parse_program(
    compilation_env: &CompilationEnv,
    named_address_maps: NamedAddressMaps,
    mut targets: Vec<IndexedVfsPackagePath>,
    mut deps: Vec<IndexedVfsPackagePath>,
) -> anyhow::Result<(MappedFiles, parser::ast::Program, CommentMap)> {
    // sort the filenames so errors about redefinitions, or other inter-file conflicts, are
    // deterministic
    targets.sort_by(|p1, p2| p1.path.as_str().cmp(p2.path.as_str()));
    deps.sort_by(|p1, p2| p1.path.as_str().cmp(p2.path.as_str()));
    ensure_targets_deps_dont_intersect(compilation_env, &targets, &mut deps)?;
    let mut files: MappedFiles = MappedFiles::empty();
    let mut source_definitions = Vec::new();
    let mut source_comments = CommentMap::new();
    let mut lib_definitions = Vec::new();

    let parsed = targets
        .into_par_iter()
        .map(|p| (false, p))
        .chain(deps.into_par_iter().map(|p| (true, p)))
        .map(|(is_dep, package_path)| {
            let IndexedVfsPackagePath {
                package,
                path,
                named_address_map,
            } = package_path;
            let file = parse_file(&path, compilation_env, package)?;
            Ok(ParsedPackageFile {
                is_dep,
                package,
                named_address_map,
                file,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    for parsed_package_file in parsed {
        let ParsedPackageFile {
            is_dep,
            package,
            named_address_map,
            file,
        } = parsed_package_file;
        let ParsedFile {
            fname,
            defs,
            comments,
            hash,
            text,
        } = file;
        files.add(hash, fname, text);
        source_comments.insert(hash, comments);
        let defs = defs.into_iter().map(|def| PackageDefinition {
            package,
            named_address_map,
            def,
        });
        if is_dep {
            lib_definitions.extend(defs);
        } else {
            source_definitions.extend(defs)
        }
    }

    let pprog = parser::ast::Program {
        named_address_maps,
        source_definitions,
        lib_definitions,
    };
    Ok((files, pprog, source_comments))
}

fn ensure_targets_deps_dont_intersect(
    compilation_env: &CompilationEnv,
    targets: &[IndexedVfsPackagePath],
    deps: &mut Vec<IndexedVfsPackagePath>,
) -> anyhow::Result<()> {
    let target_set = targets
        .iter()
        .map(|p| p.path.as_str().to_owned())
        .collect::<BTreeSet<_>>();
    let dep_set = deps
        .iter()
        .map(|p| p.path.as_str().to_owned())
        .collect::<BTreeSet<_>>();
    let intersection = target_set.intersection(&dep_set).collect::<Vec<_>>();
    if intersection.is_empty() {
        return Ok(());
    }
    if compilation_env.flags().sources_shadow_deps() {
        deps.retain(|p| !intersection.contains(&&p.path.as_str().to_owned()));
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
    path: &VfsPath,
    compilation_env: &CompilationEnv,
    package: Option<Symbol>,
) -> anyhow::Result<ParsedFile> {
    let mut source_buffer = String::new();
    path.open_file()?.read_to_string(&mut source_buffer)?;
    let file_hash = FileHash::new(&source_buffer);
    let fname = Symbol::from(path.as_str());
    let source_str = Arc::from(source_buffer);
    let reporter = compilation_env.diagnostic_reporter_at_top_level();
    if let Err(ds) = verify_string(file_hash, &source_str) {
        reporter.add_diags(ds);
        return Ok(ParsedFile {
            fname,
            defs: vec![],
            comments: MatchedFileCommentMap::new(),
            hash: file_hash,
            text: source_str,
        });
    }
    let (defs, comments) = match parse_file_string(compilation_env, file_hash, &source_str, package)
    {
        Ok(defs_and_comments) => defs_and_comments,
        Err(ds) => {
            reporter.add_diags(ds);
            (vec![], MatchedFileCommentMap::new())
        }
    };
    Ok(ParsedFile {
        fname,
        defs,
        comments,
        hash: file_hash,
        text: source_str,
    })
}
