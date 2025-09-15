// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Provides `to_doc` for various elemnts of the Move model summary to generate `RcDoc`s from
//! `pretty`. This allows for easy printing of the summary in a human-readable format, plus
//! rendering it out in other settings.

use crate::{source_kind::SourceKind, summary::*};

use indexmap::IndexMap;
use move_symbol_pool::Symbol;
use pretty::RcDoc;

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

type Doc = RcDoc<'static, ()>;

// -------------------------------------------------------------------------------------------------
// Trait
// -------------------------------------------------------------------------------------------------

pub trait ToDoc {
    fn to_doc(&self) -> Doc;
}

// -------------------------------------------------------------------------------------------------
// Macros
// -------------------------------------------------------------------------------------------------

macro_rules! cat {
    ($($e:expr),*) => {
        {
            let docs = vec![$($e),*];
            cat(docs)
        }
    };
}

// -------------------------------------------------------------------------------------------------
// Function Header Generation
// -------------------------------------------------------------------------------------------------

pub fn fun_header<K: SourceKind>(model_fun: &crate::model::Function<K>) -> Doc {
    use RcDoc as RD;

    fn returns_doc(tys: &[Type]) -> Doc {
        match tys {
            [] => RD::nil(),
            [t] => RD::text(": ").append(t.to_doc()),
            many => {
                let items = many.iter().map(|t| t.to_doc());
                RD::text(": ").append(RD::group(

                        RD::text("(")
                        .append(
                            RD::softline()
                                .append(RD::intersperse(
                                    items,
                                    RD::text(",").append(RD::softline()),
                                ))
                                .nest(4),
                        )
                        .append(RD::softline())
                        .append(RD::text(")")),
                ))
            }
        }
    }

    // TODO: Docs, Attributes
    let Function {
        visibility,
        entry,
        macro_,
        type_parameters,
        parameters,
        return_,
        doc: _,
        attributes: _,
        index: _,
        source_index: _,
    } = model_fun.summary();

    // Prefixes: [visibility] [entry] [macro]
    let mut prefix_parts: Vec<Doc> = Vec::new();
    let vis_doc = visibility.to_doc();
    if !matches!(visibility, Visibility::Private) {
        prefix_parts.push(vis_doc);
    }
    if *entry {
        prefix_parts.push(RD::text("entry"));
    }
    if *(macro_.as_ref().unwrap_or(&false)) {
        prefix_parts.push(RD::text("macro"));
    }

    // Join prefixes with spaces (if any).
    let prefix = if prefix_parts.is_empty() {
        RD::nil()
    } else {
        RD::intersperse(prefix_parts, RD::space()).append(RD::space())
    };

    // `fun name`
    let name_doc = RD::text("fun")
        .append(RD::space())
        .append(RD::as_string(model_fun.name()));

    // `<T...>` (optional)
    let tparams_doc = type_parameters.to_doc();

    // `(params...)`
    let params_doc = parameters.to_doc();

    // `: ret`
    let ret_doc = returns_doc(&return_);

    RD::group(
        prefix
            .append(name_doc)
            .append(tparams_doc)
            .append(params_doc)
            .append(ret_doc),
    )
}

// -------------------------------------------------------------------------------------------------
// model impls
// -------------------------------------------------------------------------------------------------

impl<'a, K: SourceKind> ToDoc for crate::model::Struct<'a, K> {
    fn to_doc(&self) -> Doc {
        let name = self.name();
        let summary = self.summary();

        // TODO: Docs, Attributes
        let Struct {
            abilities,
            attributes: _,
            doc: _,
            fields,
            index: _,
            type_parameters,
        } = summary;

        let type_parameters = type_parameters.to_doc();
        let abilities = if !abilities.0.is_empty() {
            cat!(RcDoc::nil(), RcDoc::text("has"), abilities.to_doc())
        } else {
            RcDoc::nil()
        };
        let fields = fields.to_doc();

        RcDoc::text("public struct")
            .append(RcDoc::space())
            .append(RcDoc::as_string(name))
            .append(type_parameters)
            .group()
            .append(abilities)
            .append(RcDoc::softline())
            .append(braces(fields))
            .group()
    }
}

impl<'a, K: SourceKind> ToDoc for crate::model::Enum<'a, K> {
    fn to_doc(&self) -> Doc {
        let name = self.name();
        let summary = self.summary();

        // TODO: Docs, Attributes
        let Enum {
            abilities,
            attributes: _,
            doc: _,
            index: _,
            type_parameters,
            variants,
        } = summary;

        let type_parameters = type_parameters.to_doc();
        let abilities = if !abilities.0.is_empty() {
            cat!(RcDoc::nil(), RcDoc::text("has"), abilities.to_doc())
        } else {
            RcDoc::nil()
        };
        let variants = variants.to_doc();

        RcDoc::text("public enum")
            .append(RcDoc::space())
            .append(RcDoc::as_string(name))
            .append(type_parameters)
            .group()
            .append(abilities)
            .append(RcDoc::hardline())
            .append(variants)
            .group()
    }
}

