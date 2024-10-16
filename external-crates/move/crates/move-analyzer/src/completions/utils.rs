// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::symbols::{
    mod_ident_to_ide_string, ret_type_to_ide_str, type_args_to_ide_string, type_list_to_ide_string,
    ModuleDefs, Symbols,
};
use lsp_types::{CompletionItem, CompletionItemKind, CompletionItemLabelDetails, InsertTextFormat};
use move_compiler::{
    expansion::ast::ModuleIdent_,
    naming::ast::{Type, Type_},
    parser::keywords::PRIMITIVE_TYPES,
    shared::Name,
};
use move_symbol_pool::Symbol;
use once_cell::sync::Lazy;

/// List of completion items of Move's primitive types.
pub static PRIMITIVE_TYPE_COMPLETIONS: Lazy<Vec<CompletionItem>> = Lazy::new(|| {
    let mut primitive_types = PRIMITIVE_TYPES
        .iter()
        .map(|label| completion_item(label, CompletionItemKind::KEYWORD))
        .collect::<Vec<_>>();
    primitive_types.push(completion_item("address", CompletionItemKind::KEYWORD));
    primitive_types
});

/// Get definitions for a given module.
pub fn mod_defs<'a>(symbols: &'a Symbols, mod_ident: &ModuleIdent_) -> Option<&'a ModuleDefs> {
    symbols
        .file_mods
        .values()
        .flatten()
        .find(|mdef| mdef.ident == *mod_ident)
}

/// Constructs an `lsp_types::CompletionItem` with the given `label` and `kind`.
pub fn completion_item(label: &str, kind: CompletionItemKind) -> CompletionItem {
    CompletionItem {
        label: label.to_owned(),
        kind: Some(kind),
        ..Default::default()
    }
}

pub fn call_completion_item(
    mod_ident: &ModuleIdent_,
    is_macro: bool,
    method_name_opt: Option<&Symbol>,
    function_name: &Symbol,
    type_args: &[Type],
    arg_names: &[Name],
    arg_types: &[Type],
    ret_type: &Type,
    inside_use: bool,
) -> CompletionItem {
    let sig_string = format!(
        "fun {}({}){}",
        type_args_to_ide_string(type_args, /* separate_lines */ false, /* verbose */ false),
        type_list_to_ide_string(arg_types, /* separate_lines */ false, /* verbose */ false),
        ret_type_to_ide_str(ret_type, /* verbose */ false)
    );
    // if it's a method call we omit the first argument which is guaranteed to be there as this is a
    // method and needs a receiver
    let omitted_arg_count = if method_name_opt.is_some() { 1 } else { 0 };
    let mut snippet_idx = 0;
    let arg_snippet = arg_names
        .iter()
        .zip(arg_types)
        .skip(omitted_arg_count)
        .map(|(name, ty)| {
            lambda_snippet(ty, &mut snippet_idx).unwrap_or_else(|| {
                let mut arg_name = name.to_string();
                if arg_name.starts_with('$') {
                    arg_name = arg_name[1..].to_string();
                }
                snippet_idx += 1;
                format!("${{{}:{}}}", snippet_idx, arg_name)
            })
        })
        .collect::<Vec<_>>()
        .join(", ");
    let macro_suffix = if is_macro { "!" } else { "" };
    let label_details = Some(CompletionItemLabelDetails {
        detail: Some(format!(
            " ({}{})",
            mod_ident_to_ide_string(mod_ident, None, true),
            function_name
        )),
        description: Some(sig_string),
    });

    let method_name = method_name_opt.unwrap_or(function_name);
    let (insert_text, insert_text_format) = if inside_use {
        (
            Some(format!("{method_name}")),
            Some(InsertTextFormat::PLAIN_TEXT),
        )
    } else {
        (
            Some(format!("{method_name}{macro_suffix}({arg_snippet})")),
            Some(InsertTextFormat::SNIPPET),
        )
    };

    CompletionItem {
        label: format!("{method_name}{macro_suffix}()"),
        label_details,
        kind: Some(CompletionItemKind::METHOD),
        insert_text,
        insert_text_format,
        ..Default::default()
    }
}

fn lambda_snippet(sp!(_, ty): &Type, snippet_idx: &mut i32) -> Option<String> {
    if let Type_::Fun(vec, _) = ty {
        let arg_snippets = vec
            .iter()
            .map(|_| {
                *snippet_idx += 1;
                format!("${{{snippet_idx}}}")
            })
            .collect::<Vec<_>>()
            .join(", ");
        *snippet_idx += 1;
        return Some(format!("|{arg_snippets}| ${{{snippet_idx}}}"));
    }
    None
}
