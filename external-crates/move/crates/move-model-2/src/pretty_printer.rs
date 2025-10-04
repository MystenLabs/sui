// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Provides `to_doc` for various elemnts of the Move model summary to generate `RcDoc`s from
//! `pretty`. This allows for easy printing of the summary in a human-readable format, plus
//! rendering it out in other settings.

use crate::{source_kind::SourceKind, summary::*};

use move_symbol_pool::Symbol;
use pretty_simple::{Doc, ToDoc, to_list};

// -------------------------------------------------------------------------------------------------
// Function Header Generation
// -------------------------------------------------------------------------------------------------

/// Build the *header line* for a Move function from its summary.
/// Example outputs (no trailing semicolon, just the header):
/// - `fun f<T>(x: u64): u64`
/// - `public entry fun g(a: u8, b: u8)`
/// - `public(friend) macro fun h<T, U>(x: T, y: U): (T, U)`
pub fn fun_header<K: SourceKind>(
    model_fun: &crate::model::Function<K>,
    use_param_names: bool,
) -> Doc {
    use Doc as D;

    fn returns_doc(tys: &[Type]) -> Doc {
        match tys {
            [] => D::nil(),
            [t] => D::text(": ").concat(t.to_doc()),
            many => {
                let items = many.iter().map(|t| t.to_doc());
                D::text(": ").concat(
                    D::softline()
                        .concat(D::intersperse(items, D::text(",").concat(D::softline())))
                        .nest(4)
                        .parens()
                        .group(),
                )
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
        prefix_parts.push(D::text("entry"));
    }
    if *(macro_.as_ref().unwrap_or(&false)) {
        prefix_parts.push(D::text("macro"));
    }

    // Join prefixes with spaces (if any).
    let prefix = if prefix_parts.is_empty() {
        D::nil()
    } else {
        D::intersperse(prefix_parts, D::space()).concat(D::space())
    };

    // `fun name`
    let name_doc = D::text("fun").concat_space(D::text(model_fun.name().as_str()));

    // `<T...>` (optional)
    let tparams_doc = if !type_parameters.is_empty() {
        D::angles(to_list(type_parameters, Doc::text(",")).group())
    } else {
        D::nil()
    };

    // `(params...)`
    let params_doc = if !use_param_names {
        let parameters = parameters.iter().enumerate().map(|(i, p)| {
            D::text(format!("l{i}"))
                .concat(D::text(":"))
                .group()
                .concat_space(p.type_.to_doc())
                .group()
        });
        D::intersperse(parameters, Doc::comma().concat(Doc::space()))
            .group()
            .parens()
    } else {
        to_list(parameters, Doc::text(",")).group().parens()
    };

    // `: ret`
    let ret_doc = returns_doc(return_);

    prefix
        .concat(name_doc)
        .concat(tparams_doc)
        .concat(params_doc)
        .concat(ret_doc)
        .group()
}

// -------------------------------------------------------------------------------------------------
// model impls
// -------------------------------------------------------------------------------------------------

impl<K: SourceKind> ToDoc for crate::model::Struct<'_, K> {
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

        let type_parameters = if !type_parameters.is_empty() {
            Doc::angles(to_list(type_parameters, Doc::text(",")).group())
        } else {
            Doc::nil()
        };
        let abilities = if !abilities.0.is_empty() {
            Doc::softline()
                .concat(Doc::hsep(vec![Doc::text("has"), abilities.to_doc()]))
                .group()
        } else {
            Doc::nil()
        };

        Doc::text("public struct")
            .concat_space(Doc::text(name.as_str()))
            .concat(type_parameters)
            .group()
            .concat(abilities)
            .concat_space(fields.to_doc().braces())
            .group()
    }
}

impl<K: SourceKind> ToDoc for crate::model::Enum<'_, K> {
    fn to_doc(&self) -> Doc {
        use Doc as D;
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

        let type_parameters = if !type_parameters.is_empty() {
            D::angles(to_list(type_parameters, D::text(",")).group())
        } else {
            D::nil()
        };
        let abilities = if !abilities.0.is_empty() {
            Doc::softline()
                .concat(Doc::hsep(vec![Doc::text("has"), abilities.to_doc()]))
                .group()
        } else {
            Doc::nil()
        };
        let variants = D::intersperse(
            variants.iter().map(|(name, variant)| {
                D::text(name.as_str())
                    .concat_space(variant.to_doc())
                    .concat(D::text(","))
            }),
            D::line(),
        );

        Doc::text("public enum")
            .concat_space(Doc::text(name.as_ref()))
            .concat(type_parameters)
            .group()
            .concat(abilities)
            .concat_space(
                D::line()
                    .concat(variants.indent(4))
                    .concat(D::line())
                    .braces(),
            )
    }
}

// -------------------------------------------------------------------------------------------------
// Summary Impls
// -------------------------------------------------------------------------------------------------

impl ToDoc for Visibility {
    fn to_doc(&self) -> Doc {
        use Doc as D;
        match self {
            Visibility::Private => D::nil(),
            Visibility::Public => D::text("public"),
            Visibility::Friend => D::text("public(friend)"),
            Visibility::Package => D::text("public(package)"),
        }
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
        fields.braces()
    }
}

impl ToDoc for Parameter {
    fn to_doc(&self) -> Doc {
        use Doc as D;
        let name = self.name.unwrap();
        D::text(name.as_str())
            .concat(D::text(":"))
            .group()
            .concat_space(self.type_.to_doc())
            .group()
    }
}

