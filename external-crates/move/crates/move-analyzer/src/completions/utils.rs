// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::symbols::{
    Symbols,
    compilation::{CompiledPkgInfo, SymbolsComputationData},
    compute_symbols_parsed_program, compute_symbols_pre_process,
    cursor::CursorContext,
    def_info::DefInfo,
    ide_strings::{
        mod_ident_to_ide_string, ret_type_to_ide_str, type_args_to_ide_string,
        type_list_to_ide_string,
    },
    mod_defs::{AutoImportInsertionInfo, AutoImportInsertionKind, ModuleDefs},
};

use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionItemLabelDetails, InsertTextFormat, Position,
    Range, TextEdit,
};
use move_compiler::{
    expansion::ast::{Address, ModuleIdent, ModuleIdent_, Visibility},
    naming::ast::{Type, TypeInner},
    parser::keywords::PRIMITIVE_TYPES,
    shared::{Identifier, Name, ide::AliasAutocompleteInfo},
};
use move_ir_types::location::sp;
use move_symbol_pool::Symbol;

use std::{path::PathBuf, sync::LazyLock};

/// Describes how a module should be referenced from an auto-imported completion or quick fix.
pub struct ModuleImportInfo {
    /// Prefix to insert at the use site, usually the module name or an existing alias.
    pub module_prefix: String,
    /// Import to insert for the module, or `None` if the module is already in scope.
    pub import_text: Option<String>,
}

/// List of completion items of Move's primitive types.
pub static PRIMITIVE_TYPE_COMPLETIONS: LazyLock<Vec<CompletionItem>> = LazyLock::new(|| {
    let mut primitive_types = PRIMITIVE_TYPES
        .iter()
        .map(|label| completion_item(label, CompletionItemKind::KEYWORD))
        .collect::<Vec<_>>();
    primitive_types.push(completion_item("address", CompletionItemKind::KEYWORD));
    primitive_types
});

/// Get import insertion info for the file where the cursor is located.
pub fn import_insertion_info(
    symbols: &Symbols,
    cursor: &CursorContext,
) -> Option<AutoImportInsertionInfo> {
    // Get the file path from the cursor's location
    let fhash = cursor.loc.file_hash();
    let fpath = symbols.files.file_path(&fhash);
    // For auto-imports, find the module whose range contains the cursor
    symbols.mod_parsing_info.get(fpath).and_then(|mod_map| {
        mod_map
            .iter()
            .find(|(mod_loc, _)| mod_loc.contains(&cursor.loc))
            .and_then(|(_, mod_info)| mod_info.import_insert_info)
    })
}

/// Get definitions for a given module.
pub fn mod_defs<'a>(symbols: &'a Symbols, mod_ident: &ModuleIdent_) -> Option<&'a ModuleDefs> {
    symbols
        .file_mods
        .values()
        .flatten()
        .find(|mdef| mdef.ident == *mod_ident)
}

/// Create text edit for auto-import insertion.
pub fn auto_import_text_edit(
    import_text: String,
    import_insertion_info: AutoImportInsertionInfo,
) -> TextEdit {
    let r = Range {
        start: import_insertion_info.pos,
        end: import_insertion_info.pos,
    };
    match import_insertion_info.kind {
        AutoImportInsertionKind::AfterLastImport => TextEdit {
            range: r,
            new_text: format!(
                "\n{}{};",
                " ".repeat(import_insertion_info.tabulation),
                import_text
            ),
        },
        AutoImportInsertionKind::BeforeFirstMember => TextEdit {
            range: r,
            new_text: format!(
                "{};\n\n{}",
                import_text,
                " ".repeat(import_insertion_info.tabulation)
            ),
        },
    }
}

