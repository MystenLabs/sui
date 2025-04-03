// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    completions::utils::{
        auto_import_text_edit, exclude_member_from_import, import_insertion_info,
    },
    context::Context,
    symbols::{
        get_symbols, ChainInfo, CursorContext, DefInfo, PrecomputedPkgInfo, SymbolicatorRunner,
        Symbols,
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
    expansion::ast::ModuleIdent_, linters::LintLevel, parser::ast::NameAccessChain_, shared::Name,
};
use move_package::source_package::parsed_manifest::Dependencies;

// The following reflects prefixes of error messages
// subject to auto fixes. The constants representing them
// cannot be lifted directly from the Move compiler,
// as they are (at least most of them) dynamically generated
// (in naming/translate.rs).
#[derive(Debug, EnumIter)]
enum DiagErrPrefix {
    UnboundType,
    UnboundStruct,
    UnboundStructOrEnum,
    UnboundTypeOrFunction,
    UnboundFunction,
}

impl DiagErrPrefix {
    fn as_str(&self) -> &'static str {
        // we include `'` at the end of the prefix as some of the prefixes
        // are shared with error messages that cannot be auto-fixed
        match self {
            DiagErrPrefix::UnboundType => "Unbound type '",
            DiagErrPrefix::UnboundStruct => "Unbound struct '",
            DiagErrPrefix::UnboundStructOrEnum => "Unbound struct or enum '",
            DiagErrPrefix::UnboundTypeOrFunction => "Unbound datatype or function '",
            DiagErrPrefix::UnboundFunction => "Unbound function '",
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

    if !params
        .context
        .diagnostics
        .iter()
        .any(|diag| DiagErrPrefix::iter().any(|prefix| diag.message.starts_with(prefix.as_str())))
    {
        return code_actions;
    }

    let path: PathBuf = params.text_document.uri.path().into();
    let Some(pkg_path) = SymbolicatorRunner::root_dir(&path) else {
        return code_actions;
    };

    for diag in params.context.diagnostics.into_iter() {
        if !DiagErrPrefix::iter().any(|prefix| diag.message.starts_with(prefix.as_str())) {
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
        if let NameAccessChain_::Single(path_entry) = chain.value {
            if diag
                .message
                .starts_with(DiagErrPrefix::UnboundFunction.as_str())
            {
                single_id_access_chain_autofixes(
                    &mut code_actions,
                    &symbols,
                    cursor,
                    params.text_document.uri.clone(),
                    path_entry.name,
                    all_mod_functions(&symbols, cursor),
                    diag,
                );
            }
        }
    }

    code_actions
}

/// Returns an iterator over module identifiers and their function keys.
fn all_mod_functions<'a>(
    symbols: &'a Symbols,
    cursor: &'a CursorContext,
) -> impl Iterator<Item = (&'a ModuleIdent_, impl Iterator<Item = &'a Symbol> + 'a)> + 'a {
    symbols.file_mods.values().flatten().map(|mod_defs| {
        (
            &mod_defs.ident,
            mod_defs.functions.iter().filter_map(|(member_name, fdef)| {
                if let Some(DefInfo::Function(_, visibility, ..)) = symbols.def_info(&fdef.name_loc)
                {
                    if exclude_member_from_import(mod_defs, cursor.module, visibility) {
                        return None;
                    }
                }
                Some(member_name)
            }),
        )
    })
}

/// Create auto-fixes for a single unbound identifier in an access chain.
fn single_id_access_chain_autofixes<'a, I, K>(
    code_actions: &mut Vec<CodeAction>,
    symbols: &Symbols,
    cursor: &CursorContext,
    file_url: Url,
    unbound_name: Name,
    mod_members: I,
    diag: Diagnostic,
) where
    I: Iterator<Item = (&'a ModuleIdent_, K)>,
    K: Iterator<Item = &'a Symbol>,
{
    let Some(unbound_name_lsp_pos) =
        loc_start_to_lsp_position_opt(&symbols.files, &unbound_name.loc)
    else {
        return;
    };
    for (mod_ident, mod_members) in mod_members {
        for member_name in mod_members {
            if member_name == &unbound_name.value {
                let qualified_prefix = format!("{}::{}::", mod_ident.address, mod_ident.module);
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
