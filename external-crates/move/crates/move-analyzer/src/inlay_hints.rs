// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    context::Context,
    symbols::{
        on_hover_markup, type_to_ide_string, DefInfo, ModuleDefs, SymbolicatorRunner, Symbols,
    },
};
use lsp_server::Request;
use lsp_types::{
    InlayHint, InlayHintKind, InlayHintLabel, InlayHintLabelPart, InlayHintParams,
    InlayHintTooltip, Position,
};

use move_compiler::{naming::ast as N, shared::Identifier};
use std::path::PathBuf;

/// Handles inlay hints request of the language server
pub fn on_inlay_hint_request(context: &Context, request: &Request) {
    let parameters = serde_json::from_value::<InlayHintParams>(request.params.clone())
        .expect("could not deserialize inlay hints request");

    let fpath = parameters.text_document.uri.to_file_path().unwrap();
    eprintln!(
        "inlay_hints_request (types: {}, params: {}): {:?}",
        context.inlay_type_hints, context.inlay_param_hints, fpath
    );
    let hints = if context.inlay_type_hints || context.inlay_param_hints {
        inlay_hints(context, fpath).unwrap_or_default()
    } else {
        vec![]
    };

    let response = lsp_server::Response::new_ok(request.id.clone(), hints);
    if let Err(err) = context
        .connection
        .sender
        .send(lsp_server::Message::Response(response))
    {
        eprintln!("could not send inlay thing response: {:?}", err);
    }
}

fn inlay_hints(context: &Context, fpath: PathBuf) -> Option<Vec<InlayHint>> {
    let symbols_map = &context.symbols.lock().ok()?;
    let symbols =
        SymbolicatorRunner::root_dir(&fpath).and_then(|pkg_path| symbols_map.get(&pkg_path))?;
    inlay_hints_internal(
        symbols,
        fpath,
        context.inlay_type_hints,
        context.inlay_param_hints,
    )
}

fn inlay_type_hints_internal(symbols: &Symbols, mod_defs: &ModuleDefs, hints: &mut Vec<InlayHint>) {
    for untyped_def_loc in mod_defs.untyped_defs() {
        let start_position = symbols.files.start_position(untyped_def_loc);
        if let Some(DefInfo::Local(n, t, _, _, _)) = symbols.def_info(untyped_def_loc) {
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
                value: type_to_ide_string(t, /* verbose */ false),
                tooltip: None,
                location: None,
                command: None,
            };
            let h = InlayHint {
                position,
                label: InlayHintLabel::LabelParts(vec![colon_label, type_label]),
                kind: Some(InlayHintKind::TYPE),
                text_edits: None,
                tooltip: additional_type_hint_info(t, symbols),
                padding_left: None,
                padding_right: None,
                data: None,
            };
            hints.push(h);
        }
    }
}

fn inlay_param_hints_internal(
    symbols: &Symbols,
    mod_defs: &ModuleDefs,
    hints: &mut Vec<InlayHint>,
) {
    for call_info in mod_defs.call_infos.values() {
        let Some(def_loc) = call_info.def_loc else {
            continue;
        };
        let Some(DefInfo::Function(.., args, _, _, _)) = symbols.def_info.get(&def_loc) else {
            continue;
        };
        if call_info.dot_call && args.is_empty() {
            // methods should have at least one argument
            continue;
        };
        for (sp!(def_loc, name), use_loc) in args
            .iter()
            .skip(if call_info.dot_call { 1 } else { 0 })
            .zip(&call_info.arg_locs)
        {
            let name_label = InlayHintLabelPart {
                value: name.to_string(),
                tooltip: None,
                location: None,
                command: None,
            };
            let colon_label = InlayHintLabelPart {
                value: ": ".to_string(),
                tooltip: None,
                location: None,
                command: None,
            };
            let position = symbols.files.start_position(use_loc);
            let tooltip = symbols.def_info(def_loc).map(|arg_def_info| {
                // see doc comment for `additional_type_hint_info` to see
                // why we only support on hover tooltip for now
                InlayHintTooltip::MarkupContent(on_hover_markup(arg_def_info))
            });
            let h = InlayHint {
                position: position.into(),
                label: InlayHintLabel::LabelParts(vec![name_label, colon_label]),
                kind: Some(InlayHintKind::PARAMETER),
                text_edits: None,
                tooltip,
                padding_left: None,
                padding_right: None,
                data: None,
            };
            hints.push(h);
        }
    }
}

pub fn inlay_hints_internal(
    symbols: &Symbols,
    fpath: PathBuf,
    type_hints: bool,
    param_hints: bool,
) -> Option<Vec<InlayHint>> {
    let mut hints: Vec<InlayHint> = vec![];
    let file_defs = symbols.file_mods.get(&fpath)?;
    for mod_defs in file_defs {
        if type_hints {
            inlay_type_hints_internal(symbols, mod_defs, &mut hints);
        }
        if param_hints {
            inlay_param_hints_internal(symbols, mod_defs, &mut hints);
        }
    }
    Some(hints)
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
fn additional_type_hint_info(sp!(_, t): &N::Type, symbols: &Symbols) -> Option<InlayHintTooltip> {
    if let N::Type_::Ref(_, t) = t {
        return additional_type_hint_info(t, symbols);
    }
    let N::Type_::Apply(_, sp!(_, type_name), _) = t else {
        return None;
    };
    let N::TypeName_::ModuleType(mod_ident, struct_name) = type_name else {
        return None;
    };

    let mod_defs = symbols
        .file_mods
        .values()
        .flatten()
        .find(|m| m.ident() == &mod_ident.value)?;

    let struct_def = mod_defs.structs().get(&struct_name.value())?;

    let struct_def_info = symbols.def_info(&struct_def.name_loc)?;

    Some(InlayHintTooltip::MarkupContent(on_hover_markup(
        struct_def_info,
    )))
}
