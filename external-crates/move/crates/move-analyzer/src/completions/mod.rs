// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    completions::{
        dot::dot_completions,
        name_chain::{name_chain_completions, use_decl_completions},
        snippets::{init_completion, object_completion},
        utils::{completion_item, PRIMITIVE_TYPE_COMPLETIONS},
    },
    context::Context,
    symbols::{self, CursorContext, PrecomputedPkgInfo, SymbolicatorRunner, Symbols},
};
use lsp_server::Request;
use lsp_types::{CompletionItem, CompletionItemKind, CompletionParams, Position};
use move_command_line_common::files::FileHash;
use move_compiler::{
    editions::Edition,
    linters::LintLevel,
    parser::{
        keywords::{BUILTINS, CONTEXTUAL_KEYWORDS, KEYWORDS, PRIMITIVE_TYPES},
        lexer::{Lexer, Tok},
    },
};
use move_symbol_pool::Symbol;

use once_cell::sync::Lazy;

use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use vfs::VfsPath;

mod dot;
mod name_chain;
mod snippets;
mod utils;

/// List of completion items corresponding to each one of Move's keywords.
///
/// Currently, this does not filter keywords out based on whether they are valid at the completion
/// request's cursor position, but in the future it ought to. For example, this function returns
/// all specification language keywords, but in the future it should be modified to only do so
/// within a spec block.
static KEYWORD_COMPLETIONS: Lazy<Vec<CompletionItem>> = Lazy::new(|| {
    let mut keywords = KEYWORDS
        .iter()
        .chain(CONTEXTUAL_KEYWORDS.iter())
        .chain(PRIMITIVE_TYPES.iter())
        .map(|label| {
            let kind = if label == &"copy" || label == &"move" {
                CompletionItemKind::OPERATOR
            } else {
                CompletionItemKind::KEYWORD
            };
            completion_item(label, kind)
        })
        .collect::<Vec<_>>();
    keywords.extend(PRIMITIVE_TYPE_COMPLETIONS.clone());
    keywords
});

/// List of completion items corresponding to each one of Move's builtin functions.
static BUILTIN_COMPLETIONS: Lazy<Vec<CompletionItem>> = Lazy::new(|| {
    BUILTINS
        .iter()
        .map(|label| completion_item(label, CompletionItemKind::FUNCTION))
        .collect()
});

/// Sends the given connection a response to a completion request.
///
/// The completions returned depend upon where the user's cursor is positioned.
pub fn on_completion_request(
    context: &Context,
    request: &Request,
    ide_files_root: VfsPath,
    pkg_dependencies: Arc<Mutex<BTreeMap<PathBuf, PrecomputedPkgInfo>>>,
) {
    eprintln!("handling completion request");
    let parameters = serde_json::from_value::<CompletionParams>(request.params.clone())
        .expect("could not deserialize completion request");

    let path = parameters
        .text_document_position
        .text_document
        .uri
        .to_file_path()
        .unwrap();

    let mut pos = parameters.text_document_position.position;
    if pos.character != 0 {
        // adjust column to be at the character that has just been inserted rather than right after
        // it (unless we are at the very first column)
        pos = Position::new(pos.line, pos.character - 1);
    }
    let completions =
        completions(context, ide_files_root, pkg_dependencies, &path, pos).unwrap_or_default();
    let completions_len = completions.len();

    let result =
        serde_json::to_value(completions).expect("could not serialize completion response");
    eprintln!("about to send completion response with {completions_len} items");
    let response = lsp_server::Response::new_ok(request.id.clone(), result);
    if let Err(err) = context
        .connection
        .sender
        .send(lsp_server::Message::Response(response))
    {
        eprintln!("could not send completion response: {:?}", err);
    }
}

/// Computes a list of auto-completions for a given position in a file,
/// given the current context.
fn completions(
    context: &Context,
    ide_files_root: VfsPath,
    pkg_dependencies: Arc<Mutex<BTreeMap<PathBuf, PrecomputedPkgInfo>>>,
    path: &Path,
    pos: Position,
) -> Option<Vec<CompletionItem>> {
    let Some(pkg_path) = SymbolicatorRunner::root_dir(path) else {
        eprintln!("failed completion for {:?} (package root not found)", path);
        return None;
    };
    let symbol_map = context.symbols.lock().unwrap();
    let current_symbols = symbol_map.get(&pkg_path)?;
    Some(compute_completions(
        current_symbols,
        ide_files_root,
        pkg_dependencies,
        path,
        pos,
    ))
}

/// Computes a list of auto-completions for a given position in a file,
/// based on the current symbols.
pub fn compute_completions(
    current_symbols: &Symbols,
    ide_files_root: VfsPath,
    pkg_dependencies: Arc<Mutex<BTreeMap<PathBuf, PrecomputedPkgInfo>>>,
    path: &Path,
    pos: Position,
) -> Vec<CompletionItem> {
    compute_completions_new_symbols(ide_files_root, pkg_dependencies, path, pos)
        .unwrap_or_else(|| compute_completions_with_symbols(current_symbols, path, pos))
}

