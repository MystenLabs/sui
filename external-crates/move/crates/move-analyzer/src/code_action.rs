// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    completions::utils::{
        all_mod_enums_to_import, all_mod_functions_to_import, all_mod_structs_to_import,
        auto_import_text_edit, import_insertion_info,
    },
    context::Context,
    symbols::{
        get_symbols, ChainInfo, CursorContext, PrecomputedPkgInfo, SymbolicatorRunner, Symbols,
    },
    utils::loc_start_to_lsp_position_opt,
};

use lsp_server::{Message, Request, Response};
use lsp_types::{
    CodeAction, CodeActionKind, CodeActionParams, Diagnostic, Range, TextEdit, WorkspaceEdit,
};
use move_symbol_pool::Symbol;
use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::{Arc, Mutex},
};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;
use url::Url;
use vfs::VfsPath;

use move_compiler::{
    expansion::ast::ModuleIdent,
    linters::LintLevel,
    parser::ast::{LeadingNameAccess_, NameAccessChain_},
    shared::{Identifier, Name},
};
use move_package::source_package::parsed_manifest::Dependencies;

// The following reflects prefixes of error messages for
// problems with a single-element access chain that are
// subject to auto fixes. The constants representing them
// cannot be lifted directly from the Move compiler,
// as they are (at least most of them) dynamically generated
// (in naming/translate.rs).
#[derive(Debug, EnumIter)]
enum SingleElementChainDiagPrefix {
    UnboundType,
    UnboundStructOrEnum,
    UnboundStruct,
    UnboundTypeOrFunction,
    UnboundFunction,
}

impl SingleElementChainDiagPrefix {
    fn as_str(&self) -> &'static str {
        // we include `'` at the end of the prefix as some of the prefixes
        // are shared with error messages that cannot be auto-fixed
        match self {
            SingleElementChainDiagPrefix::UnboundType => "Unbound type '",
            SingleElementChainDiagPrefix::UnboundStruct => "Unbound struct '",
            SingleElementChainDiagPrefix::UnboundStructOrEnum => "Unbound struct or enum '",
            SingleElementChainDiagPrefix::UnboundTypeOrFunction => "Unbound datatype or function '",
            SingleElementChainDiagPrefix::UnboundFunction => "Unbound function '",
        }
    }
}

/// The following reflects prefixes of error messages for
/// problems with a two-element access chain that are subject
/// to auto fixes.
#[derive(Debug, EnumIter)]
enum TwoElementChainDiagPrefix {
    UnresolvedName,
}

impl TwoElementChainDiagPrefix {
    fn as_str(&self) -> &'static str {
        match self {
            TwoElementChainDiagPrefix::UnresolvedName => "Could not resolve the name '",
        }
    }
}

/// Handles inlay hints request of the language server
pub fn on_code_action_request(
    context: &Context,
    request: &Request,
    ide_files_root: VfsPath,
    pkg_dependencies: Arc<Mutex<BTreeMap<PathBuf, PrecomputedPkgInfo>>>,
    implicit_deps: Dependencies,
) {
    let response = Response::new_ok(
        request.id.clone(),
        access_chain_autofix_actions(request, ide_files_root, pkg_dependencies, implicit_deps),
    );
    eprintln!("code_action_request: {:?}", request);
    if let Err(err) = context.connection.sender.send(Message::Response(response)) {
        eprintln!("could not send code action response: {:?}", err);
    }
}

