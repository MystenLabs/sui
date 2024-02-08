// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    context::Context,
    symbols::{DefInfo, Symbols},
};
use lsp_server::Request;
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionParams, Documentation, InsertTextFormat, Position,
};
use move_command_line_common::files::FileHash;
use move_compiler::{
    editions::Edition,
    expansion::ast::Visibility,
    parser::{
        keywords::{BUILTINS, CONTEXTUAL_KEYWORDS, KEYWORDS, PRIMITIVE_TYPES},
        lexer::{Lexer, Tok},
    },
    shared::Identifier,
};
use move_symbol_pool::Symbol;
use std::{collections::HashSet, path::PathBuf};
use vfs::VfsPath;

/// Constructs an `lsp_types::CompletionItem` with the given `label` and `kind`.
fn completion_item(label: &str, kind: CompletionItemKind) -> CompletionItem {
    CompletionItem {
        label: label.to_owned(),
        kind: Some(kind),
        ..Default::default()
    }
}

/// Return a list of completion items corresponding to each one of Move's keywords.
///
/// Currently, this does not filter keywords out based on whether they are valid at the completion
/// request's cursor position, but in the future it ought to. For example, this function returns
/// all specification language keywords, but in the future it should be modified to only do so
/// within a spec block.
fn keywords() -> Vec<CompletionItem> {
    KEYWORDS
        .iter()
        .chain(CONTEXTUAL_KEYWORDS.iter())
        .chain(PRIMITIVE_TYPES.iter())
        .map(|label| {
            let kind = if label == &"copy" || label == &"move" {
                CompletionItemKind::Operator
            } else {
                CompletionItemKind::Keyword
            };
            completion_item(label, kind)
        })
        .collect()
}

/// Return a list of completion items of Move's primitive types
fn primitive_types() -> Vec<CompletionItem> {
    PRIMITIVE_TYPES
        .iter()
        .map(|label| completion_item(label, CompletionItemKind::Keyword))
        .collect()
}

