// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    context::Context,
    symbols::{
        self, mod_ident_to_ide_string, ret_type_to_ide_str, type_args_to_ide_string,
        type_list_to_ide_string, type_to_ide_string, ChainCompletionKind, ChainInfo, CursorContext,
        CursorDefinition, DefInfo, FunType, PrecompiledPkgDeps, SymbolicatorRunner, Symbols,
        VariantInfo,
    },
    utils,
};
use itertools::Itertools;
use lsp_server::Request;
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionItemLabelDetails, CompletionParams,
    Documentation, InsertTextFormat, Position,
};
use move_command_line_common::files::FileHash;
use move_compiler::{
    editions::Edition,
    expansion::ast::{Address, ModuleIdent, ModuleIdent_, Visibility},
    linters::LintLevel,
    naming::ast::{Type, Type_},
    parser::{
        ast::{self as P, Ability_, LeadingNameAccess, LeadingNameAccess_},
        keywords::{BUILTINS, CONTEXTUAL_KEYWORDS, KEYWORDS, PRIMITIVE_TYPES},
        lexer::{Lexer, Tok},
    },
    shared::{
        ide::{AliasAutocompleteInfo, AutocompleteMethod},
        Identifier, Name, NumericalAddress,
    },
};
use move_ir_types::location::{sp, Loc};
use move_symbol_pool::Symbol;

use once_cell::sync::Lazy;

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use vfs::VfsPath;

/// Describes kind of the name access chain component.
enum ChainComponentKind {
    Package(LeadingNameAccess),
    Module(ModuleIdent),
    Member(ModuleIdent, Symbol),
}

/// Information about access chain component - its location and kind.
struct ChainComponentInfo {
    loc: Loc,
    kind: ChainComponentKind,
}

impl ChainComponentInfo {
    fn new(loc: Loc, kind: ChainComponentKind) -> Self {
        Self { loc, kind }
    }
}

/// Constructs an `lsp_types::CompletionItem` with the given `label` and `kind`.
fn completion_item(label: &str, kind: CompletionItemKind) -> CompletionItem {
    CompletionItem {
        label: label.to_owned(),
        kind: Some(kind),
        ..Default::default()
    }
}

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

/// List of completion items of Move's primitive types.
static PRIMITIVE_TYPE_COMPLETIONS: Lazy<Vec<CompletionItem>> = Lazy::new(|| {
    let mut primitive_types = PRIMITIVE_TYPES
        .iter()
        .map(|label| completion_item(label, CompletionItemKind::KEYWORD))
        .collect::<Vec<_>>();
    primitive_types.push(completion_item("address", CompletionItemKind::KEYWORD));
    primitive_types
});

/// List of completion items corresponding to each one of Move's builtin functions.
static BUILTIN_COMPLETIONS: Lazy<Vec<CompletionItem>> = Lazy::new(|| {
    BUILTINS
        .iter()
        .map(|label| completion_item(label, CompletionItemKind::FUNCTION))
        .collect()
});

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

/// Handle context-specific auto-completion requests with lbrace (`{`) trigger character.
fn context_specific_lbrace(
    symbols: &Symbols,
    cursor: &CursorContext,
) -> (Vec<CompletionItem>, bool) {
    let mut completions = vec![];
    let mut only_custom_items = false;
    // look for a struct definition on the line that contains `{`, check its abilities,
    // and do auto-completion if `key` ability is present
    let Some(CursorDefinition::Struct(sname)) = &cursor.defn_name else {
        return (completions, only_custom_items);
    };
    only_custom_items = true;
    let Some(mident) = cursor.module else {
        return (completions, only_custom_items);
    };
    let Some(typed_ast) = symbols.typed_ast.as_ref() else {
        return (completions, only_custom_items);
    };
    let Some(struct_def) = typed_ast.info.struct_definition_opt(&mident, sname) else {
        return (completions, only_custom_items);
    };
    if struct_def.abilities.has_ability_(Ability_::Key) {
        let obj_snippet = "\n\tid: UID,\n\t$1\n".to_string();
        let init_completion = CompletionItem {
            label: "id: UID".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            documentation: Some(Documentation::String("Object snippet".to_string())),
            insert_text: Some(obj_snippet),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        };
        completions.push(init_completion);
    }
    (completions, only_custom_items)
}

fn fun_def_info(symbols: &Symbols, mod_ident: ModuleIdent_, name: Symbol) -> Option<&DefInfo> {
    let Some(mod_defs) = symbols
        .file_mods
        .values()
        .flatten()
        .find(|mdef| mdef.ident == mod_ident)
    else {
        return None;
    };

    let Some(fdef) = mod_defs.functions.get(&name) else {
        return None;
    };
    symbols.def_info(&fdef.name_loc)
}