/// Computes code actions related to access chain autofixes.
fn access_chain_autofix_actions(
    request: &Request,
    ide_files_root: VfsPath,
    pkg_dependencies: Arc<Mutex<BTreeMap<PathBuf, PrecomputedPkgInfo>>>,
    implicit_deps: Dependencies,
) -> Vec<CodeAction> {
    let mut code_actions = vec![];

    let params = serde_json::from_value::<CodeActionParams>(request.params.clone())
        .expect("could not deserialize code action request");

    if let Some(action_kinds) = params.context.only {
        // only serving quick fixes for now
        if !action_kinds.contains(&CodeActionKind::QUICKFIX) {
            return code_actions;
        }
    }

    if !params.context.diagnostics.iter().any(|diag| {
        SingleElementChainDiagPrefix::iter().any(|prefix| diag.message.starts_with(prefix.as_str()))
            || TwoElementChainDiagPrefix::iter()
                .any(|prefix| diag.message.starts_with(prefix.as_str()))
    }) {
        return code_actions;
    }

    let path: PathBuf = params.text_document.uri.path().into();
    let Some(pkg_path) = SymbolicatorRunner::root_dir(&path) else {
        return code_actions;
    };

    for diag in params.context.diagnostics.into_iter() {
        if !SingleElementChainDiagPrefix::iter()
            .any(|prefix| diag.message.starts_with(prefix.as_str()))
            && !TwoElementChainDiagPrefix::iter()
                .any(|prefix| diag.message.starts_with(prefix.as_str()))
        {
            continue;
        }
        // compute symbols just to get cursor position
        let cursor_position = diag.range.start;

        let cursor_info = Some((&path, cursor_position));

        let Ok((Some(symbols), _)) = get_symbols(
            pkg_dependencies.clone(),
            ide_files_root.clone(),
            &pkg_path,
            Some(vec![]),
            LintLevel::None,
            cursor_info,
            implicit_deps.clone(),
        ) else {
            continue;
        };
        let Some(ref cursor) = symbols.cursor_context else {
            continue;
        };
        let Some(ChainInfo {
            chain,
            kind: _,
            inside_use,
        }) = cursor.find_access_chain()
        else {
            continue;
        };
        if inside_use {
            // no auto-fixes in imports
            continue;
        }
        eprintln!("chain: {:?}", chain);
        match chain.value {
            NameAccessChain_::Single(path_entry) => {
                SingleElementChainDiagPrefix::iter().for_each(|prefix| match prefix {
                    SingleElementChainDiagPrefix::UnboundType
                    | SingleElementChainDiagPrefix::UnboundStructOrEnum => {
                        if diag.message.starts_with(prefix.as_str()) {
                            single_element_access_chain_autofixes(
                                &mut code_actions,
                                &symbols,
                                cursor,
                                params.text_document.uri.clone(),
                                path_entry.name,
                                all_mod_structs_to_import(&symbols, cursor)
                                    .chain(all_mod_enums_to_import(&symbols, cursor)),
                                diag.clone(),
                            );
                        }
                    }
                    SingleElementChainDiagPrefix::UnboundStruct => {
                        if diag.message.starts_with(prefix.as_str()) {
                            single_element_access_chain_autofixes(
                                &mut code_actions,
                                &symbols,
                                cursor,
                                params.text_document.uri.clone(),
                                path_entry.name,
                                all_mod_structs_to_import(&symbols, cursor),
                                diag.clone(),
                            );
                        }
                    }
                    SingleElementChainDiagPrefix::UnboundTypeOrFunction => {
                        if diag.message.starts_with(prefix.as_str()) {
                            single_element_access_chain_autofixes(
                                &mut code_actions,
                                &symbols,
                                cursor,
                                params.text_document.uri.clone(),
                                path_entry.name,
                                all_mod_structs_to_import(&symbols, cursor)
                                    .chain(all_mod_enums_to_import(&symbols, cursor))
                                    .chain(all_mod_functions_to_import(&symbols, cursor)),
                                diag.clone(),
                            );
                        }
                    }
                    SingleElementChainDiagPrefix::UnboundFunction => {
                        if diag.message.starts_with(prefix.as_str()) {
                            single_element_access_chain_autofixes(
                                &mut code_actions,
                                &symbols,
                                cursor,
                                params.text_document.uri.clone(),
                                path_entry.name,
                                all_mod_functions_to_import(&symbols, cursor),
                                diag.clone(),
                            );
                        }
                    }
                });
            }
            NameAccessChain_::Path(name_path) => {
                eprintln!("name_path: {:?}", name_path);
                if let LeadingNameAccess_::Name(unbound_name) = name_path.root.name.value {
                    eprintln!("unbound_name: {:?}", unbound_name);
                    if name_path.entries.len() == 1 {
                        eprintln!("two element chain");
                        // we assume that diagnostic reporting unbound chain component
                        // is on the first element of the chain
                        if unbound_name.loc.contains(&cursor.loc) {
                            eprintln!("contains cursor");
                            TwoElementChainDiagPrefix::iter().for_each(|prefix| match prefix {
                                TwoElementChainDiagPrefix::UnresolvedName => {
                                    if diag.message.starts_with(prefix.as_str()) {
                                        two_element_access_chain_autofixes(
                                            &mut code_actions,
                                            &symbols,
                                            cursor,
                                            params.text_document.uri.clone(),
                                            unbound_name,
                                            name_path.entries[0].name,
                                            all_mod_structs_to_import(&symbols, cursor)
                                                .chain(all_mod_enums_to_import(&symbols, cursor))
                                                .chain(all_mod_functions_to_import(
                                                    &symbols, cursor,
                                                )),
                                            diag.clone(),
                                        );
                                    }
                                }
                            });
                        }
                    }
                }
            }
        }
    }

    code_actions
}

