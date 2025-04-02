// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    context::Context,
    symbols::{get_symbols, ChainInfo, PrecomputedPkgInfo, SymbolicatorRunner},
};

use lsp_server::{Message, Request, Response};
use lsp_types::{CodeAction, CodeActionKind, CodeActionParams};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;
use vfs::VfsPath;

use move_compiler::linters::LintLevel;
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

    for diag in params.context.diagnostics.iter() {
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
        let Some(cursor) = symbols.cursor_context else {
            continue;
        };
        let Some(ChainInfo {
            chain,
            kind: chain_kind,
            inside_use,
        }) = cursor.find_access_chain()
        else {
            continue;
        };
        if inside_use {
            // no auto-fixes in imports
            continue;
        }
        eprintln!("CHAIN ({:?}): {:?}", chain_kind, chain);
    }

    code_actions
}
