// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    context::Context,
    symbols::{on_hover_markup, type_to_ide_string, DefInfo, Symbols},
};
use lsp_server::Request;
use lsp_types::{
    InlayHint, InlayHintKind, InlayHintLabel, InlayHintLabelPart, InlayHintParams,
    InlayHintTooltip, Position,
};

use move_compiler::{
    naming::ast as N,
    shared::{files::FilePosition, Identifier},
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
        if let Some(file_defs) = symbols.file_mods.get(&fpath) {
            for mod_defs in file_defs {
                for untyped_def_loc in mod_defs.untyped_defs() {
                    let start_position = symbols.files.start_position(untyped_def_loc);
                    if let Some(DefInfo::Local(n, t, _, _)) = symbols.def_info(untyped_def_loc) {
                        let position = Position {
                            line: start_position.line_offset() as u32,
                            character: start_position.column_offset() as u32 + n.len() as u32,
                        };
                        let colon_label = InlayHintLabelPart {
                            value: ": ".to_string(),
                            tooltip: None,
                            location: None,
                            command: None,
                        };
                        let type_label = InlayHintLabelPart {
                            value: type_to_ide_string(t, /* verbose */ true),
                            tooltip: None,
                            location: None,
                            command: None,
                        };
                        let h = InlayHint {
                            position,
                            label: InlayHintLabel::LabelParts(vec![colon_label, type_label]),
                            kind: Some(InlayHintKind::TYPE),
                            text_edits: None,
                            tooltip: additional_hint_info(t, symbols),
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

/// Helper function to compute additional optional info for the hint.
/// At this point it's just the on-hover information as support
/// for adding location of type definition does not seem to quite
/// work in the current version of VSCode
///
/// TODO: revisit adding location of type definition once current problems
/// are resolved (the main problem is that adding it enables a drop-down menu
/// containing options that are not supported for the type definition, such
/// as go-to-declaration, and which jump to weird locations in the file).
fn additional_hint_info(sp!(_, t): &N::Type, symbols: &Symbols) -> Option<InlayHintTooltip> {
    if let N::Type_::Ref(_, t) = t {
        return additional_hint_info(t, symbols);
    }
    let N::Type_::Apply(_, sp!(_, type_name), _) = t else {
        return None;
    };
    let N::TypeName_::ModuleType(mod_ident, struct_name) = type_name else {
        return None;
    };

    let Some(mod_defs) = symbols
        .file_mods
        .values()
        .flatten()
        .find(|m| m.ident() == &mod_ident.value)
    else {
        return None;
    };

    let Some(struct_def) = mod_defs.structs().get(&struct_name.value()) else {
        return None;
    };

    let Some(struct_def_info) = symbols.def_info(&struct_def.name_start()) else {
        return None;
    };

    Some(InlayHintTooltip::MarkupContent(on_hover_markup(
        struct_def_info,
    )))
}
