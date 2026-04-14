// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Generates Move source stubs from compiled bytecode modules.
//!
//! Stubs contain `native fun` declarations for all public functions and full definitions for all
//! structs and enums. They are used as source-level stand-ins for on-chain bytecode dependencies.

use std::fmt::Write;

use move_binary_format::{
    file_format::{AbilitySet, DatatypeTyParameter, Visibility},
    normalized::{self, Type},
};
use move_core_types::account_address::AccountAddress;
use move_symbol_pool::Symbol;

/// Render a single compiled `module` as a Move source stub.
/// The module declaration uses the given `address` for its address.
pub fn render_module(address: &AccountAddress, module: &normalized::Module<Symbol>) -> String {
    let mut out = String::new();

    writeln!(
        &mut out,
        "/// Auto-generated stub for on-chain package at {}",
        address.to_canonical_string(true)
    )
    .unwrap();
    writeln!(&mut out, "module {}::{} {{", address.to_canonical_string(true), module.name()).unwrap();

    // Structs
    for (_, s) in &module.structs {
        render_struct(&mut out, s);
    }

    // Enums
    for (_, e) in &module.enums {
        render_enum(&mut out, e);
    }

    // Functions (only public and friend)
    for (_, f) in &module.functions {
        if f.visibility == Visibility::Private {
            continue;
        }
        render_function(&mut out, f);
    }

    writeln!(&mut out, "}}").unwrap();
    out
}

/// Render a struct definition with abilities, type parameters, and fields.
fn render_struct(out: &mut String, s: &normalized::Struct<Symbol>) {
    let tparams = render_datatype_type_params(&s.type_parameters);
    let abilities = render_has_abilities(&s.abilities);
    let has_clause = if abilities.is_empty() {
        String::new()
    } else {
        format!(" has {abilities}")
    };

    write!(out, "    public struct {}{tparams}{has_clause}", s.name).unwrap();

    let tparam_names = datatype_tparam_names(&s.type_parameters);
    render_fields(out, &s.fields, &tparam_names, "    ");
    writeln!(out).unwrap();
}

/// Render an enum definition with abilities, type parameters, and variants.
fn render_enum(out: &mut String, e: &normalized::Enum<Symbol>) {
    let tparams = render_datatype_type_params(&e.type_parameters);
    let abilities = render_has_abilities(&e.abilities);
    let has_clause = if abilities.is_empty() {
        String::new()
    } else {
        format!(" has {abilities}")
    };

    writeln!(out, "    public enum {}{tparams}{has_clause} {{", e.name).unwrap();

    let tparam_names = datatype_tparam_names(&e.type_parameters);
    for (_, variant) in &e.variants {
        render_variant(out, variant, &tparam_names);
    }

    writeln!(out, "    }}").unwrap();
}

/// Render a single enum variant.
fn render_variant(
    out: &mut String,
    variant: &normalized::Variant<Symbol>,
    tparam_names: &[String],
) {
    write!(out, "        {}", variant.name).unwrap();
    if variant.fields.0.is_empty() {
        writeln!(out, ",").unwrap();
    } else {
        render_fields(out, &variant.fields, tparam_names, "        ");
        writeln!(out, ",").unwrap();
    }
}

/// Render a function as a `native fun` declaration.
fn render_function(out: &mut String, f: &normalized::Function<Symbol>) {
    let vis = match f.visibility {
        Visibility::Public => "public ",
        Visibility::Friend => "public(package) ",
        Visibility::Private => "",
    };
    let entry = if f.is_entry { "entry " } else { "" };

    let tparam_names: Vec<String> = (0..f.type_parameters.len())
        .map(|i| format!("T{i}"))
        .collect();

    let tparams = if f.type_parameters.is_empty() {
        String::new()
    } else {
        let rendered: Vec<String> = f
            .type_parameters
            .iter()
            .enumerate()
            .map(|(i, abilities)| {
                let constraints = render_constraint_abilities(abilities);
                if constraints.is_empty() {
                    tparam_names[i].clone()
                } else {
                    format!("{}: {constraints}", tparam_names[i])
                }
            })
            .collect();
        format!("<{}>", rendered.join(", "))
    };

    let params: Vec<String> = f
        .parameters
        .iter()
        .enumerate()
        .map(|(i, ty)| format!("p{i}: {}", render_type(ty, &tparam_names)))
        .collect();

    let ret = if f.return_.is_empty() {
        String::new()
    } else {
        let types: Vec<String> = f
            .return_
            .iter()
            .map(|ty| render_type(ty, &tparam_names))
            .collect();
        if types.len() == 1 {
            format!(": {}", types[0])
        } else {
            format!(": ({})", types.join(", "))
        }
    };

    writeln!(
        out,
        "    {vis}{entry}native fun {}{tparams}({}){ret};",
        f.name,
        params.join(", ")
    )
    .unwrap();
}