fn lamda_snippet(sp!(_, ty): &Type, snippet_idx: &mut i32) -> Option<String> {
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

fn call_completion_item(
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
        type_args_to_ide_string(type_args, /* verbose */ false),
        type_list_to_ide_string(arg_types, /* verbose */ false),
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
            lamda_snippet(ty, &mut snippet_idx).unwrap_or_else(|| {
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
            " ({}::{})",
            mod_ident_to_ide_string(mod_ident),
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

/// Handle dot auto-completion at a given position.
fn dot_completions(
    symbols: &Symbols,
    use_fpath: &Path,
    position: &Position,
) -> Vec<CompletionItem> {
    let mut completions = vec![];
    let Some(fhash) = symbols.file_hash(use_fpath) else {
        eprintln!("no dot completions due to missing file");
        return completions;
    };
    let Some(loc) = utils::lsp_position_to_loc(&symbols.files, fhash, position) else {
        eprintln!("no dot completions due to missing loc");
        return completions;
    };
    let Some(info) = symbols.compiler_info.get_autocomplete_info(fhash, &loc) else {
        eprintln!("no dot completions due to no compiler autocomplete info");
        return completions;
    };
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
    completions
}

/// Returns all possible completions for a module member (e.g., a datatype) component of a name
/// access chain, where the prefix of this component (e.g, in `some_pkg::some_mod::`) represents a
/// module specified in `prefix_mod_ident`. The `inside_use` parameter determines if completion is
/// for "regular" access chain or for completion within a `use` statement.
fn module_member_completions(
    symbols: &Symbols,
    cursor: &CursorContext,
    prefix_mod_ident: &ModuleIdent,
    chain_kind: ChainCompletionKind,
    inside_use: bool,
) -> Vec<CompletionItem> {
    use ChainCompletionKind as CT;

    let mut completions = vec![];

    let Some(mod_defs) = symbols
        .file_mods
        .values()
        .flatten()
        .find(|mdef| mdef.ident == prefix_mod_ident.value)
    else {
        return completions;
    };

    // list all members or only publicly visible ones
    let mut same_module = false;
    let mut same_package = false;
    if let Some(cursor_mod_ident) = cursor.module {
        if &cursor_mod_ident == prefix_mod_ident {
            same_module = true;
        }
        if cursor_mod_ident.value.address == prefix_mod_ident.value.address {
            same_package = true;
        }
    }

    if matches!(chain_kind, CT::Function) || matches!(chain_kind, CT::All) {
        let fun_completions = mod_defs
            .functions
            .iter()
            .filter_map(|(fname, fdef)| {
                symbols
                    .def_info(&fdef.name_loc)
                    .map(|def_info| (fname, def_info))
            })
            .filter(|(_, def_info)| {
                if let DefInfo::Function(_, visibility, ..) = def_info {
                    match visibility {
                        Visibility::Internal => same_module,
                        Visibility::Package(_) => same_package,
                        _ => true,
                    }
                } else {
                    false
                }
            })
            .filter_map(|(fname, def_info)| {
                if let DefInfo::Function(
                    _,
                    _,
                    fun_type,
                    _,
                    type_args,
                    arg_names,
                    arg_types,
                    ret_type,
                    _,
                ) = def_info
                {
                    Some(call_completion_item(
                        &prefix_mod_ident.value,
                        matches!(fun_type, FunType::Macro),
                        None,
                        fname,
                        type_args,
                        arg_names,
                        arg_types,
                        ret_type,
                        inside_use,
                    ))
                } else {
                    None
                }
            });
        completions.extend(fun_completions);
    }

    if matches!(chain_kind, CT::Type) || matches!(chain_kind, CT::All) {
        completions.extend(
            mod_defs
                .structs
                .keys()
                .map(|sname| completion_item(sname, CompletionItemKind::STRUCT)),
        );
        completions.extend(
            mod_defs
                .enums
                .keys()
                .map(|ename| completion_item(ename, CompletionItemKind::ENUM)),
        );
    }

    if matches!(chain_kind, CT::All) && same_module {
        completions.extend(
            mod_defs
                .constants
                .keys()
                .map(|cname| completion_item(cname, CompletionItemKind::CONSTANT)),
        );
    }

    completions
}

/// Returns completion item if a given name/alias identifies a valid member of a given module
/// available in the completion scope as if it was a single-length name chain.
fn single_name_member_completion(
    symbols: &Symbols,
    mod_ident: &ModuleIdent_,
    member_alias: &Symbol,
    member_name: &Symbol,
    chain_kind: ChainCompletionKind,
) -> Option<CompletionItem> {
    use ChainCompletionKind as CT;

    let Some(mod_defs) = symbols
        .file_mods
        .values()
        .flatten()
        .find(|mdef| mdef.ident == *mod_ident)
    else {
        return None;
    };

    // is it a function?
    if let Some(fdef) = mod_defs.functions.get(member_name) {
        if !(matches!(chain_kind, CT::Function) || matches!(chain_kind, CT::All)) {
            return None;
        }
        let Some(DefInfo::Function(.., fun_type, _, type_args, arg_names, arg_types, ret_type, _)) =
            symbols.def_info(&fdef.name_loc)
        else {
            return None;
        };
        return Some(call_completion_item(
            mod_ident,
            matches!(fun_type, FunType::Macro),
            None,
            member_alias,
            type_args,
            arg_names,
            arg_types,
            ret_type,
            /* inside_use */ false,
        ));
    };

    // is it a struct?
    if mod_defs.structs.get(member_name).is_some() {
        if !(matches!(chain_kind, CT::Type) || matches!(chain_kind, CT::All)) {
            return None;
        }
        return Some(completion_item(
            member_alias.as_str(),
            CompletionItemKind::STRUCT,
        ));
    }

    // is it an enum?
    if mod_defs.enums.get(member_name).is_some() {
        if !(matches!(chain_kind, CT::Type) || matches!(chain_kind, CT::All)) {
            return None;
        }
        return Some(completion_item(
            member_alias.as_str(),
            CompletionItemKind::ENUM,
        ));
    }

    // is it a const?
    if mod_defs.constants.get(member_name).is_some() {
        if !matches!(chain_kind, CT::All) {
            return None;
        }
        return Some(completion_item(
            member_alias.as_str(),
            CompletionItemKind::CONSTANT,
        ));
    }

    None
}

/// Returns completion items for all members of a given module as if they were single-length name
/// chains.
fn all_single_name_member_completions(
    symbols: &Symbols,
    members_info: &BTreeSet<(Symbol, ModuleIdent, Name)>,
    chain_kind: ChainCompletionKind,
) -> Vec<CompletionItem> {
    let mut completions = vec![];
    for (member_alias, sp!(_, mod_ident), member_name) in members_info {
        let Some(member_completion) = single_name_member_completion(
            symbols,
            mod_ident,
            member_alias,
            &member_name.value,
            chain_kind,
        ) else {
            continue;
        };
        completions.push(member_completion);
    }
    completions
}

/// Checks if a given module identifier represents a module in a package identifier by
/// `leading_name`.
fn is_pkg_mod_ident(mod_ident: &ModuleIdent_, leading_name: &LeadingNameAccess) -> bool {
    match mod_ident.address {
        Address::NamedUnassigned(name) => matches!(leading_name.value,
            LeadingNameAccess_::Name(n) | LeadingNameAccess_::GlobalAddress(n) if name == n),
        Address::Numerical {
            name,
            value,
            name_conflict: _,
        } => match leading_name.value {
            LeadingNameAccess_::AnonymousAddress(addr) if addr == value.value => true,
            LeadingNameAccess_::Name(addr_name) | LeadingNameAccess_::GlobalAddress(addr_name)
                if Some(addr_name) == name =>
            {
                true
            }
            _ => false,
        },
    }
}

/// Gets module identifiers for a given package identified by `leading_name`.
fn pkg_mod_identifiers(
    symbols: &Symbols,
    info: &AliasAutocompleteInfo,
    leading_name: &LeadingNameAccess,
) -> BTreeSet<ModuleIdent> {
    info.modules
        .values()
        .filter(|mod_ident| is_pkg_mod_ident(&mod_ident.value, leading_name))
        .copied()
        .chain(
            symbols
                .file_mods
                .values()
                .flatten()
                .map(|mdef| sp(mdef.name_loc, mdef.ident))
                .filter(|mod_ident| is_pkg_mod_ident(&mod_ident.value, leading_name)),
        )
        .collect::<BTreeSet<_>>()
}

/// Computes completion for a single enum variant.
fn variant_completion(symbols: &Symbols, vinfo: &VariantInfo) -> Option<CompletionItem> {
    let Some(DefInfo::Variant(_, _, vname, is_positional, field_names, ..)) =
        symbols.def_info.get(&vinfo.name.loc)
    else {
        return None;
    };

    let label = if field_names.is_empty() {
        vname.to_string()
    } else if *is_positional {
        format!("{vname}()")
    } else {
        format!("{vname}{{}}")
    };
    let field_snippet = field_names
        .iter()
        .enumerate()
        .map(|(snippet_idx, fname)| {
            if *is_positional {
                format!("${{{}}}", snippet_idx + 1)
            } else {
                format!("${{{}:{}}}", snippet_idx + 1, fname)
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let insert_text = if *is_positional {
        format!("{vname}({field_snippet})")
    } else {
        format!("{vname}{{{field_snippet}}}")
    };

    Some(CompletionItem {
        label,
        kind: Some(CompletionItemKind::ENUM_MEMBER),
        insert_text: Some(insert_text),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    })
}

/// Computes completions for variants of a given enum.
fn all_variant_completions(
    symbols: &Symbols,
    mod_ident: &ModuleIdent,
    datatype_name: Symbol,
) -> Vec<CompletionItem> {
    let Some(mod_defs) = symbols
        .file_mods
        .values()
        .flatten()
        .find(|mdef| mdef.ident == mod_ident.value)
    else {
        return vec![];
    };

    let Some(edef) = mod_defs.enums.get(&datatype_name) else {
        return vec![];
    };

    let Some(DefInfo::Enum(.., variants, _)) = symbols.def_info.get(&edef.name_loc) else {
        return vec![];
    };

    variants
        .iter()
        .filter_map(|vinfo| variant_completion(symbols, vinfo))
        .collect_vec()
}

/// Computes completions for a given chain entry: `prev_kind` determines the kind of previous chain
/// component, and `chain_kind` contains information about the entity that the whole chain may
/// represent (e.g., a type of or a function).
fn name_chain_entry_completions(
    symbols: &Symbols,
    cursor: &CursorContext,
    info: &AliasAutocompleteInfo,
    prev_kind: ChainComponentKind,
    chain_kind: ChainCompletionKind,
    inside_use: bool,
    completions: &mut Vec<CompletionItem>,
) {
    match prev_kind {
        ChainComponentKind::Package(leading_name) => {
            for mod_ident in pkg_mod_identifiers(symbols, info, &leading_name) {
                completions.push(completion_item(
                    mod_ident.value.module.value().as_str(),
                    CompletionItemKind::MODULE,
                ));
            }
        }
        ChainComponentKind::Module(mod_ident) => {
            completions.extend(module_member_completions(
                symbols, cursor, &mod_ident, chain_kind, inside_use,
            ));
        }
        ChainComponentKind::Member(mod_ident, member_name) => {
            completions.extend(all_variant_completions(symbols, &mod_ident, member_name));
        }
    }
}

/// Computes the kind of the next chain component (based on what the previous one, represented by
/// `prev_kind` was).
fn next_name_chain_component_kind(
    symbols: &Symbols,
    info: &AliasAutocompleteInfo,
    prev_kind: ChainComponentKind,
    component_name: Name,
) -> Option<ChainComponentKind> {
    match prev_kind {
        ChainComponentKind::Package(leading_name) => {
            pkg_mod_identifiers(symbols, info, &leading_name)
                .into_iter()
                .find(|mod_ident| mod_ident.value.module.value() == component_name.value)
                .map(ChainComponentKind::Module)
        }
        ChainComponentKind::Module(mod_ident) => {
            Some(ChainComponentKind::Member(mod_ident, component_name.value))
        }
        ChainComponentKind::Member(_, _) => None, // no more "after" completions to be processed
    }
}

/// Walks down a name chain, looking for the relevant portion that contains the cursor. When it
/// finds, it calls to `name_chain_entry_completions` to compute and return the completions.
fn completions_for_name_chain_entry(
    symbols: &Symbols,
    cursor: &CursorContext,
    info: &AliasAutocompleteInfo,
    prev_info: ChainComponentInfo,
    chain_kind: ChainCompletionKind,
    path_entries: &[Name],
    path_index: usize,
    colon_colon_triggered: bool,
    inside_use: bool,
    completions: &mut Vec<CompletionItem>,
) {
    let ChainComponentInfo {
        loc: prev_loc,
        kind: prev_kind,
    } = prev_info;

    let mut at_colon_colon = false;
    if path_index == path_entries.len() {
        // the only reason we would not return here is if we were at `::` which is past the location
        // of the last path component
        if colon_colon_triggered && cursor.loc.start() > prev_loc.end() {
            at_colon_colon = true;
        } else {
            return;
        }
    }

    if !at_colon_colon {
        // we are not at the last `::` but we may be at an intermediate one
        if colon_colon_triggered
            && path_index < path_entries.len()
            && cursor.loc.start() > prev_loc.end()
            && cursor.loc.end() <= path_entries[path_index].loc.start()
        {
            at_colon_colon = true;
        }
    }

    // we are at `::`, or at some component's identifier
    if at_colon_colon || path_entries[path_index].loc.contains(&cursor.loc) {
        name_chain_entry_completions(
            symbols,
            cursor,
            info,
            prev_kind,
            chain_kind,
            inside_use,
            completions,
        );
    } else {
        let component_name = path_entries[path_index];
        if let Some(next_kind) =
            next_name_chain_component_kind(symbols, info, prev_kind, component_name)
        {
            completions_for_name_chain_entry(
                symbols,
                cursor,
                info,
                ChainComponentInfo::new(component_name.loc, next_kind),
                chain_kind,
                path_entries,
                path_index + 1,
                colon_colon_triggered,
                inside_use,
                completions,
            );
        }
    }
}

/// Check if a given address represents a package within the current program.
fn is_package_address(
    symbols: &Symbols,
    info: &AliasAutocompleteInfo,
    pkg_addr: NumericalAddress,
) -> bool {
    if info.addresses.iter().any(|(_, a)| a == &pkg_addr) {
        return true;
    }

    symbols.file_mods.values().flatten().any(|mdef| {
        matches!(mdef.ident.address,
            Address::Numerical { value, .. } if value.value == pkg_addr)
    })
}

/// Check if a given name represents a package within the current program.
fn is_package_name(symbols: &Symbols, info: &AliasAutocompleteInfo, pkg_name: Name) -> bool {
    if info.addresses.contains_key(&pkg_name.value) {
        return true;
    }

    symbols
        .file_mods
        .values()
        .flatten()
        .map(|mdef| &mdef.ident)
        .any(|mod_ident| match &mod_ident.address {
            Address::NamedUnassigned(name) if name == &pkg_name => true,
            Address::Numerical {
                name: Some(name), ..
            } if name == &pkg_name => true,
            _ => false,
        })
}

/// Get all packages that could be a target of auto-completion, whether they are part of
/// `AliasAutocompleteInfo` or not.
fn all_packages(symbols: &Symbols, info: &AliasAutocompleteInfo) -> BTreeSet<String> {
    let mut addresses = BTreeSet::new();
    for (n, a) in &info.addresses {
        addresses.insert(n.to_string());
        addresses.insert(a.to_string());
    }

    symbols
        .file_mods
        .values()
        .flatten()
        .map(|mdef| &mdef.ident)
        .for_each(|mod_ident| match &mod_ident.address {
            Address::Numerical { name, value, .. } => {
                if let Some(n) = name {
                    addresses.insert(n.to_string());
                }
                addresses.insert(value.to_string());
            }
            Address::NamedUnassigned(n) => {
                addresses.insert(n.to_string());
            }
        });

    addresses
}

/// Computes the kind of the fist chain component.
fn first_name_chain_component_kind(
    symbols: &Symbols,
    info: &AliasAutocompleteInfo,
    leading_name: LeadingNameAccess,
) -> Option<ChainComponentKind> {
    match leading_name.value {
        LeadingNameAccess_::Name(n) => {
            if is_package_name(symbols, info, n) {
                Some(ChainComponentKind::Package(leading_name))
            } else if let Some(mod_ident) = info.modules.get(&n.value) {
                Some(ChainComponentKind::Module(*mod_ident))
            } else if let Some((mod_ident, member_name)) =
                info.members
                    .iter()
                    .find_map(|(alias_name, mod_ident, member_name)| {
                        if alias_name == &n.value {
                            Some((*mod_ident, member_name))
                        } else {
                            None
                        }
                    })
            {
                Some(ChainComponentKind::Member(mod_ident, member_name.value))
            } else {
                None
            }
        }
        LeadingNameAccess_::AnonymousAddress(addr) => {
            if is_package_address(symbols, info, addr) {
                Some(ChainComponentKind::Package(leading_name))
            } else {
                None
            }
        }
        LeadingNameAccess_::GlobalAddress(n) => {
            // if leading name is global address then the first component can only be a
            // package
            if is_package_name(symbols, info, n) {
                Some(ChainComponentKind::Package(leading_name))
            } else {
                None
            }
        }
    }
}

/// Handle name chain auto-completion at a given position. The gist of this approach is to first
/// identify what the first component of the access chain represents (as it may be a package, module
/// or a member) and if the chain has other components, recursively process them in turn to either
/// - finish auto-completion if cursor is on a given component's identifier
/// - identify what the subsequent component represents and keep going
fn name_chain_completions(
    symbols: &Symbols,
    cursor: &CursorContext,
    colon_colon_triggered: bool,
) -> (Vec<CompletionItem>, bool) {
    eprintln!("looking for name access chains");
    let mut completions = vec![];
    let mut only_custom_items = false;
    let Some(ChainInfo {
        chain,
        kind: chain_kind,
        inside_use,
    }) = cursor.find_access_chain()
    else {
        eprintln!("no access chain");
        return (completions, only_custom_items);
    };

    let (leading_name, path_entries) = match &chain.value {
        P::NameAccessChain_::Single(entry) => (
            sp(entry.name.loc, LeadingNameAccess_::Name(entry.name)),
            vec![],
        ),
        P::NameAccessChain_::Path(name_path) => (
            name_path.root.name,
            name_path.entries.iter().map(|e| e.name).collect::<Vec<_>>(),
        ),
    };

    // there may be access chains for which there is not auto-completion info generated by the
    // compiler but which still have to be handled (e.g., chains starting with numeric address)
    let info = symbols
        .compiler_info
        .path_autocomplete_info
        .get(&leading_name.loc)
        .cloned()
        .unwrap_or_else(AliasAutocompleteInfo::new);

    eprintln!("found access chain for auto-completion (adddreses: {}, modules: {}, members: {}, tparams: {}",
              info.addresses.len(), info.modules.len(), info.members.len(), info.type_params.len());

    // if we are auto-completing for an access chain, there is no need to include default completions
    only_custom_items = true;

    if leading_name.loc.contains(&cursor.loc) {
        // at first position of the chain suggest all packages that are available regardless of what
        // the leading name represents, as a package always fits at that position, for example:
        // OxCAFE::...
        // some_name::...
        // ::some_name
        //
        completions.extend(
            all_packages(symbols, &info)
                .iter()
                .map(|n| completion_item(n.as_str(), CompletionItemKind::UNIT)),
        );

        // only if leading name is actually a name, modules or module members are a correct
        // auto-completion in the first position
        if let LeadingNameAccess_::Name(_) = &leading_name.value {
            completions.extend(
                info.modules
                    .keys()
                    .map(|n| completion_item(n.as_str(), CompletionItemKind::MODULE)),
            );
            completions.extend(all_single_name_member_completions(
                symbols,
                &info.members,
                chain_kind,
            ));
            if matches!(chain_kind, ChainCompletionKind::Type) {
                completions.extend(PRIMITIVE_TYPE_COMPLETIONS.clone());
                completions.extend(
                    info.type_params
                        .iter()
                        .map(|t| completion_item(t.as_str(), CompletionItemKind::TYPE_PARAMETER)),
                );
            }
        }
    } else if let Some(next_kind) = first_name_chain_component_kind(symbols, &info, leading_name) {
        completions_for_name_chain_entry(
            symbols,
            cursor,
            &info,
            ChainComponentInfo::new(leading_name.loc, next_kind),
            chain_kind,
            &path_entries,
            /* path_index */ 0,
            colon_colon_triggered,
            inside_use,
            &mut completions,
        );
    }

    eprintln!("found {} access chain completions", completions.len());

    (completions, only_custom_items)
}

/// Computes auto-completions for module uses.
fn module_use_completions(
    symbols: &Symbols,
    cursor: &CursorContext,
    info: &AliasAutocompleteInfo,
    mod_use: &P::ModuleUse,
    package: &LeadingNameAccess,
    mod_name: &P::ModuleName,
) -> Vec<CompletionItem> {
    use P::ModuleUse as MU;
    let mut completions = vec![];

    let Some(mod_ident) = pkg_mod_identifiers(symbols, info, package)
        .into_iter()
        .find(|mod_ident| &mod_ident.value.module == mod_name)
    else {
        return completions;
    };

    match mod_use {
        MU::Module(_) => (), // nothing to do with just module alias
        MU::Members(members) => {
            if let Some((first_name, _)) = members.first() {
                if cursor.loc.start() > mod_name.loc().end()
                    && cursor.loc.end() <= first_name.loc.start()
                {
                    // cursor is after `::` succeeding module but before the first module member
                    completions.extend(module_member_completions(
                        symbols,
                        cursor,
                        &mod_ident,
                        ChainCompletionKind::All,
                        /* inside_use */ true,
                    ));
                    // no point in falling through to the members loop below
                    return completions;
                }
            }

            for (sp!(mloc, _), _) in members {
                if mloc.contains(&cursor.loc) {
                    // cursor is at identifier representing module member
                    completions.extend(module_member_completions(
                        symbols,
                        cursor,
                        &mod_ident,
                        ChainCompletionKind::All,
                        /* inside_use */ true,
                    ));
                    // no point checking other locations
                    break;
                }
            }
        }
        MU::Partial {
            colon_colon,
            opening_brace: _,
        } => {
            if let Some(colon_colon_loc) = colon_colon {
                if cursor.loc.start() >= colon_colon_loc.start() {
                    // cursor is on or past `::`
                    completions.extend(module_member_completions(
                        symbols,
                        cursor,
                        &mod_ident,
                        ChainCompletionKind::All,
                        /* inside_use */ true,
                    ));
                }
            }
        }
    }

    completions
}

/// Handles auto-completions for "regular" `use` declarations (name access chains in `use fun`
/// declarations are handled as part of name chain completions).
fn use_decl_completions(symbols: &Symbols, cursor: &CursorContext) -> (Vec<CompletionItem>, bool) {
    eprintln!("looking for use declarations");
    let mut completions = vec![];
    let mut only_custom_items = false;
    let Some(use_) = cursor.find_use_decl() else {
        eprintln!("no use declaration");
        return (completions, only_custom_items);
    };
    eprintln!("use declaration {:?}", use_);

    // if we are auto-completing for a use decl, there is no need to include default completions
    only_custom_items = true;

    // there is no auto-completion info generated by the compiler for this but helper methods used
    // here are shared with name chain completion where it may exist, so we create an "empty" one
    // here
    let info = AliasAutocompleteInfo::new();

    match use_ {
        P::Use::ModuleUse(sp!(_, mod_ident), mod_use) => {
            if mod_ident.address.loc.contains(&cursor.loc) {
                // cursor on package (e.g., on `some_pkg` in `some_pkg::some_mod`)
                completions.extend(
                    all_packages(symbols, &info)
                        .iter()
                        .map(|n| completion_item(n.as_str(), CompletionItemKind::UNIT)),
                );
            } else if cursor.loc.start() > mod_ident.address.loc.end()
                && cursor.loc.end() <= mod_ident.module.loc().end()
            {
                // cursor is either at the `::` succeeding package/address or at the identifier
                // following that particular `::`
                for ident in pkg_mod_identifiers(symbols, &info, &mod_ident.address) {
                    completions.push(completion_item(
                        ident.value.module.value().as_str(),
                        CompletionItemKind::MODULE,
                    ));
                }
            } else {
                completions.extend(module_use_completions(
                    symbols,
                    cursor,
                    &info,
                    &mod_use,
                    &mod_ident.address,
                    &mod_ident.module,
                ));
            }
        }
        P::Use::NestedModuleUses(leading_name, uses) => {
            if leading_name.loc.contains(&cursor.loc) {
                // cursor on package
                completions.extend(
                    all_packages(symbols, &info)
                        .iter()
                        .map(|n| completion_item(n.as_str(), CompletionItemKind::UNIT)),
                );
            } else {
                if let Some((first_name, _)) = uses.first() {
                    if cursor.loc.start() > leading_name.loc.end()
                        && cursor.loc.end() <= first_name.loc().start()
                    {
                        // cursor is after `::` succeeding address/package but before the first
                        // module
                        for ident in pkg_mod_identifiers(symbols, &info, &leading_name) {
                            completions.push(completion_item(
                                ident.value.module.value().as_str(),
                                CompletionItemKind::MODULE,
                            ));
                        }
                        // no point in falling through to the uses loop below
                        return (completions, only_custom_items);
                    }
                }

                for (mod_name, mod_use) in &uses {
                    if mod_name.loc().contains(&cursor.loc) {
                        for ident in pkg_mod_identifiers(symbols, &info, &leading_name) {
                            completions.push(completion_item(
                                ident.value.module.value().as_str(),
                                CompletionItemKind::MODULE,
                            ));
                        }
                        // no point checking other locations
                        break;
                    }
                    completions.extend(module_use_completions(
                        symbols,
                        cursor,
                        &info,
                        mod_use,
                        &leading_name,
                        mod_name,
                    ));
                }
            }
        }
        P::Use::Fun { .. } => (), // already handled as part of name chain completion
        P::Use::Partial {
            package,
            colon_colon,
            opening_brace: _,
        } => {
            if package.loc.contains(&cursor.loc) {
                // cursor on package name/address
                completions.extend(
                    all_packages(symbols, &info)
                        .iter()
                        .map(|n| completion_item(n.as_str(), CompletionItemKind::UNIT)),
                );
            }
            if let Some(colon_colon_loc) = colon_colon {
                if cursor.loc.start() >= colon_colon_loc.start() {
                    // cursor is on or past `::`
                    for ident in pkg_mod_identifiers(symbols, &info, &package) {
                        completions.push(completion_item(
                            ident.value.module.value().as_str(),
                            CompletionItemKind::MODULE,
                        ));
                    }
                }
            }
        }
    }

    (completions, only_custom_items)
}

/// Handle context-specific auto-completion requests with no trigger character.
fn context_specific_no_trigger(
    symbols: &Symbols,
    use_fpath: &Path,
    buffer: &str,
    position: &Position,
) -> (Vec<CompletionItem>, bool) {
    eprintln!("looking for dot");
    let mut completions = dot_completions(symbols, use_fpath, position);
    eprintln!("dot found: {}", !completions.is_empty());
    if !completions.is_empty() {
        // found dot completions - do not look for any other
        return (completions, true);
    }

    let mut only_custom_items = false;

    let strings = preceding_strings(buffer, position);

    if strings.is_empty() {
        return (completions, only_custom_items);
    }

    // at this point only try to auto-complete init function declararation - get the last string
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
                only_custom_items = true;
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
    (completions, only_custom_items)
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

/// Sends the given connection a response to a completion request.
///
/// The completions returned depend upon where the user's cursor is positioned.
pub fn on_completion_request(
    context: &Context,
    request: &Request,
    ide_files_root: VfsPath,
    pkg_dependencies: Arc<Mutex<BTreeMap<PathBuf, PrecompiledPkgDeps>>>,
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
    let items = completions_with_context(context, ide_files_root, pkg_dependencies, &path, pos)
        .unwrap_or_default();
    let items_len = items.len();

    let result = serde_json::to_value(items).expect("could not serialize completion response");
    eprintln!("about to send completion response with {items_len} items");
    let response = lsp_server::Response::new_ok(request.id.clone(), result);
    if let Err(err) = context
        .connection
        .sender
        .send(lsp_server::Message::Response(response))
    {
        eprintln!("could not send completion response: {:?}", err);
    }
}

pub fn completions_with_context(
    context: &Context,
    ide_files_root: VfsPath,
    pkg_dependencies: Arc<Mutex<BTreeMap<PathBuf, PrecompiledPkgDeps>>>,
    path: &Path,
    pos: Position,
) -> Option<Vec<CompletionItem>> {
    let Some(pkg_path) = SymbolicatorRunner::root_dir(path) else {
        eprintln!("failed completion for {:?} (package root not found)", path);
        return None;
    };
    let symbol_map = context.symbols.lock().unwrap();
    let current_symbols = symbol_map.get(&pkg_path)?;
    Some(completion_items(
        current_symbols,
        ide_files_root,
        pkg_dependencies,
        path,
        pos,
    ))
}

pub fn completion_items(
    current_symbols: &Symbols,
    ide_files_root: VfsPath,
    pkg_dependencies: Arc<Mutex<BTreeMap<PathBuf, PrecompiledPkgDeps>>>,
    path: &Path,
    pos: Position,
) -> Vec<CompletionItem> {
    compute_cursor_completion_items(ide_files_root, pkg_dependencies, path, pos)
        .unwrap_or_else(|| compute_completion_items(current_symbols, path, pos))
}

fn compute_cursor_completion_items(
    ide_files_root: VfsPath,
    pkg_dependencies: Arc<Mutex<BTreeMap<PathBuf, PrecompiledPkgDeps>>>,
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
        LintLevel::None,
        cursor_info,
    )
    .ok()?;
    let symbols = symbols?;
    Some(compute_completion_items(&symbols, path, cursor_position))
}

/// Computes completion items for a given completion request.
fn compute_completion_items(symbols: &Symbols, path: &Path, pos: Position) -> Vec<CompletionItem> {
    let mut items = vec![];

    let Some(fhash) = symbols.file_hash(path) else {
        return items;
    };
    let Some(file_id) = symbols.files.file_mapping().get(&fhash) else {
        return items;
    };
    let Ok(file) = symbols.files.files().get(*file_id) else {
        return items;
    };

    let file_source = file.source().clone();
    if !file_source.is_empty() {
        let only_custom_items;
        match &symbols.cursor_context {
            Some(cursor_context) => {
                eprintln!("cursor completion");
                let (new_items, only_has_custom_items) =
                    cursor_completion_items(symbols, path, &file_source, pos, cursor_context);
                only_custom_items = only_has_custom_items;
                items.extend(new_items);
            }
            None => {
                eprintln!("non-cursor completion");
                let (new_items, only_has_custom_items) =
                    default_items(symbols, path, &file_source, pos);
                only_custom_items = only_has_custom_items;
                items.extend(new_items);
            }
        }
        if !only_custom_items {
            eprintln!("including identifiers");
            let identifiers = identifiers(&file_source, symbols, path);
            items.extend(identifiers);
        }
    } else {
        // no file content
        items.extend(KEYWORD_COMPLETIONS.clone());
        items.extend(BUILTIN_COMPLETIONS.clone());
    }
    items
}

/// Return completion items, plus a flag indicating if we should only use the custom items returned
/// (i.e., when the flag is false, default items and identifiers should also be added).
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
        Some(Tok::Period) => {
            eprintln!("found period");
            let items = dot_completions(symbols, path, &pos);
            let items_is_empty = items.is_empty();
            eprintln!("found items: {}", !items_is_empty);
            // whether completions have been found for the dot or not
            // it makes no sense to try offering "dumb" autocompletion
            // options here as they will not fit (an example would
            // be dot completion of u64 variable without any methods
            // with u64 receiver being visible)
            (items, true)
        }
        Some(Tok::ColonColon) => {
            let mut items = vec![];
            let mut only_custom_items = false;
            let (path_items, path_custom) =
                name_chain_completions(symbols, cursor, /* colon_colon_triggered */ true);
            items.extend(path_items);
            only_custom_items |= path_custom;
            if !only_custom_items {
                let (path_items, path_custom) = use_decl_completions(symbols, cursor);
                items.extend(path_items);
                only_custom_items |= path_custom;
            }
            (items, only_custom_items)
        }
        // Carve out to suggest UID for struct with key ability
        Some(Tok::LBrace) => {
            let mut items = vec![];
            let mut only_custom_items = false;
            let (path_items, path_custom) = context_specific_lbrace(symbols, cursor);
            items.extend(path_items);
            only_custom_items |= path_custom;
            if !only_custom_items {
                let (path_items, path_custom) = use_decl_completions(symbols, cursor);
                items.extend(path_items);
                only_custom_items |= path_custom;
            }
            (items, only_custom_items)
        }
        // TODO: should we handle auto-completion on `:`? If we model our support after
        // rust-analyzer then it does not do this - it starts auto-completing types after the first
        // character beyond `:` is typed
        _ => {
            eprintln!("no relevant cursor leader");
            let mut items = vec![];
            let mut only_custom_items = false;
            let (path_items, path_custom) =
                name_chain_completions(symbols, cursor, /* colon_colon_triggered */ false);
            items.extend(path_items);
            only_custom_items |= path_custom;
            if !only_custom_items {
                if matches!(cursor_leader, Some(Tok::Colon)) {
                    // much like rust-analyzer we do not auto-complete in the middle of `::`
                    only_custom_items = true;
                } else {
                    let (path_items, path_custom) = use_decl_completions(symbols, cursor);
                    items.extend(path_items);
                    only_custom_items |= path_custom;
                }
            }
            if !only_custom_items {
                eprintln!("checking default items");
                let (default_items, default_custom) =
                    default_items(symbols, path, file_source, pos);
                items.extend(default_items);
                only_custom_items |= default_custom;
            }
            (items, only_custom_items)
        }
    }
}

fn default_items(
    symbols: &Symbols,
    path: &Path,
    file_source: &str,
    pos: Position,
) -> (Vec<CompletionItem>, bool) {
    // If the user's cursor is positioned anywhere other than following a `.`, `:`, or `::`,
    // offer them context-specific autocompletion items as well as
    // Move's keywords, operators, and builtins.
    let (custom_items, only_custom_items) =
        context_specific_no_trigger(symbols, path, file_source, &pos);
    let mut items = custom_items;
    if !only_custom_items {
        items.extend(KEYWORD_COMPLETIONS.clone());
        items.extend(BUILTIN_COMPLETIONS.clone());
    }
    (items, only_custom_items)
}