/// Create auto-fixes for a single unbound element in an access chain.
fn single_element_access_chain_autofixes<'a, I, K>(
    code_actions: &mut Vec<CodeAction>,
    symbols: &Symbols,
    cursor: &CursorContext,
    file_url: Url,
    unbound_name: Name,
    mod_members: I,
    diag: Diagnostic,
) where
    I: Iterator<Item = (ModuleIdent, K)>,
    K: Iterator<Item = Symbol>,
{
    let Some(unbound_name_lsp_pos) =
        loc_start_to_lsp_position_opt(&symbols.files, &unbound_name.loc)
    else {
        return;
    };
    for (mod_ident, mod_members) in mod_members {
        for member_name in mod_members {
            if member_name == unbound_name.value {
                let qualified_prefix =
                    format!("{}::{}::", mod_ident.value.address, mod_ident.value.module);
                let text_edit = TextEdit {
                    range: Range {
                        start: unbound_name_lsp_pos,
                        end: unbound_name_lsp_pos,
                    },
                    new_text: qualified_prefix.clone(),
                };
                let title = format!("Qualify as `{}{}`", qualified_prefix, unbound_name);
                code_actions.push(access_chain_code_action(
                    title,
                    text_edit,
                    diag.clone(),
                    file_url.clone(),
                ));
                if let Some(import_insertion_info) = import_insertion_info(symbols, cursor) {
                    let title = format!("Import as `{}{}`", qualified_prefix, unbound_name);
                    let import_text = format!("use {}{}", qualified_prefix, unbound_name);
                    let text_edit = auto_import_text_edit(import_text, import_insertion_info);
                    code_actions.push(access_chain_code_action(
                        title,
                        text_edit,
                        diag.clone(),
                        file_url.clone(),
                    ));
                }
            }
        }
    }
}

/// Create auto-fixes for a two-element access chain where
/// the first element is unbound.
fn two_element_access_chain_autofixes<'a, I, K>(
    code_actions: &mut Vec<CodeAction>,
    symbols: &Symbols,
    cursor: &CursorContext,
    file_url: Url,
    unbound_first_name: Name,
    second_name: Name,
    mod_members: I,
    diag: Diagnostic,
) where
    I: Iterator<Item = (ModuleIdent, K)>,
    K: Iterator<Item = Symbol>,
{
    let Some(unbound_name_lsp_pos) =
        loc_start_to_lsp_position_opt(&symbols.files, &unbound_first_name.loc)
    else {
        return;
    };
    for (mod_ident, mod_members) in mod_members {
        for member_name in mod_members {
            if mod_ident.value.module.value() == unbound_first_name.value
                && member_name == second_name.value
            {
                let qualified_prefix = format!("{}::", mod_ident.value.address);
                let text_edit = TextEdit {
                    range: Range {
                        start: unbound_name_lsp_pos,
                        end: unbound_name_lsp_pos,
                    },
                    new_text: qualified_prefix.clone(),
                };
                let title = format!(
                    "Qualify as `{}{}::{}`",
                    qualified_prefix, unbound_first_name, second_name
                );
                code_actions.push(access_chain_code_action(
                    title,
                    text_edit,
                    diag.clone(),
                    file_url.clone(),
                ));
                if let Some(import_insertion_info) = import_insertion_info(symbols, cursor) {
                    let title = format!(
                        "Import as `{}{}::{}`",
                        qualified_prefix, unbound_first_name, second_name
                    );
                    let import_text = format!(
                        "use {}{}::{}",
                        qualified_prefix, unbound_first_name, second_name
                    );
                    let text_edit = auto_import_text_edit(import_text, import_insertion_info);
                    code_actions.push(access_chain_code_action(
                        title,
                        text_edit,
                        diag.clone(),
                        file_url.clone(),
                    ));
                }
            }
        }
    }
}

/// Create code action for fixing access chain.
fn access_chain_code_action(
    title: String,
    text_edit: TextEdit,
    diag: Diagnostic,
    file_url: Url,
) -> CodeAction {
    let mut changes = HashMap::new();
    changes.insert(file_url.clone(), vec![text_edit]);
    CodeAction {
        title,
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diag.clone()]),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: None,
        disabled: None,
        data: None,
    }
}