/// Return a list of completion items corresponding to each one of Move's builtin functions.
fn builtins() -> Vec<CompletionItem> {
    BUILTINS
        .iter()
        .map(|label| completion_item(label, CompletionItemKind::Function))
        .collect()
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
fn identifiers(buffer: &str, symbols: &Symbols, path: &PathBuf) -> Vec<CompletionItem> {
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

    let mods_opt = symbols.file_mods().get(path);

    // The completion item kind "text" indicates that the item is based on simple textual matching,
    // not any deeper semantic analysis.
    ids.iter()
        .map(|label| {
            if let Some(mods) = mods_opt {
                if mods
                    .iter()
                    .any(|m| m.functions().contains_key(&Symbol::from(*label)))
                {
                    completion_item(label, CompletionItemKind::Function)
                } else {
                    completion_item(label, CompletionItemKind::Text)
                }
            } else {
                completion_item(label, CompletionItemKind::Text)
            }
        })
        .collect()
}

/// Returns the token corresponding to the "trigger character" that precedes the user's cursor,
/// if it is one of `.`, `:`, or `::`. Otherwise, returns `None`.
fn get_cursor_token(buffer: &str, position: &Position) -> Option<Tok> {
    // If the cursor is at the start of a new line, it cannot be preceded by a trigger character.
    if position.character == 0 {
        return None;
    }

    let line = match buffer.lines().nth(position.line as usize) {
        Some(line) => line,
        None => return None, // Our buffer does not contain the line, and so must be out of date.
    };
    match line.chars().nth(position.character as usize - 1) {
        Some('.') => Some(Tok::Period),
        Some(':') => {
            if position.character > 1
                && line.chars().nth(position.character as usize - 2) == Some(':')
            {
                Some(Tok::ColonColon)
            } else {
                Some(Tok::Colon)
            }
        }
        _ => None,
    }
}

/// Handle context-specific auto-completion requests.
fn context_specific(
    symbols: &Symbols,
    use_fpath: &PathBuf,
    buffer: &str,
    position: &Position,
) -> Vec<CompletionItem> {
    let mut completions = vec![];
    // If the cursor is at the start of a new line, it cannot be preceded by a trigger character.
    if position.character == 0 {
        return completions;
    }
    let line = match buffer.lines().nth(position.line as usize) {
        Some(line) => line,
        None => return completions, // Our buffer does not contain the line, and so must be out of date.
    };

    // find white-space separated strings on the line containing auto-completion request
    // and their locations
    let mut strings = vec![];
    let mut chars = line.chars();
    let mut cur_col = 0;
    let mut cur_str_start = 0;
    let mut cur_str = "".to_string();
    while cur_col < position.character {
        let Some(c) = chars.next() else {
            return completions;
        };
        if c == ' ' || c == '\t' {
            if !cur_str.is_empty() {
                // finish an already started string
                strings.push((cur_str, cur_str_start));
                cur_str = "".to_string();
            }
        } else {
            if cur_str.is_empty() {
                // start a new string
                cur_str_start = cur_col;
            }
            cur_str.push(c);
        }

        cur_col += c.len_utf8() as u32;
    }
    if !cur_str.is_empty() {
        // finish the last string
        strings.push((cur_str, cur_str_start));
    }

    if strings.is_empty() {
        return completions;
    }

    // at this point only try to auto-complete init function declararation - get the last string
    // and see if it represents the beginning of init function declaration
    let (n, use_col) = strings.last().unwrap();
    for u in symbols.line_uses(use_fpath, position.line) {
        if *use_col >= u.col_start() && *use_col <= u.col_end() {
            let Some(def_info) = symbols.def_info(&u.def_loc()) else {
                break;
            };
            let DefInfo::Function(mod_ident, v, _, _, _, _, _) = def_info else {
                // not a function
                break;
            };
            if !"init".starts_with(n) {
                // starting to type "init"
                break;
            }
            if !matches!(v, Visibility::Internal) {
                // private (otherwise perhaps it's "init_something")
                break;
            }

            // get module info containing the init function
            let Some(mdef) = symbols.mod_defs(&u.def_loc().fhash(), *mod_ident) else {
                break;
            };
            // TODO: remove sui::tx_context once auto-imports are enabled
            let sui_ctx_arg = "ctx: &mut sui::tx_context::TxContext";

            // decide on the list of parameters depending on whether a module containing
            // the init function has a struct thats an one-time-witness candidate struct
            let otw_candidate = Symbol::from(mod_ident.module.value().to_uppercase());
            let init_snippet = if mdef.structs().contains_key(&otw_candidate) {
                format!("init(${{1:witness}}: {otw_candidate}, {sui_ctx_arg}) {{\n\t${{2:}}\n}}\n")
            } else {
                format!("init({sui_ctx_arg}) {{\n\t${{1:}}\n}}\n")
            };

            let init_completion = CompletionItem {
                label: "init".to_string(),
                kind: Some(CompletionItemKind::Snippet),
                documentation: Some(Documentation::String(
                    "Snippet for module initializer".to_string(),
                )),
                insert_text: Some(init_snippet),
                insert_text_format: Some(InsertTextFormat::Snippet),
                ..Default::default()
            };
            completions.push(init_completion);
            break;
        }
    }
    completions
}

/// Sends the given connection a response to a completion request.
///
/// The completions returned depend upon where the user's cursor is positioned.
pub fn on_completion_request(
    context: &Context,
    request: &Request,
    ide_files: VfsPath,
    symbols: &Symbols,
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
    let mut buffer = String::new();
    if let Ok(mut f) = ide_files.join(path.to_string_lossy()).unwrap().open_file() {
        if f.read_to_string(&mut buffer).is_err() {
            eprintln!(
                "Could not read '{:?}' when handling completion request",
                path
            );
        }
    }

    let mut items = vec![];
    if !buffer.is_empty() {
        let cursor = get_cursor_token(buffer.as_str(), &parameters.text_document_position.position);
        match cursor {
            Some(Tok::Colon) => {
                items.extend_from_slice(&primitive_types());
            }
            Some(Tok::Period) | Some(Tok::ColonColon) => {
                // `.` or `::` must be followed by identifiers, which are added to the completion items
                // below.
            }
            _ => {
                // If the user's cursor is positioned anywhere other than following a `.`, `:`, or `::`,
                // offer them context-specific autocompletion items as well as
                // Move's keywords, operators, and builtins.
                items.extend_from_slice(&context_specific(
                    symbols,
                    &path,
                    buffer.as_str(),
                    &parameters.text_document_position.position,
                ));
                // If the user's cursor is positioned anywhere other than following a `.`, `:`, or `::`,
                // offer them Move's keywords, operators, and builtins as completion items.
                items.extend_from_slice(&keywords());
                items.extend_from_slice(&builtins());
            }
        }
        let identifiers = identifiers(buffer.as_str(), symbols, &path);
        items.extend_from_slice(&identifiers);
    } else {
        // no file content
        items.extend_from_slice(&keywords());
        items.extend_from_slice(&builtins());
    }

    let result = serde_json::to_value(items).expect("could not serialize completion response");
    eprintln!("about to send completion response");
    let response = lsp_server::Response::new_ok(request.id.clone(), result);
    if let Err(err) = context
        .connection
        .sender
        .send(lsp_server::Message::Response(response))
    {
        eprintln!("could not send completion response: {:?}", err);
    }
}
