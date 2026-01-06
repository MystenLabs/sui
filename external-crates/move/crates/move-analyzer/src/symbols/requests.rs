// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module contains code responsible for handling symbols-related requests.

use crate::{
    context::Context,
    symbols::{
        Symbols,
        def_info::DefInfo,
        mod_defs::{MemberDef, MemberDefInfo, ModuleDefs},
        runner::SymbolicatorRunner,
        use_def::UseDef,
    },
    utils::lsp_position_to_loc,
};

use lsp_server::{Message, Request, RequestId, Response};
use lsp_types::{
    DocumentSymbol, DocumentSymbolParams, GotoDefinitionParams, Hover, HoverContents, HoverParams,
    Location, MarkupContent, MarkupKind, Position, Range, ReferenceParams, SymbolKind,
    request::GotoTypeDefinitionParams,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    vec,
};
use url::Url;

use move_compiler::naming::ast::TypeInner;
use move_ir_types::location::*;

/// Handles go-to-def request of the language server
pub fn on_go_to_def_request(context: &Context, request: &Request) {
    let symbols_map = &context.symbols.lock().unwrap();
    let parameters = serde_json::from_value::<GotoDefinitionParams>(request.params.clone())
        .expect("could not deserialize go-to-def request");

    let fpath = parameters
        .text_document_position_params
        .text_document
        .uri
        .to_file_path()
        .unwrap();
    let loc = parameters.text_document_position_params.position;
    let line = loc.line;
    let col = loc.character;

    on_use_request(
        context,
        symbols_map,
        &fpath,
        line,
        col,
        request.id.clone(),
        |u, symbols| {
            let loc = def_ide_location(&u.def_loc, symbols);
            Some(serde_json::to_value(loc).unwrap())
        },
    );
}

/// Handles go-to-type-def request of the language server
pub fn on_go_to_type_def_request(context: &Context, request: &Request) {
    let symbols_map = &context.symbols.lock().unwrap();
    let parameters = serde_json::from_value::<GotoTypeDefinitionParams>(request.params.clone())
        .expect("could not deserialize go-to-type-def request");

    let fpath = parameters
        .text_document_position_params
        .text_document
        .uri
        .to_file_path()
        .unwrap();
    let loc = parameters.text_document_position_params.position;
    let line = loc.line;
    let col = loc.character;

    on_use_request(
        context,
        symbols_map,
        &fpath,
        line,
        col,
        request.id.clone(),
        |u, symbols| {
            u.type_def_loc.map(|def_loc| {
                let loc = def_ide_location(&def_loc, symbols);
                serde_json::to_value(loc).unwrap()
            })
        },
    );
}

/// Handles go-to-references request of the language server
pub fn on_references_request(context: &Context, request: &Request) {
    let symbols_map = &context.symbols.lock().unwrap();
    let parameters = serde_json::from_value::<ReferenceParams>(request.params.clone())
        .expect("could not deserialize references request");

    let fpath = parameters
        .text_document_position
        .text_document
        .uri
        .to_file_path()
        .unwrap();
    let loc = parameters.text_document_position.position;
    let line = loc.line;
    let col = loc.character;
    let include_decl = parameters.context.include_declaration;

    on_use_request(
        context,
        symbols_map,
        &fpath,
        line,
        col,
        request.id.clone(),
        |u, symbols| {
            let def_posn = symbols.files.file_start_position_opt(&u.def_loc)?;
            symbols
                .references
                .get(&u.def_loc)
                .map(|s| {
                    let mut locs = vec![];

                    for ref_loc in s {
                        if include_decl
                            || !(Into::<Position>::into(def_posn.position) == ref_loc.start
                                && def_posn.file_hash == ref_loc.fhash)
                        {
                            let end_pos = Position {
                                line: ref_loc.start.line,
                                character: ref_loc.col_end,
                            };
                            let range = Range {
                                start: ref_loc.start,
                                end: end_pos,
                            };
                            let path = symbols.files.file_path(&ref_loc.fhash);
                            locs.push(Location {
                                uri: Url::from_file_path(path).unwrap(),
                                range,
                            });
                        }
                    }
                    locs
                })
                .map(|locs| serde_json::to_value(locs).unwrap())
        },
    );
}