impl ToDoc for DatatypeTArg {
    fn to_doc(&self) -> Doc {
        self.argument.to_doc()
    }
}

impl ToDoc for TParam {
    fn to_doc(&self) -> Doc {
        use Doc as D;
        let TParam { name, constraints } = self;
        let name = name.unwrap_or_else(|| Symbol::from("T"));
        let mut doc = D::text(name.as_str());
        let AbilitySet(constraints) = constraints;
        if !constraints.is_empty() {
            let constraints =
                D::intersperse(constraints.iter().map(|c| c.to_doc()), D::text("+")).group();
            doc = doc.concat(D::text(":")).concat_space(constraints);
        }
        doc.group()
    }
}

impl ToDoc for DatatypeTParam {
    fn to_doc(&self) -> Doc {
        use Doc as D;
        let DatatypeTParam { phantom, tparam } = self;
        let TParam { name, constraints } = tparam;
        let name = name.unwrap_or_else(|| Symbol::from("T"));
        let mut doc = if *phantom {
            D::text("phantom").concat(D::space())
        } else {
            D::nil()
        };
        doc = doc.concat(D::text(name.as_str()));
        let AbilitySet(constraints) = constraints;
        if !constraints.is_empty() {
            let constraints =
                D::intersperse(constraints.iter().map(|c| c.to_doc()), D::text("+")).group();
            doc = doc.concat(D::text(":")).concat_space(constraints);
        }
        doc.group()
    }
}

impl ToDoc for Ability {
    fn to_doc(&self) -> Doc {
        match self {
            Ability::Copy => Doc::text("copy"),
            Ability::Drop => Doc::text("drop"),
            Ability::Store => Doc::text("store"),
            Ability::Key => Doc::text("key"),
        }
    }
}

impl ToDoc for AbilitySet {
    fn to_doc(&self) -> Doc {
        use Doc as D;
        if self.0.is_empty() {
            D::nil()
        } else {
            to_list(self.0.iter(), Doc::text(",").concat(Doc::space())).group()
        }
    }
}

impl ToDoc for Fields {
    fn to_doc(&self) -> Doc {
        use Doc as D;

        // TODO: positional_fields (left as-is per your comment)
        let Fields {
            positional_fields: _,
            fields,
        } = self;

        // name: Type
        let items: Vec<Doc> = fields
            .iter()
            .map(|(name, field)| {
                D::text(name.as_str())
                    .concat(D::text(":"))
                    .concat_space(field.type_.to_doc())
            })
            .collect();

        if items.is_empty() {
            return D::nil();
        }

        // --- Wide (single-line) ---
        let wide = D::space().concat(
            D::intersperse(items.iter().cloned(), D::text(",").concat(D::space()))
                .concat(D::space()),
        );

        // --- Tall (multiline) with trailing comma ---
        let tall = D::line()
            .concat(
                D::intersperse(items, D::text(",").concat(D::line()))
                    .concat(D::text(","))
                    .indent(4),
            )
            .concat(D::line())
            .group();

        // Pick one layout for the entire list, consistently.
        D::alt(wide, tall).group()
    }
}

impl ToDoc for ModuleId {
    fn to_doc(&self) -> Doc {
        let ModuleId { address, name } = self;
        Doc::text(format!("{address}::{name}"))
    }
}

impl ToDoc for Datatype {
    fn to_doc(&self) -> Doc {
        use Doc as D;
        let Datatype {
            module,
            name,
            type_arguments,
        } = self;
        let targs = to_list(type_arguments, D::text(",").concat(D::space()));
        module
            .to_doc()
            .concat(D::text("::"))
            .concat(D::text(name.as_str()))
            .concat(targs)
            .group()
    }
}

impl ToDoc for Type {
    fn to_doc(&self) -> Doc {
        use Doc as D;
        match self {
            Type::Bool => D::text("bool"),
            Type::U8 => D::text("u8"),
            Type::U16 => D::text("u16"),
            Type::U32 => D::text("u32"),
            Type::U64 => D::text("u64"),
            Type::U128 => D::text("u128"),
            Type::U256 => D::text("u256"),
            Type::Address => D::text("address"),
            Type::Signer => D::text("signer"),
            Type::Any => D::text("_"),
            Type::NamedTypeParameter(name) => D::text(name.as_str()),
            Type::Datatype(dt) => dt.to_doc(),
            Type::Vector(inner) => D::text("vector").concat(D::angles(inner.to_doc())).group(),
            Type::Reference(is_mut, inner) => {
                let mut doc = D::text("&");
                if *is_mut {
                    doc = doc.concat(D::text("mut")).concat(D::space());
                }
                doc.group().concat(inner.to_doc()).group()
            }
            Type::Fun(params, ret_) => {
                let params = D::intersperse(
                    params.iter().map(|p| p.to_doc()),
                    D::text(",").concat(D::space()),
                );
                let params = D::text("|")
                    .concat(params)
                    .concat(D::text("|"))
                    .group()
                    .concat_space(D::text("->"))
                    .group();
                params
                    .concat(D::softline())
                    .concat(ret_.to_doc().nest(4))
                    .group()
            }
            Type::Tuple(types) => {
                let types = to_list(types, Doc::text(","));
                D::parens(types).group()
            }
            Type::TypeParameter(ndx) => D::text(format!("T{ndx}")),
        }
    }
}