/// Computes a list of auto-completions for a given position in a file,
/// after attempting to re-compute the symbols to get the most up-to-date
/// view of the code (returns `None` if the symbols could not be re-computed).
fn compute_completions_new_symbols(
    ide_files_root: VfsPath,
    pkg_dependencies: Arc<Mutex<BTreeMap<PathBuf, PrecomputedPkgInfo>>>,
    path: &Path,
    cursor_position: Position,
) -> Option<Vec<CompletionItem>> {
    let Some(pkg_path) = SymbolicatorRunner::root_dir(path) else {
        eprintln!("failed completion for {:?} (package root not found)", path);
        return None;
    };
    let cursor_path = path.to_path_buf();
    let cursor_info = Some((&cursor_path, cursor_position));
    let (symbols, _diags) = symbols::get_symbols(
        pkg_dependencies,
        ide_files_root,
        &pkg_path,
        Some(vec![path.to_path_buf()]),
        LintLevel::None,
        cursor_info,
    )
    .ok()?;
    let symbols = symbols?;
    Some(compute_completions_with_symbols(
        &symbols,
        path,
        cursor_position,
    ))
}

/// Computes a list of auto-completions for a given position in a file
/// using the symbols provided as argument.
pub fn compute_completions_with_symbols(
    symbols: &Symbols,
    path: &Path,
    pos: Position,
) -> Vec<CompletionItem> {
    let mut completions = vec![];

    let Some(fhash) = symbols.file_hash(path) else {
        return completions;
    };
    let Some(file_id) = symbols.files.file_mapping().get(&fhash) else {
        return completions;
    };
    let Ok(file) = symbols.files.files().get(*file_id) else {
        return completions;
    };

    let file_source = file.source().clone();
    if !file_source.is_empty() {
        let completion_finalized;
        match &symbols.cursor_context {
            Some(cursor_context) => {
                eprintln!("cursor completion");
                let (cursor_completions, cursor_finalized) =
                    cursor_completion_items(symbols, path, &file_source, pos, cursor_context);
                completion_finalized = cursor_finalized;
                completions.extend(cursor_completions);
            }
            None => {
                eprintln!("non-cursor completion");
                let (no_cursor_completions, no_cursor_finalized) =
                    no_cursor_completion_items(symbols, path, &file_source, pos);
                completion_finalized = no_cursor_finalized;
                completions.extend(no_cursor_completions);
            }
        }
        if !completion_finalized {
            eprintln!("including identifiers");
            let identifiers = identifiers(&file_source, symbols, path);
            completions.extend(identifiers);
        }
    } else {
        // no file content
        completions.extend(KEYWORD_COMPLETIONS.clone());
        completions.extend(BUILTIN_COMPLETIONS.clone());
    }
    completions
}

/// Return completion items in case cursor is available plus a flag indicating
/// if we should continue searching for more completions.
fn cursor_completion_items(
    symbols: &Symbols,
    path: &Path,
    file_source: &str,
    pos: Position,
    cursor: &CursorContext,
) -> (Vec<CompletionItem>, bool) {
    let cursor_leader = get_cursor_token(file_source, &pos);
    match cursor_leader {
        // TODO: consider using `cursor.position` for this instead
        Some(Tok::Period) => dot_completions(symbols, path, &pos),
        Some(Tok::ColonColon) => {
            let mut completions = vec![];
            let mut completion_finalized = false;
            let (name_chain_completions, name_chain_finalized) =
                name_chain_completions(symbols, cursor, /* colon_colon_triggered */ true);
            completions.extend(name_chain_completions);
            completion_finalized |= name_chain_finalized;
            if !completion_finalized {
                let (use_decl_completions, use_decl_finalized) =
                    use_decl_completions(symbols, cursor);
                completions.extend(use_decl_completions);
                completion_finalized |= use_decl_finalized;
            }
            (completions, completion_finalized)
        }
        // Carve out to suggest UID for struct with key ability
        Some(Tok::LBrace) => {
            let mut completions = vec![];
            let mut completion_finalized = false;
            let (custom_completions, custom_finalized) = lbrace_cursor_completions(symbols, cursor);
            completions.extend(custom_completions);
            completion_finalized |= custom_finalized;
            if !completion_finalized {
                let (use_decl_completions, _) = use_decl_completions(symbols, cursor);
                completions.extend(use_decl_completions);
            }
            // do not offer default completions after `{` as this may get annoying
            // when simply starting a body of a function and hitting enter triggers
            // auto-completion of an essentially random identifier
            (completions, true)
        }
        // TODO: should we handle auto-completion on `:`? If we model our support after
        // rust-analyzer then it does not do this - it starts auto-completing types after the first
        // character beyond `:` is typed
        _ => {
            eprintln!("no relevant cursor leader");
            let mut completions = vec![];
            let mut completion_finalized = false;
            let (name_chain_completions, name_chain_finalized) =
                name_chain_completions(symbols, cursor, /* colon_colon_triggered */ false);
            completions.extend(name_chain_completions);
            completion_finalized |= name_chain_finalized;
            if !completion_finalized {
                if matches!(cursor_leader, Some(Tok::Colon)) {
                    // much like rust-analyzer we do not auto-complete in the middle of `::`
                    completion_finalized = true;
                } else {
                    let (use_decl_completions, use_decl_finalized) =
                        use_decl_completions(symbols, cursor);
                    completions.extend(use_decl_completions);
                    completion_finalized |= use_decl_finalized;
                }
            }
            if !completion_finalized {
                eprintln!("checking default items");
                let (default_completions, default_finalized) =
                    no_cursor_completion_items(symbols, path, file_source, pos);
                completions.extend(default_completions);
                completion_finalized |= default_finalized;
            }
            (completions, completion_finalized)
        }
    }
}

