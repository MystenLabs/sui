// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    completions::utils::{
        addr_to_ide_string, all_mod_enums_to_import, all_mod_functions_to_import,
        all_mod_structs_to_import, auto_import_text_edit, compute_cursor, import_insertion_info,
    },
    context::Context,
    symbols::{
        Symbols,
        compilation::{CachedPackages, CompiledPkgInfo, get_compiled_pkg},
        cursor::{ChainInfo, CursorContext},
        runner::SymbolicatorRunner,
    },
    utils::loc_start_to_lsp_position_opt,
};

use lsp_server::{Message, Request, Response};
use lsp_types::{
    CodeAction, CodeActionKind, CodeActionParams, Diagnostic, Position, Range, TextEdit,
    WorkspaceEdit,
};
use move_symbol_pool::Symbol;
use std::{
    collections::{BTreeSet, HashMap},
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
    UnboundFunction,
}

impl SingleElementChainDiagPrefix {
    fn as_str(&self) -> &'static str {
        // we include `'` at the end of the prefix as some of the prefixes
        // are shared with error messages that cannot be auto-fixed
        match self {
            SingleElementChainDiagPrefix::UnboundType => "Unbound type '",
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
    pkg_dependencies: Arc<Mutex<CachedPackages>>,
    implicit_deps: Dependencies,
) {
    let response = Response::new_ok(
        request.id.clone(),
        access_chain_autofix_actions(
            context,
            request,
            ide_files_root,
            pkg_dependencies,
            implicit_deps,
        ),
    );
    eprintln!("code_action_request: {:?}", request);
    if let Err(err) = context.connection.sender.send(Message::Response(response)) {
        eprintln!("could not send code action response: {:?}", err);
    }
}

/// Computes code actions related to access chain autofixes.
fn access_chain_autofix_actions(
    context: &Context,
    request: &Request,
    ide_files_root: VfsPath,
    pkg_dependencies: Arc<Mutex<CachedPackages>>,
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

    let file_url = params.text_document.uri.clone();
    let file_path = PathBuf::from(file_url.path());
    let Some(pkg_path) = SymbolicatorRunner::root_dir(&file_path) else {
        return code_actions;
    };

    let Ok((Some(mut compiled_pkg_info), _)) = get_compiled_pkg(
        pkg_dependencies.clone(),
        ide_files_root,
        &pkg_path,
        Some(vec![]),
        LintLevel::None,
        implicit_deps,
    ) else {
        return code_actions;
    };

    let mut symbol_map = context.symbols.lock().unwrap();
    let Some(symbols) = symbol_map.get_mut(&pkg_path) else {
        return code_actions;
    };

    for diag in params.context.diagnostics.into_iter() {
        let file_url = params.text_document.uri.clone();
        let err_pos = diag.range.start;
        let err_msg = diag.message.clone();
        access_chain_autofix_actions_for_error(
            symbols,
            &mut compiled_pkg_info,
            file_url,
            err_pos,
            err_msg,
            Some(diag.clone()),
            &mut code_actions,
        );
    }

    code_actions
}

/// Public function to also be used in tests.
pub fn access_chain_autofix_actions_for_error(
    symbols: &mut Symbols,
    compiled_pkg_info: &mut CompiledPkgInfo,
    file_url: Url,
    err_pos: Position,
    err_msg: String,
    diag: Option<Diagnostic>,
    code_actions: &mut Vec<CodeAction>,
) {
    if !SingleElementChainDiagPrefix::iter().any(|prefix| err_msg.starts_with(prefix.as_str()))
        && !TwoElementChainDiagPrefix::iter().any(|prefix| err_msg.starts_with(prefix.as_str()))
    {
        return;
    }
    // compute cursor and update symbols with it
    let file_path: PathBuf = file_url.path().into();
    compute_cursor(symbols, compiled_pkg_info, &file_path, err_pos);
    let Some(ref cursor) = symbols.cursor_context else {
        return;
    };
    let Some(ChainInfo {
        chain,
        kind: _,
        inside_use,
    }) = cursor.find_access_chain()
    else {
        return;
    };
    if inside_use {
        // no auto-fixes in imports
        return;
    }
    match chain.value {
        NameAccessChain_::Single(path_entry) => {
            SingleElementChainDiagPrefix::iter().for_each(|prefix| match prefix {
                SingleElementChainDiagPrefix::UnboundType => {
                    if err_msg.starts_with(prefix.as_str()) {
                        single_element_access_chain_autofixes(
                            code_actions,
                            symbols,
                            cursor,
                            file_url.clone(),
                            path_entry.name,
                            all_mod_structs_to_import(symbols, cursor)
                                .chain(all_mod_enums_to_import(symbols, cursor)),
                            diag.clone(),
                        );
                    }
                }
                SingleElementChainDiagPrefix::UnboundFunction => {
                    if err_msg.starts_with(prefix.as_str()) {
                        single_element_access_chain_autofixes(
                            code_actions,
                            symbols,
                            cursor,
                            file_url.clone(),
                            path_entry.name,
                            all_mod_functions_to_import(symbols, cursor),
                            diag.clone(),
                        );
                    }
                }
            });
        }
        NameAccessChain_::Path(name_path) => {
            if let LeadingNameAccess_::Name(unbound_name) = name_path.root.name.value {
                if name_path.entries.len() == 1 {
                    // we assume that diagnostic reporting unbound chain component
                    // is on the first element of the chain
                    if unbound_name.loc.contains(&cursor.loc) {
                        TwoElementChainDiagPrefix::iter().for_each(|prefix| match prefix {
                            TwoElementChainDiagPrefix::UnresolvedName => {
                                if err_msg.starts_with(prefix.as_str()) {
                                    two_element_access_chain_autofixes(
                                        code_actions,
                                        symbols,
                                        file_url.clone(),
                                        unbound_name,
                                        name_path.entries[0].name,
                                        all_mod_structs_to_import(symbols, cursor)
                                            .chain(all_mod_enums_to_import(symbols, cursor))
                                            .chain(all_mod_functions_to_import(symbols, cursor)),
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

/// Create auto-fixes for a single unbound element in an access chain.
fn single_element_access_chain_autofixes<I, K>(
    code_actions: &mut Vec<CodeAction>,
    symbols: &Symbols,
    cursor: &CursorContext,
    file_url: Url,
    unbound_name: Name,
    mod_members: I,
    diag: Option<Diagnostic>,
) where
    I: Iterator<Item = (ModuleIdent, K)>,
    K: Iterator<Item = Symbol>,
{
    let Some(unbound_name_lsp_pos) =
        loc_start_to_lsp_position_opt(&symbols.files, &unbound_name.loc)
    else {
        return;
    };
    let mut added_modules = BTreeSet::new();
    for (mod_ident, mod_members) in mod_members {
        // add pkg::module if module name matches unbound name
        if mod_ident.value.module.value() == unbound_name.value
            && !added_modules.contains(&mod_ident)
        {
            added_modules.insert(mod_ident);
            let autofix_prefix = format!("{}::", addr_to_ide_string(&mod_ident.value.address));
            let text_edit = TextEdit {
                range: Range {
                    start: unbound_name_lsp_pos,
                    end: unbound_name_lsp_pos,
                },
                new_text: autofix_prefix.clone(),
            };
            let title = format!("Qualify as `{}{}`", autofix_prefix, unbound_name);
            code_actions.push(access_chain_code_action(
                title,
                text_edit,
                diag.clone(),
                file_url.clone(),
            ));
            if let Some(import_insertion_info) = import_insertion_info(symbols, cursor) {
                let title = format!("Import as `{}{}`", autofix_prefix, unbound_name);
                let import_text = format!("use {}{}", autofix_prefix, unbound_name);
                let text_edit = auto_import_text_edit(import_text, import_insertion_info);
                code_actions.push(access_chain_code_action(
                    title,
                    text_edit,
                    diag.clone(),
                    file_url.clone(),
                ));
            }
        }
        // add pkg::module::member if member name matches unbound name
        for member_name in mod_members {
            if member_name == unbound_name.value {
                let autofix_prefix = format!(
                    "{}::{}::",
                    addr_to_ide_string(&mod_ident.value.address),
                    mod_ident.value.module
                );
                let text_edit = TextEdit {
                    range: Range {
                        start: unbound_name_lsp_pos,
                        end: unbound_name_lsp_pos,
                    },
                    new_text: autofix_prefix.clone(),
                };
                let title = format!("Qualify as `{}{}`", autofix_prefix, unbound_name);
                code_actions.push(access_chain_code_action(
                    title,
                    text_edit,
                    diag.clone(),
                    file_url.clone(),
                ));
                if let Some(import_insertion_info) = import_insertion_info(symbols, cursor) {
                    let title = format!("Import as `{}{}`", autofix_prefix, unbound_name);
                    let import_text = format!("use {}{}", autofix_prefix, unbound_name);
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
fn two_element_access_chain_autofixes<I, K>(
    code_actions: &mut Vec<CodeAction>,
    symbols: &Symbols,
    file_url: Url,
    unbound_first_name: Name,
    second_name: Name,
    mod_members: I,
    diag: Option<Diagnostic>,
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
                let qualified_prefix =
                    format!("{}::", addr_to_ide_string(&mod_ident.value.address));
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
            }
        }
    }
}

/// Create code action for fixing access chain.
fn access_chain_code_action(
    title: String,
    text_edit: TextEdit,
    diag: Option<Diagnostic>,
    file_url: Url,
) -> CodeAction {
    let mut changes = HashMap::new();
    changes.insert(file_url.clone(), vec![text_edit]);
    CodeAction {
        title,
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: diag.map(|d| vec![d]),
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
