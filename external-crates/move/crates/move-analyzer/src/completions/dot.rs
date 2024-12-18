// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// Auto-completion for the dot operator, e.g., `struct.` or `value.foo()`.`

use crate::{
    completions::utils::{call_completion_item, mod_defs},
    symbols::{type_to_ide_string, DefInfo, FunType, Symbols},
    utils::lsp_position_to_loc,
};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionItemLabelDetails, InsertTextFormat, Position,
};
use move_compiler::{
    expansion::ast::ModuleIdent_,
    shared::{ide::AutocompleteMethod, Identifier},
};
use move_symbol_pool::Symbol;

use std::path::Path;

/// Handle "dot" auto-completion at a given position.
pub fn dot_completions(
    symbols: &Symbols,
    use_fpath: &Path,
    position: &Position,
) -> (Vec<CompletionItem>, bool) {
    let mut completions = vec![];
    let mut completion_finalized = false;
    let Some(fhash) = symbols.file_hash(use_fpath) else {
        eprintln!("no dot completions due to missing file");
        return (completions, completion_finalized);
    };
    let Some(loc) = lsp_position_to_loc(&symbols.files, fhash, position) else {
        eprintln!("no dot completions due to missing loc");
        return (completions, completion_finalized);
    };
    let Some(info) = symbols.compiler_info.get_autocomplete_info(fhash, &loc) else {
        return (completions, completion_finalized);
    };
    // we found auto-completion info, so don't look for any more completions
    // even if if it does not contain any
    completion_finalized = true;
    for AutocompleteMethod {
        method_name,
        target_function: (mod_ident, function_name),
    } in &info.methods
    {
        let call_completion = if let Some(DefInfo::Function(
            ..,
            fun_type,
            _,
            type_args,
            arg_names,
            arg_types,
            ret_type,
            _,
        )) = fun_def_info(symbols, mod_ident.value, function_name.value())
        {
            call_completion_item(
                &mod_ident.value,
                matches!(fun_type, FunType::Macro),
                Some(method_name),
                &function_name.value(),
                type_args,
                arg_names,
                arg_types,
                ret_type,
                /* inside_use */ false,
            )
        } else {
            // this shouldn't really happen as we should be able to get
            // `DefInfo` for a function but if for some reason we cannot,
            // let's generate simpler autotompletion value
            eprintln!("incomplete dot item");
            CompletionItem {
                label: format!("{method_name}()"),
                kind: Some(CompletionItemKind::METHOD),
                insert_text: Some(method_name.to_string()),
                insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                ..Default::default()
            }
        };
        completions.push(call_completion);
    }
    for (n, t) in &info.fields {
        let label_details = Some(CompletionItemLabelDetails {
            detail: None,
            description: Some(type_to_ide_string(t, /* verbose */ false)),
        });
        let init_completion = CompletionItem {
            label: n.to_string(),
            label_details,
            kind: Some(CompletionItemKind::FIELD),
            insert_text: Some(n.to_string()),
            insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
            ..Default::default()
        };
        completions.push(init_completion);
    }

    (completions, completion_finalized)
}

/// Get the `DefInfo` for a function definition.
fn fun_def_info(symbols: &Symbols, mod_ident: ModuleIdent_, name: Symbol) -> Option<&DefInfo> {
    let Some(mod_defs) = mod_defs(symbols, &mod_ident) else {
        return None;
    };

    let Some(fdef) = mod_defs.functions.get(&name) else {
        return None;
    };
    symbols.def_info(&fdef.name_loc)
}