/// Handles hover request of the language server
pub fn on_hover_request(context: &Context, request: &Request) {
    let symbols_map = &context.symbols.lock().unwrap();
    let parameters = serde_json::from_value::<HoverParams>(request.params.clone())
        .expect("could not deserialize hover request");

    let fpath = parameters
        .text_document_position_params
        .text_document
        .uri
        .to_file_path()
        .unwrap();
    let loc = parameters.text_document_position_params.position;
    let line = loc.line;
    let col = loc.character;

    on_use_request(
        context,
        symbols_map,
        &fpath,
        line,
        col,
        request.id.clone(),
        |u, symbols| {
            let Some(info) = symbols.def_info.get(&u.def_loc) else {
                return Some(serde_json::to_value(Option::<lsp_types::Location>::None).unwrap());
            };
            let contents =
                if let Some(guard_info) = maybe_convert_for_guard(info, &fpath, &loc, symbols) {
                    HoverContents::Markup(on_hover_markup(&guard_info))
                } else {
                    HoverContents::Markup(on_hover_markup(info))
                };
            let range = None;
            Some(serde_json::to_value(Hover { contents, range }).unwrap())
        },
    );
}

/// Helper function to handle language server queries related to identifier uses
pub fn on_use_request(
    context: &Context,
    symbols_map: &BTreeMap<PathBuf, Symbols>,
    use_fpath: &PathBuf,
    use_line: u32,
    use_col: u32,
    id: RequestId,
    use_def_action: impl Fn(&UseDef, &Symbols) -> Option<serde_json::Value>,
) {
    let mut result = None;

    if let Some(symbols) =
        SymbolicatorRunner::root_dir(use_fpath).and_then(|pkg_path| symbols_map.get(&pkg_path))
        && let Some(mod_symbols) = symbols.file_use_defs.get(use_fpath)
        && let Some(uses) = mod_symbols.get(use_line)
    {
        for u in uses {
            if use_col >= u.col_start && use_col <= u.col_end {
                result = use_def_action(&u, symbols);
            }
        }
    }
    eprintln!(
        "about to send use response (symbols found: {})",
        result.is_some()
    );

    if result.is_none() {
        result = Some(serde_json::to_value(Option::<lsp_types::Location>::None).unwrap());
    }

    // unwrap will succeed based on the logic above which the compiler is unable to figure out
    // without using Option
    let response = Response::new_ok(id, result.unwrap());
    if let Err(err) = context.connection.sender.send(Message::Response(response)) {
        eprintln!("could not send use response: {:?}", err);
    }
}