/// Render struct/variant fields as `{ name: Type, ... }` or ` {}` if empty.
/// `indent` is the base indentation for the enclosing definition.
fn render_fields(
    out: &mut String,
    fields: &normalized::Fields<Symbol>,
    tparam_names: &[String],
    indent: &str,
) {
    if fields.0.is_empty() {
        write!(out, " {{}}").unwrap();
        return;
    }

    writeln!(out, " {{").unwrap();
    for (_, field) in &fields.0 {
        writeln!(
            out,
            "{indent}    {}: {},",
            field.name,
            render_type(&field.type_, tparam_names)
        )
        .unwrap();
    }
    write!(out, "{indent}}}").unwrap();
}

/// Render a type expression as Move source.
fn render_type(ty: &Type<Symbol>, tparam_names: &[String]) -> String {
    match ty {
        Type::Bool => "bool".to_string(),
        Type::U8 => "u8".to_string(),
        Type::U16 => "u16".to_string(),
        Type::U32 => "u32".to_string(),
        Type::U64 => "u64".to_string(),
        Type::U128 => "u128".to_string(),
        Type::U256 => "u256".to_string(),
        Type::Address => "address".to_string(),
        Type::Signer => "signer".to_string(),
        Type::Vector(inner) => format!("vector<{}>", render_type(inner, tparam_names)),
        Type::TypeParameter(idx) => {
            tparam_names
                .get(*idx as usize)
                .cloned()
                .unwrap_or_else(|| format!("T{idx}"))
        }
        Type::Reference(is_mut, inner) => {
            let prefix = if *is_mut { "&mut " } else { "&" };
            format!("{prefix}{}", render_type(inner, tparam_names))
        }
        Type::Datatype(dt) => {
            let module = format!(
                "{}::{}",
                dt.module.address.to_canonical_string(true),
                dt.module.name
            );
            if dt.type_arguments.is_empty() {
                format!("{module}::{}", dt.name)
            } else {
                let args: Vec<String> = dt
                    .type_arguments
                    .iter()
                    .map(|t| render_type(t, tparam_names))
                    .collect();
                format!("{module}::{}<{}>", dt.name, args.join(", "))
            }
        }
    }
}

/// Render datatype type parameters (e.g. `<phantom T0: store, T1: copy + drop>`).
fn render_datatype_type_params(params: &[DatatypeTyParameter]) -> String {
    if params.is_empty() {
        return String::new();
    }
    let rendered: Vec<String> = params
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let phantom = if p.is_phantom { "phantom " } else { "" };
            let constraints = render_constraint_abilities(&p.constraints);
            if constraints.is_empty() {
                format!("{phantom}T{i}")
            } else {
                format!("{phantom}T{i}: {constraints}")
            }
        })
        .collect();
    format!("<{}>", rendered.join(", "))
}

/// Render an ability set as a list of ability names.
fn ability_names(abilities: &AbilitySet) -> Vec<&'static str> {
    let mut parts = Vec::new();
    if abilities.has_copy() {
        parts.push("copy");
    }
    if abilities.has_drop() {
        parts.push("drop");
    }
    if abilities.has_key() {
        parts.push("key");
    }
    if abilities.has_store() {
        parts.push("store");
    }
    parts
}

/// Render an ability set for a `has` clause (comma-separated, e.g. `copy, drop, store`).
fn render_has_abilities(abilities: &AbilitySet) -> String {
    ability_names(abilities).join(", ")
}

/// Render an ability set for type parameter constraints (`+`-separated, e.g. `copy + drop`).
fn render_constraint_abilities(abilities: &AbilitySet) -> String {
    ability_names(abilities).join(" + ")
}

/// Generate type parameter names `["T0", "T1", ...]` for datatype type parameters.
fn datatype_tparam_names(params: &[DatatypeTyParameter]) -> Vec<String> {
    (0..params.len()).map(|i| format!("T{i}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_snapshot;
    use move_binary_format::CompiledModule;
    use move_core_types::identifier::IdentStr;

    /// Minimal [normalized::StringPool] impl for tests that interns into [Symbol]s.
    struct TestSymbolPool;

    impl normalized::StringPool for TestSymbolPool {
        type String = Symbol;

        fn intern(&mut self, s: &IdentStr) -> Self::String {
            Symbol::from(s.as_str())
        }

        fn as_ident_str<'a>(&'a self, s: &'a Self::String) -> &'a IdentStr {
            IdentStr::new(s.as_str()).expect("symbol is a valid identifier")
        }
    }

    /// Render the pre-compiled `example.mv` test fixture and snapshot the output. The fixture is
    /// built from `tests/data/stub_test/sources/example.move`; if you change the source, rebuild
    /// the fixture with `cargo run -p move-cli -- build --path tests/data/stub_test` and copy
    /// `build/stub_test/bytecode_modules/example.mv` back to `tests/data/stub_test/example.mv`.
    #[test]
    fn render_example_stub() {
        let bytes = include_bytes!("../tests/data/stub_test/example.mv");
        let compiled = CompiledModule::deserialize_with_defaults(bytes)
            .expect("valid compiled module");
        let normalized = normalized::Module::new(
            &mut TestSymbolPool,
            &compiled,
            /* include code */ false,
        );
        let address = *compiled.self_id().address();
        let rendered = render_module(&address, &normalized);
        assert_snapshot!(rendered);
    }
}