// -------------------------------------------------------------------------------------------------
// Summary Impls
// -------------------------------------------------------------------------------------------------

/// Build the *header line* for a Move function from its summary.
/// Example outputs (no trailing semicolon, just the header):
/// - `fun f<T>(x: u64): u64`
/// - `public entry fun g(a: u8, b: u8)`
/// - `public(friend) macro fun h<T, U>(x: T, y: U): (T, U)`

impl ToDoc for Visibility {
    fn to_doc(&self) -> Doc {
        use RcDoc as RD;
        match self {
            Visibility::Private => RD::nil(),
            Visibility::Public => RD::text("public"),
            Visibility::Friend => RD::text("public(friend)"),
            Visibility::Package => RD::text("public(package)"),
        }
    }
}

impl ToDoc for IndexMap<Symbol, Variant> {
    fn to_doc(&self) -> Doc {
        use RcDoc as RD;
        let variants = self.iter().map(|(name, v)| {
            RD::as_string(name)
                .append(RD::space())
                .append(v.to_doc())
                .group()
                .nest(4)
        });
        braces_block(comma_lines(variants.collect())).group()
    }
}

impl ToDoc for Variant {
    fn to_doc(&self) -> Doc {
        // TODO: Docs
        let Variant {
            index: _,
            doc: _,
            fields,
        } = self;
        let fields = fields.to_doc();
        braces(fields)
    }
}

impl ToDoc for Vec<Parameter> {
    fn to_doc(&self) -> Doc {
        use RcDoc as RD;
        let params = self.iter().enumerate().map(|(ndx, p)| {
            let name = p.name.unwrap_or_else(|| Symbol::from(format!("arg{ndx}")));
            cat!(
                RD::as_string(name).append(RD::text(":")).group(),
                p.type_.to_doc()
            )
            .group()
        });
        parens(comma(params.collect::<Vec<_>>()))
    }
}

impl ToDoc for Vec<DatatypeTArg> {
    fn to_doc(&self) -> Doc {
        if self.is_empty() {
            RcDoc::nil()
        } else {
            angles(comma(self.iter().map(|t| t.argument.to_doc()).collect()))
        }
    }
}

impl ToDoc for Vec<TParam> {
    fn to_doc(&self) -> Doc {
        use RcDoc as RD;
        if self.is_empty() {
            RcDoc::nil()
        } else {
            let tparams = self.iter().enumerate().map(|(ndx, tp)| {
                let TParam { name, constraints } = tp;
                let name = name.unwrap_or_else(|| Symbol::from(format!("T{ndx}")));
                let mut doc = RD::as_string(name);
                let AbilitySet(constraints) = constraints;
                if !constraints.is_empty() {
                    let constraints =
                        RD::intersperse(constraints.iter().map(|c| c.to_doc()), RD::text("+"))
                            .group();
                    doc = cat!(doc.append(RD::text(":")), constraints);
                }
                doc.group()
            });
            let tparams = comma(tparams.collect());
            angles(tparams)
        }
    }
}

impl ToDoc for Vec<DatatypeTParam> {
    fn to_doc(&self) -> Doc {
        use RcDoc as RD;
        if self.is_empty() {
            RcDoc::nil()
        } else {
            let tparams = self.iter().enumerate().map(|(ndx, tp)| {
                let DatatypeTParam { phantom, tparam } = tp;
                let TParam { name, constraints } = tparam;
                let name = name.unwrap_or_else(|| Symbol::from(format!("T{ndx}")));
                let mut doc = if *phantom {
                    RD::text("phantom")
                } else {
                    RD::nil()
                };
                doc = cat!(doc, RD::as_string(name));
                let AbilitySet(constraints) = constraints;
                if !constraints.is_empty() {
                    let constraints =
                        RD::intersperse(constraints.iter().map(|c| c.to_doc()), RD::text("+"))
                            .group();
                    doc = cat!(doc.append(RD::text(":")), constraints);
                }
                doc.group()
            });
            let tparams = comma(tparams.collect());
            angles(tparams)
        }
    }
}

impl ToDoc for Ability {
    fn to_doc(&self) -> Doc {
        match self {
            Ability::Copy => RcDoc::text("copy"),
            Ability::Drop => RcDoc::text("drop"),
            Ability::Store => RcDoc::text("store"),
            Ability::Key => RcDoc::text("key"),
        }
    }
}

impl ToDoc for AbilitySet {
    fn to_doc(&self) -> Doc {
        use RcDoc as RD;
        if self.0.is_empty() {
            RD::nil()
        } else {
            comma(self.0.iter().map(|a| a.to_doc()).collect())
        }
    }
}

impl ToDoc for Fields {
    fn to_doc(&self) -> Doc {
        use RcDoc as RD;

        // TODO: positional_fields (left as-is per your comment)
        let Fields { positional_fields: _, fields } = self;

        // name: Type
        let items: Vec<Doc> = fields.iter().map(|(name, field)| {
            RD::as_string(name)
                .append(RD::text(": "))
                .append(field.type_.to_doc())
        }).collect();

        if items.is_empty() {
            return RD::nil();
        }

        // --- Wide (single-line) ---
        let wide = comma(items.iter().cloned().collect());

        // --- Tall (multiline) with trailing comma ---
        let tall_body = RD::intersperse(items.into_iter(), RD::text(",").append(RD::hardline()));
        let tall = tall_body.append(RD::text(",")).append(RD::hardline()); // trailing comma

        // Pick one layout for the entire list, consistently.
        RD::group(tall.flat_alt(wide))
    }
}