/// Checks if a name is already bound in scope to a module, a member alias, a named
/// address, or a type parameter. Inserting an import with such a name would either
/// produce a duplicate-alias error (same-scope aliases) or silently shadow the
/// existing binding for the rest of the module (named addresses, as modules and
/// addresses share the leading-name namespace).
///
/// The member alias check is conservative: only struct and enum aliases share the
/// leading-name namespace, but alias info does not carry the member kind, so all
/// member aliases are treated as conflicts (the cost is falling back to a fully
/// qualified fix or completion).
pub fn name_taken_in_scope(info: &AliasAutocompleteInfo, name: Symbol) -> bool {
    info.modules.contains_key(&name)
        || info.addresses.contains_key(&name)
        || info.type_params.contains(&name)
        || info
            .members
            .values()
            .flatten()
            .any(|member_alias| *member_alias == name)
}

/// Computes a module prefix and optional module import for a target module.
/// Returns `None` if importing the module would conflict with a name already in scope.
pub fn module_import_info(
    mod_ident: ModuleIdent,
    info: &AliasAutocompleteInfo,
) -> Option<ModuleImportInfo> {
    for (alias, in_scope_mod_ident) in &info.modules {
        if in_scope_mod_ident.value == mod_ident.value {
            return Some(ModuleImportInfo {
                module_prefix: alias.to_string(),
                import_text: None,
            });
        }
    }

    let module_name = mod_ident.value.module.value();
    if name_taken_in_scope(info, module_name) {
        return None;
    }

    Some(ModuleImportInfo {
        module_prefix: module_name.to_string(),
        import_text: Some(format!(
            "use {}::{}",
            addr_to_ide_string(&mod_ident.value.address),
            mod_ident.value.module,
        )),
    })
}