/// Handles document symbol request of the language server
#[allow(deprecated)]
pub fn on_document_symbol_request(context: &Context, request: &Request) {
    let symbols_map = &context.symbols.lock().unwrap();
    let parameters = serde_json::from_value::<DocumentSymbolParams>(request.params.clone())
        .expect("could not deserialize document symbol request");

    let fpath = parameters.text_document.uri.to_file_path().unwrap();
    eprintln!("on_document_symbol_request: {:?}", fpath);

    let mut defs: Vec<DocumentSymbol> = vec![];
    if let Some(symbols) =
        SymbolicatorRunner::root_dir(&fpath).and_then(|pkg_path| symbols_map.get(&pkg_path))
    {
        let empty_mods: BTreeSet<ModuleDefs> = BTreeSet::new();
        let mods = symbols.file_mods.get(&fpath).unwrap_or(&empty_mods);

        for mod_def in mods {
            let name = mod_def.ident.module.clone().to_string();
            let detail = Some(mod_def.ident.clone().to_string());
            let kind = SymbolKind::MODULE;
            let Some(range) = symbols.files.lsp_range_opt(&mod_def.name_loc) else {
                continue;
            };

            let mut children = vec![];

            // handle constants
            for (sym, const_def) in &mod_def.constants {
                let Some(const_range) = symbols.files.lsp_range_opt(&const_def.name_loc) else {
                    continue;
                };
                children.push(DocumentSymbol {
                    name: sym.clone().to_string(),
                    detail: None,
                    kind: SymbolKind::CONSTANT,
                    range: const_range,
                    selection_range: const_range,
                    children: None,
                    tags: Some(vec![]),
                    deprecated: Some(false),
                });
            }

            // handle structs
            for (sym, struct_def) in &mod_def.structs {
                let Some(struct_range) = symbols.files.lsp_range_opt(&struct_def.name_loc) else {
                    continue;
                };

                let fields = struct_field_symbols(struct_def, symbols);
                children.push(DocumentSymbol {
                    name: sym.clone().to_string(),
                    detail: None,
                    kind: SymbolKind::STRUCT,
                    range: struct_range,
                    selection_range: struct_range,
                    children: Some(fields),
                    tags: Some(vec![]),
                    deprecated: Some(false),
                });
            }

            // handle enums
            for (sym, enum_def) in &mod_def.enums {
                let Some(enum_range) = symbols.files.lsp_range_opt(&enum_def.name_loc) else {
                    continue;
                };

                let variants = enum_variant_symbols(enum_def, symbols);
                children.push(DocumentSymbol {
                    name: sym.clone().to_string(),
                    detail: None,
                    kind: SymbolKind::ENUM,
                    range: enum_range,
                    selection_range: enum_range,
                    children: Some(variants),
                    tags: Some(vec![]),
                    deprecated: Some(false),
                });
            }

            // handle functions
            for (sym, func_def) in &mod_def.functions {
                let MemberDefInfo::Fun { attrs } = &func_def.info else {
                    continue;
                };
                let Some(func_range) = symbols.files.lsp_range_opt(&func_def.name_loc) else {
                    continue;
                };

                let mut detail = None;
                if !attrs.is_empty() {
                    detail = Some(format!("{:?}", attrs));
                }

                children.push(DocumentSymbol {
                    name: sym.clone().to_string(),
                    detail,
                    kind: SymbolKind::FUNCTION,
                    range: func_range,
                    selection_range: func_range,
                    children: None,
                    tags: Some(vec![]),
                    deprecated: Some(false),
                });
            }

            defs.push(DocumentSymbol {
                name,
                detail,
                kind,
                range,
                selection_range: range,
                children: Some(children),
                tags: Some(vec![]),
                deprecated: Some(false),
            });
        }
    }
    // unwrap will succeed based on the logic above which the compiler is unable to figure out
    let response = Response::new_ok(request.id.clone(), defs);
    if let Err(err) = context.connection.sender.send(Message::Response(response)) {
        eprintln!("could not send use response: {:?}", err);
    }
}

/// Helper function that takes a DefInfo, checks if it represents
/// a enum arm variable defintion, and if need be converts it
/// to the one that represents an enum guard variable (which
/// has immutable reference type regarldes of arm variable definition
/// type).
pub fn maybe_convert_for_guard(
    def_info: &DefInfo,
    use_fpath: &Path,
    position: &Position,
    symbols: &Symbols,
) -> Option<DefInfo> {
    // In Move match expressions with guards, variables bound in patterns have their original
    // type (T) at the binding site, but when accessed within the guard expression, they
    // appear as immutable references (&T).
    //
    // Example:
    //   match (value) {
    //       MyEnum::Variant(x) if x > 10 => { ... }
    //   }
    // In the pattern MyEnum::Variant(x): x is bound as type T
    // In the guard (if x > 10): x is accessed as type &T
    //
    // This function checks if the cursor is within the guard expression and converts
    // the type accordingly.

    let DefInfo::Local(name, ty, is_let, is_mut, guard_loc) = def_info else {
        return None;
    };
    // If this local has an associated guard location, check if cursor is inside it
    let gloc = (*guard_loc)?;
    let fhash = symbols.file_hash(use_fpath)?;
    let loc = lsp_position_to_loc(&symbols.files, fhash, position)?;

    // If the cursor position is within the guard expression, convert type to &T
    if gloc.contains(&loc) {
        let new_ty = sp(
            ty.loc,
            TypeInner::Ref(false, sp(ty.loc, ty.value.base_type_())).into(),
        );
        return Some(DefInfo::Local(*name, new_ty, *is_let, *is_mut, *guard_loc));
    }
    None
}