/// Returns the token corresponding to the "trigger character" if it is one of `.`, `:`, '{', or
/// `::`. Otherwise, returns `None` (position points at the potential trigger character itself).
fn get_cursor_token(buffer: &str, position: &Position) -> Option<Tok> {
    let line = match buffer.lines().nth(position.line as usize) {
        Some(line) => line,
        None => return None, // Our buffer does not contain the line, and so must be out of date.
    };
    match line.chars().nth(position.character as usize) {
        Some('.') => Some(Tok::Period),
        Some(':') => {
            if position.character > 0
                && line.chars().nth(position.character as usize - 1) == Some(':')
            {
                Some(Tok::ColonColon)
            } else {
                Some(Tok::Colon)
            }
        }
        Some('{') => Some(Tok::LBrace),
        _ => None,
    }
}

/// Handle auto-completion requests with lbrace (`{`) trigger character
/// when cursor is available.
fn lbrace_cursor_completions(
    symbols: &Symbols,
    cursor: &CursorContext,
) -> (Vec<CompletionItem>, bool) {
    let completions = vec![];
    let (completion_item_opt, completion_finalized) = object_completion(symbols, cursor);
    if let Some(completion_item) = completion_item_opt {
        return (vec![completion_item], completion_finalized);
    }
    (completions, completion_finalized)
}

/// Return completion items no cursor is available plus a flag indicating
/// if we should continue searching for more completions.
fn no_cursor_completion_items(
    symbols: &Symbols,
    path: &Path,
    file_source: &str,
    pos: Position,
) -> (Vec<CompletionItem>, bool) {
    // If the user's cursor is positioned anywhere other than following a `.`, `:`, or `::`,
    // offer them context-specific autocompletion items and, if needed,
    // Move's keywords, and builtins.
    let (mut completions, mut completion_finalized) = dot_completions(symbols, path, &pos);
    if !completion_finalized {
        let (init_completions, init_finalized) = init_completion(symbols, path, file_source, &pos);
        completions.extend(init_completions);
        completion_finalized |= init_finalized;
    }

    if !completion_finalized {
        completions.extend(KEYWORD_COMPLETIONS.clone());
        completions.extend(BUILTIN_COMPLETIONS.clone());
    }
    (completions, true)
}

/// Lexes the Move source file at the given path and returns a list of completion items
/// corresponding to the non-keyword identifiers therein.
///
/// Currently, this does not perform semantic analysis to determine whether the identifiers
/// returned are valid at the request's cursor position. However, this list of identifiers is akin
/// to what editors like Visual Studio Code would provide as completion items if this language
/// server did not initialize with a response indicating it's capable of providing completions. In
/// the future, the server should be modified to return semantically valid completion items, not
/// simple textual suggestions.
fn identifiers(buffer: &str, symbols: &Symbols, path: &Path) -> Vec<CompletionItem> {
    // TODO thread through package configs
    let mut lexer = Lexer::new(buffer, FileHash::new(buffer), Edition::LEGACY);
    if lexer.advance().is_err() {
        return vec![];
    }
    let mut ids = HashSet::new();
    while lexer.peek() != Tok::EOF {
        // Some tokens, such as "phantom", are contextual keywords that are only reserved in
        // certain contexts. Since for now this language server doesn't analyze semantic context,
        // tokens such as "phantom" are always present in keyword suggestions. To avoid displaying
        // these keywords to the user twice in the case that the token "phantom" is present in the
        // source program (once as a keyword, and once as an identifier), we filter out any
        // identifier token that has the same text as a keyword.
        if lexer.peek() == Tok::Identifier && !KEYWORDS.contains(&lexer.content()) {
            // The completion item kind "text" indicates the item is not based on any semantic
            // context of the request cursor's position.
            ids.insert(lexer.content());
        }
        if lexer.advance().is_err() {
            break;
        }
    }

    let mods_opt = symbols.file_mods.get(path);

    // The completion item kind "text" indicates that the item is based on simple textual matching,
    // not any deeper semantic analysis.
    ids.iter()
        .map(|label| {
            if let Some(mods) = mods_opt {
                if mods
                    .iter()
                    .any(|m| m.functions().contains_key(&Symbol::from(*label)))
                {
                    completion_item(label, CompletionItemKind::FUNCTION)
                } else {
                    completion_item(label, CompletionItemKind::TEXT)
                }
            } else {
                completion_item(label, CompletionItemKind::TEXT)
            }
        })
        .collect()
}
