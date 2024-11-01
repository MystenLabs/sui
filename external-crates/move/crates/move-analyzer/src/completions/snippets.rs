// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// Snippets auto-completion for various language elements, such as `init` function
// or structs representing objects.

use crate::{
    completions::utils::mod_defs,
    symbols::{CursorContext, CursorDefinition, DefInfo, Symbols},
};
use lsp_types::{CompletionItem, CompletionItemKind, Documentation, InsertTextFormat, Position};
use move_command_line_common::files::FileHash;
use move_compiler::{expansion::ast::Visibility, parser::ast::Ability_, shared::Identifier};
use move_ir_types::location::Loc;
use move_symbol_pool::Symbol;

use std::path::Path;

/// Checks if the cursor is at the opening brace of a struct definition and returns
/// auto-completion of this struct into an object if the struct has the `key` ability.
pub fn object_completion(
    symbols: &Symbols,
    cursor: &CursorContext,
) -> (Option<CompletionItem>, bool) {
    let mut completion_finalized = false;
    // look for a struct definition on the line that contains `{`, check its abilities,
    // and do auto-completion if `key` ability is present
    let Some(CursorDefinition::Struct(sname)) = &cursor.defn_name else {
        return (None, completion_finalized);
    };
    completion_finalized = true;
    let Some(mod_ident) = cursor.module else {
        return (None, completion_finalized);
    };
    let Some(mod_defs) = mod_defs(symbols, &mod_ident.value) else {
        return (None, completion_finalized);
    };
    let Some(struct_def) = mod_defs.structs.get(&sname.value()) else {
        return (None, completion_finalized);
    };

    let Some(DefInfo::Struct(_, _, _, _, abilities, ..)) =
        symbols.def_info.get(&struct_def.name_loc)
    else {
        return (None, completion_finalized);
    };

    if !abilities.has_ability_(Ability_::Key) {
        return (None, completion_finalized);
    }
    let obj_snippet = "\n\tid: UID,\n\t$1\n".to_string();
    let init_completion = CompletionItem {
        label: "id: UID".to_string(),
        kind: Some(CompletionItemKind::SNIPPET),
        documentation: Some(Documentation::String("Object snippet".to_string())),
        insert_text: Some(obj_snippet),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    };
    (Some(init_completion), completion_finalized)
}

/// Auto-completion for `init` function snippet.
pub fn init_completion(
    symbols: &Symbols,
    use_fpath: &Path,
    buffer: &str,
    position: &Position,
) -> (Vec<CompletionItem>, bool) {
    let mut completions = vec![];
    let mut completion_finalized = false;

    let strings = preceding_strings(buffer, position);

    if strings.is_empty() {
        return (completions, completion_finalized);
    }

    // try to auto-complete init function declararation - get the last string
    // and see if it represents the beginning of init function declaration
    const INIT_FN_NAME: &str = "init";
    let (n, use_col) = strings.last().unwrap();
    for u in symbols.line_uses(use_fpath, position.line) {
        if *use_col >= u.col_start() && *use_col <= u.col_end() {
            let def_loc = u.def_loc();
            let Some(use_file_mod_definition) = symbols.file_mods.get(use_fpath) else {
                break;
            };
            let Some(use_file_mod_def) = use_file_mod_definition.first() else {
                break;
            };
            if is_definition(
                symbols,
                position.line,
                u.col_start(),
                use_file_mod_def.fhash(),
                def_loc,
            ) {
                // since it's a definition, there is no point in trying to suggest a name
                // if one is about to create a fresh identifier
                completion_finalized = true;
            }
            let Some(def_info) = symbols.def_info(&def_loc) else {
                break;
            };
            let DefInfo::Function(mod_ident, v, ..) = def_info else {
                // not a function
                break;
            };
            if !INIT_FN_NAME.starts_with(n) {
                // starting to type "init"
                break;
            }
            if !matches!(v, Visibility::Internal) {
                // private (otherwise perhaps it's "init_something")
                break;
            }

            // get module info containing the init function
            let Some(mdef) = symbols.mod_defs(&u.def_loc().file_hash(), *mod_ident) else {
                break;
            };

            if mdef.functions().contains_key(&(INIT_FN_NAME.into())) {
                // already has init function
                break;
            }

            let sui_ctx_arg = "ctx: &mut TxContext";

            // decide on the list of parameters depending on whether a module containing
            // the init function has a struct thats an one-time-witness candidate struct
            let otw_candidate = Symbol::from(mod_ident.module.value().to_uppercase());
            let init_snippet = if mdef.structs().contains_key(&otw_candidate) {
                format!("{INIT_FN_NAME}(${{1:witness}}: {otw_candidate}, {sui_ctx_arg}) {{\n\t${{2:}}\n}}\n")
            } else {
                format!("{INIT_FN_NAME}({sui_ctx_arg}) {{\n\t${{1:}}\n}}\n")
            };

            let init_completion = CompletionItem {
                label: INIT_FN_NAME.to_string(),
                kind: Some(CompletionItemKind::SNIPPET),
                documentation: Some(Documentation::String(
                    "Module initializer snippet".to_string(),
                )),
                insert_text: Some(init_snippet),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            };
            completions.push(init_completion);
            break;
        }
    }

    (completions, completion_finalized)
}

/// Finds white-space separated strings on the line containing auto-completion request and their
/// locations.
fn preceding_strings(buffer: &str, position: &Position) -> Vec<(String, u32)> {
    let mut strings = vec![];
    let line = match buffer.lines().nth(position.line as usize) {
        Some(line) => line,
        None => return strings, // Our buffer does not contain the line, and so must be out of date.
    };

    let mut chars = line.chars();
    let mut cur_col = 0;
    let mut cur_str_start = 0;
    let mut cur_str = "".to_string();
    while cur_col <= position.character {
        let Some(c) = chars.next() else {
            return strings;
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
    strings
}

/// Checks if a use at a given position is also a definition.
fn is_definition(
    symbols: &Symbols,
    use_line: u32,
    use_col: u32,
    use_fhash: FileHash,
    def_loc: Loc,
) -> bool {
    if let Some(use_loc) = symbols
        .files
        .line_char_offset_to_loc_opt(use_fhash, use_line, use_col)
    {
        // TODO: is overlapping better?
        def_loc.contains(&use_loc)
    } else {
        false
    }
}