pub fn def_info_doc_string(def_info: &DefInfo) -> Option<String> {
    match def_info {
        DefInfo::Type(_) => None,
        DefInfo::Function(.., s) => s.clone(),
        DefInfo::Struct(.., s) => s.clone(),
        DefInfo::Enum(.., s) => s.clone(),
        DefInfo::Variant(.., s) => s.clone(),
        DefInfo::Field(.., s) => s.clone(),
        DefInfo::Local(..) => None,
        DefInfo::Const(.., s) => s.clone(),
        DefInfo::Module(_, s) => s.clone(),
    }
}

pub fn on_hover_markup(info: &DefInfo) -> MarkupContent {
    // use rust for highlighting in Markdown until there is support for Move
    let value = if let Some(s) = &def_info_doc_string(info) {
        format!("```rust\n{}\n```\n{}", info, s)
    } else {
        format!("```rust\n{}\n```", info)
    };
    MarkupContent {
        kind: MarkupKind::Markdown,
        value,
    }
}

fn def_ide_location(def_loc: &Loc, symbols: &Symbols) -> Location {
    // TODO: Do we need beginning and end of the definition? Does not seem to make a
    // difference from the IDE perspective as the cursor goes to the beginning anyway (at
    // least in VSCode).
    let span = symbols.files.position_opt(def_loc).unwrap();
    let range = Range {
        start: span.start.into(),
        end: span.end.into(),
    };
    let path = symbols.files.file_path(&def_loc.file_hash());
    Location {
        uri: Url::from_file_path(path).unwrap(),
        range,
    }
}

/// Helper function to generate struct field symbols
#[allow(deprecated)]
fn struct_field_symbols(struct_def: &MemberDef, symbols: &Symbols) -> Vec<DocumentSymbol> {
    let mut fields: Vec<DocumentSymbol> = vec![];
    if let MemberDefInfo::Struct {
        field_defs,
        positional: _,
    } = &struct_def.info
    {
        for field_def in field_defs {
            let Some(field_range) = symbols.files.lsp_range_opt(&field_def.loc) else {
                continue;
            };

            fields.push(DocumentSymbol {
                name: field_def.name.clone().to_string(),
                detail: None,
                kind: SymbolKind::FIELD,
                range: field_range,
                selection_range: field_range,
                children: None,
                tags: Some(vec![]),
                deprecated: Some(false),
            });
        }
    }
    fields
}

/// Helper function to generate enum variant symbols
#[allow(deprecated)]
fn enum_variant_symbols(enum_def: &MemberDef, symbols: &Symbols) -> Vec<DocumentSymbol> {
    let mut variants: Vec<DocumentSymbol> = vec![];
    if let MemberDefInfo::Enum { variants_info } = &enum_def.info {
        for (name, (loc, _, _)) in variants_info {
            let Some(variant_range) = symbols.files.lsp_range_opt(loc) else {
                continue;
            };

            variants.push(DocumentSymbol {
                name: name.clone().to_string(),
                detail: None,
                kind: SymbolKind::ENUM_MEMBER,
                range: variant_range,
                selection_range: variant_range,
                children: None,
                tags: Some(vec![]),
                deprecated: Some(false),
            });
        }
    }
    variants
}
