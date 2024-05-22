// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    context::Context,
    symbols::{type_to_ide_string, DefInfo, Symbols},
};
use lsp_server::Request;
use lsp_types::{
    InlayHint, InlayHintKind, InlayHintLabel, InlayHintLabelPart, InlayHintParams, Position,
};

/// Handles inlay hints request of the language server
pub fn on_inlay_hint_request(context: &Context, request: &Request, symbols: &Symbols) {
    let parameters = serde_json::from_value::<InlayHintParams>(request.params.clone())
        .expect("could not deserialize inlay hints request");

    let fpath = parameters.text_document.uri.to_file_path().unwrap();
    eprintln!(
        "inlay_hints_request (types: {}): {:?}",
        context.inlay_type_hints, fpath
    );
    let mut hints: Vec<InlayHint> = vec![];

    if context.inlay_type_hints {
        if let Some(file_defs) = symbols.file_mods().get(&fpath) {
            for mod_defs in file_defs {
                for untyped_def_loc in mod_defs.untyped_defs() {
                    if let Some(DefInfo::Local(n, t, _, _)) = symbols.def_info(untyped_def_loc) {
                        let position = Position {
                            line: untyped_def_loc.start().line,
                            character: untyped_def_loc.start().character + n.len() as u32,
                        };
                        let colon_label = InlayHintLabelPart {
                            value: ": ".to_string(),
                            tooltip: None,
                            location: None,
                            command: None,
                        };
                        let type_label = InlayHintLabelPart {
                            value: type_to_ide_string(t),
                            tooltip: None,
                            location: None,
                            command: None,
                        };
                        let h = InlayHint {
                            position,
                            label: InlayHintLabel::LabelParts(vec![colon_label, type_label]),
                            kind: Some(InlayHintKind::TYPE),
                            text_edits: None,
                            tooltip: None,
                            padding_left: None,
                            padding_right: None,
                            data: None,
                        };
                        hints.push(h);
                    }
                }
            }
        };
    }

    let response = lsp_server::Response::new_ok(request.id.clone(), hints);
    if let Err(err) = context
        .connection
        .sender
        .send(lsp_server::Message::Response(response))
    {
        eprintln!("could not send inlay thing response: {:?}", err);
    }
}