/// Returns an iterator over module identifiers and function names defined in these modules.
/// Filters out all functions that should not be imported.
pub fn all_mod_functions_to_import<'a>(
    symbols: &'a Symbols,
    cursor: &'a CursorContext,
) -> Box<dyn Iterator<Item = (ModuleIdent, Box<dyn Iterator<Item = Symbol> + 'a>)> + 'a> {
    Box::new(symbols.file_mods.values().flatten().map(move |mod_defs| {
        (
            sp(mod_defs.name_loc, mod_defs.ident),
            Box::new(
                mod_defs
                    .functions
                    .iter()
                    .filter_map(move |(member_name, fdef)| {
                        if let Some(DefInfo::Function(_, visibility, ..)) =
                            symbols.def_info(&fdef.name_loc)
                            && exclude_member_from_import(mod_defs, cursor.module, visibility)
                        {
                            return None;
                        }
                        Some(*member_name)
                    }),
            ) as Box<dyn Iterator<Item = Symbol>>,
        )
    }))
}

/// Returns an iterator over module identifiers and struct names defined in these modules.
/// Filters out all structs that should not be imported.
pub fn all_mod_structs_to_import<'a>(
    symbols: &'a Symbols,
    cursor: &'a CursorContext,
) -> Box<dyn Iterator<Item = (ModuleIdent, Box<dyn Iterator<Item = Symbol> + 'a>)> + 'a> {
    Box::new(symbols.file_mods.values().flatten().map(move |mod_defs| {
        (
            sp(mod_defs.name_loc, mod_defs.ident),
            Box::new(
                mod_defs
                    .structs
                    .iter()
                    .filter_map(move |(member_name, sdef)| {
                        if let Some(DefInfo::Struct(_, _, visibility, ..)) =
                            symbols.def_info(&sdef.name_loc)
                            && exclude_member_from_import(mod_defs, cursor.module, visibility)
                        {
                            return None;
                        }
                        Some(*member_name)
                    }),
            ) as Box<dyn Iterator<Item = Symbol>>,
        )
    }))
}

/// Returns an iterator over module identifiers and enum names defined in these modules.
/// Filters out all enums that should not be imported.
pub fn all_mod_enums_to_import<'a>(
    symbols: &'a Symbols,
    cursor: &'a CursorContext,
) -> Box<dyn Iterator<Item = (ModuleIdent, Box<dyn Iterator<Item = Symbol> + 'a>)> + 'a> {
    Box::new(symbols.file_mods.values().flatten().map(move |mod_defs| {
        (
            sp(mod_defs.name_loc, mod_defs.ident),
            Box::new(
                mod_defs
                    .enums
                    .iter()
                    .filter_map(move |(member_name, edef)| {
                        if let Some(DefInfo::Enum(_, _, visibility, ..)) =
                            symbols.def_info(&edef.name_loc)
                            && exclude_member_from_import(mod_defs, cursor.module, visibility)
                        {
                            return None;
                        }
                        Some(*member_name)
                    }),
            ) as Box<dyn Iterator<Item = Symbol>>,
        )
    }))
}

/// Returns an iterator over module identifiers and constant names defined in these modules.
/// Filters out all constants that should not be imported.
pub fn all_mod_consts_to_import<'a>(
    symbols: &'a Symbols,
    cursor: &'a CursorContext,
) -> Box<dyn Iterator<Item = (ModuleIdent, Box<dyn Iterator<Item = Symbol> + 'a>)> + 'a> {
    Box::new(symbols.file_mods.values().flatten().map(move |mod_defs| {
        (
            sp(mod_defs.name_loc, mod_defs.ident),
            Box::new(mod_defs.constants.keys().filter_map(move |member_name| {
                // TODO: if constants stop being private, use their actual visibility
                // instead of internal
                if exclude_member_from_import(
                    mod_defs,
                    cursor.module,
                    &move_compiler::expansion::ast::Visibility::Internal,
                ) {
                    return None;
                }
                Some(*member_name)
            })) as Box<dyn Iterator<Item = Symbol>>,
        )
    }))
}

/// Given source module, and another module definition, checks if a member
/// of this other module can be imported to the source module. If source
/// module is None, the member can be imported regardless of visibility.
fn exclude_member_from_import(
    mod_defs: &ModuleDefs,
    src_mod_ident_opt: Option<ModuleIdent>,
    visibility: &Visibility,
) -> bool {
    if let Some(src_mod_ident) = src_mod_ident_opt {
        if mod_defs.neighbors.contains_key(&src_mod_ident) {
            // circular dependency detected, exclude member
            // TODO: this only works if there are "true" dependencies
            // in the source files as the compiler does not populate
            // the `neighbors` map using `use` statements - should we
            // fix this at some point?
            return true;
        }
        if mod_defs.ident != src_mod_ident.value {
            match visibility {
                Visibility::Internal => true,
                Visibility::Package(_) => mod_defs.ident.address != src_mod_ident.value.address,
                _ => false,
            }
        } else {
            // same module, include member regardless of visibility
            false
        }
    } else {
        // no source module, include member regardless of visibility
        false
    }
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
        type_args_to_ide_string(
            type_args, /* separate_lines */ false, /* verbose */ false
        ),
        type_list_to_ide_string(
            arg_types, /* separate_lines */ false, /* verbose */ false
        ),
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

// Constructs a cursor context and from existing symbols and
// updates symbols to reflect this.
pub fn compute_cursor(
    symbols: &mut Symbols,
    compiled_pkg_info: &mut CompiledPkgInfo,
    cursor_path: &PathBuf,
    cursor_pos: Position,
) {
    let cursor_info = Some((cursor_path, cursor_pos));
    let mut symbols_computation_data = SymbolsComputationData::new();
    // we only compute cursor context and tag it on the existing symbols to avoid spending time
    // recomputing all symbols (saves quite a bit of time when running the test suite)
    let mut cursor_context = compute_symbols_pre_process(
        &mut symbols_computation_data,
        compiled_pkg_info,
        cursor_info,
    );
    cursor_context = compute_symbols_parsed_program(
        &mut symbols_computation_data,
        compiled_pkg_info,
        cursor_context,
    );
    symbols.cursor_context = cursor_context;
}

pub fn addr_to_ide_string(addr: &Address) -> String {
    match addr {
        Address::Numerical {
            name,
            value,
            name_conflict: _,
        } => {
            if let Some(n) = name {
                format!("{n}")
            } else {
                format!("{value}")
            }
        }
        Address::NamedUnassigned(name) => {
            format!("{name}")
        }
    }
}

fn lambda_snippet(sp!(_, ty): &Type, snippet_idx: &mut i32) -> Option<String> {
    if let TypeInner::Fun(vec, _) = ty.inner() {
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
