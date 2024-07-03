// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    context::Context,
    symbols::{on_hover_markup, type_to_ide_string, DefInfo, SymbolicatorRunner, Symbols},
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
        "inlay_hints_request (types: {}): {:?}",
        context.inlay_type_hints, fpath
    );
    let hints = if context.inlay_type_hints {
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
    inlay_hints_internal(symbols, fpath)
}

fn inlay_hints_internal(symbols: &Symbols, fpath: PathBuf) -> Option<Vec<InlayHint>> {
    let mut hints: Vec<InlayHint> = vec![];
    let file_defs = symbols.file_mods.get(&fpath)?;
    for mod_defs in file_defs {
        for untyped_def_loc in mod_defs.untyped_defs() {
            let start_position = symbols.files.start_position(untyped_def_loc);
            if let DefInfo::Local(n, t, _, _, _) = symbols.def_info(untyped_def_loc)? {
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

#[cfg(test)]
fn validate_type_hint(hints: &[InlayHint], path: &std::path::Path, line: u32, col: u32, ty: &str) {
    let lsp_line = line - 1; // 0th based
    let Some(label_parts) = hints.iter().find_map(|h| {
        if h.position.line == lsp_line && h.position.character == col {
            if let InlayHintLabel::LabelParts(parts) = &h.label {
                return Some(parts);
            }
        }
        None
    }) else {
        panic!("hint not found at line {line} and col {col} in {path:?}");
    };

    let found_ty = &label_parts[1].value;
    assert!(
        found_ty == ty,
        "incorrect type of hint (found '{}' instead of expected '{}') at line {} and col {} in {:?}",
            found_ty, ty, line, col, path
    );
}

#[test]
/// Tests if inlay type hints are generated correctly.
fn inlay_type_test() {
    use crate::symbols::get_symbols;
    use move_compiler::linters::LintLevel;
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex},
    };
    use vfs::{impls::memory::MemoryFS, VfsPath};

    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    path.push("tests/inlay-hints");

    let ide_files_layer: VfsPath = MemoryFS::new().into();
    let (symbols_opt, _) = get_symbols(
        Arc::new(Mutex::new(BTreeMap::new())),
        ide_files_layer,
        path.as_path(),
        LintLevel::None,
    )
    .unwrap();
    let symbols = symbols_opt.unwrap();

    let mut fpath = path.clone();
    fpath.push("sources/type_hints.move");
    let cpath = dunce::canonicalize(&fpath).unwrap();

    let hints = inlay_hints_internal(&symbols, cpath).unwrap();
    validate_type_hint(&hints, &path, 8, 22, "u64");
    validate_type_hint(&hints, &path, 9, 24, "InlayHints::type_hints::SomeStruct");
    validate_type_hint(&hints, &path, 25, 23, "u64");
    validate_type_hint(&hints, &path, 26, 25, "InlayHints::type_hints::SomeStruct");
    validate_type_hint(&hints, &path, 27, 27, "u64");
    validate_type_hint(&hints, &path, 28, 29, "InlayHints::type_hints::SomeStruct");
}