impl ToDoc for ModuleId {
    fn to_doc(&self) -> Doc {
        use RcDoc as RD;
        let ModuleId { address, name } = self;
        RD::text(format!("{address}::{name}"))
    }
}

impl ToDoc for Datatype {
    fn to_doc(&self) -> Doc {
        use RcDoc as RD;
        let Datatype {
            module,
            name,
            type_arguments,
        } = self;
        let targs = type_arguments.to_doc();
        module
            .to_doc()
            .append(RD::text("::"))
            .append(RD::as_string(name))
            .append(targs)
            .group()
    }
}

impl ToDoc for Type {
    fn to_doc(&self) -> Doc {
        use RcDoc as RD;
        match self {
            Type::Bool => RD::text("bool"),
            Type::U8 => RD::text("u8"),
            Type::U16 => RD::text("u16"),
            Type::U32 => RD::text("u32"),
            Type::U64 => RD::text("u64"),
            Type::U128 => RD::text("u128"),
            Type::U256 => RD::text("u256"),
            Type::Address => RD::text("address"),
            Type::Signer => RD::text("signer"),
            Type::Any => RD::text("_"),
            Type::NamedTypeParameter(name) => RD::as_string(name),
            Type::Datatype(dt) => dt.to_doc(),
            Type::Vector(inner) => RD::text("vector").append(angles(inner.to_doc())).group(),
            Type::Reference(is_mut, inner) => {
                let mut doc = RD::text("&");
                if *is_mut {
                    doc = doc.append(RD::text("mut")).append(RD::space());
                }
                doc.group().append(inner.to_doc()).group()
            }
            Type::Fun(params, ret_) => {
                let params = RD::intersperse(
                    params.iter().map(|p| p.to_doc()),
                    RD::text(",").append(RD::space()),
                );
                let params = RD::text("|")
                    .append(params)
                    .append("|")
                    .group()
                    .append(RD::space())
                    .append("->")
                    .group();
                params
                    .append(RD::softline())
                    .append(ret_.to_doc().nest(4))
                    .group()
            }
            Type::Tuple(types) => {
                let types = comma(types.iter().map(|t| t.to_doc()).collect());
                RD::text("(").append(types).append(RD::text(")")).group()
            }
            Type::TypeParameter(ndx) => RD::text(format!("T{ndx}")),
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Generic Impls
// -------------------------------------------------------------------------------------------------

impl ToDoc for Box<dyn ToDoc> {
    fn to_doc(&self) -> Doc {
        self.as_ref().to_doc()
    }
}

// -------------------------------------------------------------------------------------------------
// Utilities
// -------------------------------------------------------------------------------------------------

fn parens(inner: Doc) -> Doc {
    use RcDoc as RD;
    RD::text("(")
        .append(RD::softline().append(inner).nest(4))
        .append(RD::softline())
        .append(RD::text(")"))
        .group()
}

fn angles(inner: Doc) -> Doc {
    use RcDoc as RD;
    RD::text("<")
        .append(RD::softline().append(inner).nest(4))
        .append(RD::softline())
        .append(RD::text(">"))
        .group()
}

fn braces_block(inner: Doc) -> Doc {
    use RcDoc as RD;
    RD::text("{")
        .append(RD::hardline().append(inner).nest(4))
        .append(RD::hardline())
        .append(RD::text("}"))
        .group()
}

fn braces(inner: Doc) -> Doc {
    use RcDoc as RD;
    RD::text("{")
        .append(RD::softline().append(inner).nest(4))
        .append(RD::softline())
        .append(RD::text("}"))
        .group()
}

fn comma_lines(docs: Vec<RcDoc>) -> RcDoc {
    use RcDoc as RD;
    RD::intersperse(docs, RD::text(",").append(RD::hardline())).group()
}

fn comma(docs: Vec<RcDoc>) -> RcDoc {
    use RcDoc as RD;
    RD::intersperse(docs, RD::text(",").append(RD::hardline())).group()
}

fn cat(docs: Vec<RcDoc>) -> RcDoc {
    use RcDoc as RD;
    RD::intersperse(docs, RD::space()).group()
}

fn align(doc: Doc) -> Doc {
    use RcDoc as RD;
    RD::column(move |k| {
        let doc_ = doc.clone();
        RD::nesting(move |i| doc_.clone().nest(k as isize - i as isize))
    })
}

fn hang(doc: Doc, n: isize) -> Doc {
    align(doc.nest(n))
}

fn indent(doc: Doc, n: isize) -> Doc {
    let spaces = RcDoc::text(" ".repeat(n as usize));
    hang(spaces.append(doc), n)
}

