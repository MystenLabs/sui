// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Renders Move source stubs from compiled bytecode modules.
//!
//! Stubs contain `native fun` declarations for all public functions and full definitions for all
//! structs and enums. They are used as source-level stand-ins for on-chain bytecode dependencies.

use std::fmt::{self, Write};

use move_binary_format::{
    file_format::{AbilitySet, DatatypeTyParameter, Visibility},
    normalized::{self, Type},
};
use move_core_types::account_address::AccountAddress;
use move_symbol_pool::Symbol;

/// Render a single compiled `module` as a Move source stub, writing to `w`.
pub fn render_module(
    w: &mut impl Write,
    address: &AccountAddress,
    module: &normalized::Module<Symbol>,
) -> fmt::Result {
    let addr = address.to_canonical_string(true);
    writeln!(w, "/// Auto-generated stub for on-chain package at {addr}")?;
    writeln!(w, "module {addr}::{} {{", module.name())?;

    for (_, s) in &module.structs {
        render_datatype_header(w, "struct", &s.name, &s.type_parameters, &s.abilities)?;
        render_fields(w, &s.fields, &tparam_names(&s.type_parameters), "    ")?;
        writeln!(w)?;
    }

    for (_, e) in &module.enums {
        render_datatype_header(w, "enum", &e.name, &e.type_parameters, &e.abilities)?;
        writeln!(w, " {{")?;
        let names = tparam_names(&e.type_parameters);
        for (_, variant) in &e.variants {
            render_variant(w, variant, &names)?;
        }
        writeln!(w, "    }}")?;
    }

    for (_, f) in &module.functions {
        if f.visibility == Visibility::Private {
            continue;
        }
        render_function(w, f)?;
    }

    writeln!(w, "}}")
}

/// Render the shared header for struct and enum definitions:
/// `    public struct/enum Name<params> has abilities`
fn render_datatype_header(
    w: &mut impl Write,
    keyword: &str,
    name: &Symbol,
    type_parameters: &[DatatypeTyParameter],
    abilities: &AbilitySet,
) -> fmt::Result {
    let tparams = render_tparams(type_parameters);
    let has = render_has(abilities);
    write!(w, "    public {keyword} {name}{tparams}{has}")
}

/// Render a single enum variant.
fn render_variant(
    w: &mut impl Write,
    variant: &normalized::Variant<Symbol>,
    tparam_names: &[String],
) -> fmt::Result {
    write!(w, "        {}", variant.name)?;
    if variant.fields.0.is_empty() {
        writeln!(w, ",")
    } else {
        render_fields(w, &variant.fields, tparam_names, "        ")?;
        writeln!(w, ",")
    }
}

/// Render a function as a `native fun` declaration.
fn render_function(w: &mut impl Write, f: &normalized::Function<Symbol>) -> fmt::Result {
    let vis = render_visibility(f.visibility);
    let entry = if f.is_entry { "entry " } else { "" };
    let names = fun_tparam_names(&f.type_parameters);
    let tparams = render_fun_tparams(&f.type_parameters, &names);
    let params = render_params(&f.parameters, &names);
    let ret = render_return(&f.return_, &names);

    writeln!(
        w,
        "    {vis}{entry}native fun {}{tparams}({params}){ret};",
        f.name
    )
}

/// Render visibility keyword.
fn render_visibility(vis: Visibility) -> &'static str {
    match vis {
        Visibility::Public => "public ",
        Visibility::Friend => "public(package) ",
        Visibility::Private => "",
    }
}

/// Render function type parameters (e.g. `<T0: copy + drop, T1>`).
fn render_fun_tparams(params: &[AbilitySet], names: &[String]) -> String {
    if params.is_empty() {
        return String::new();
    }
    let rendered: Vec<String> = params
        .iter()
        .enumerate()
        .map(|(i, abilities)| {
            let constraints = render_abilities(abilities, " + ");
            if constraints.is_empty() {
                names[i].clone()
            } else {
                format!("{}: {constraints}", names[i])
            }
        })
        .collect();
    format!("<{}>", rendered.join(", "))
}

/// Render function parameters (e.g. `p0: u64, p1: &Coin`).
fn render_params(params: &normalized::Signature<Symbol>, names: &[String]) -> String {
    params
        .iter()
        .enumerate()
        .map(|(i, ty)| format!("p{i}: {}", render_type(ty, names)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Render return type (e.g. `: u64` or `: (u64, bool)` or empty).
fn render_return(return_: &normalized::Signature<Symbol>, names: &[String]) -> String {
    if return_.is_empty() {
        return String::new();
    }
    let types: Vec<String> = return_.iter().map(|ty| render_type(ty, names)).collect();
    if types.len() == 1 {
        format!(": {}", types[0])
    } else {
        format!(": ({})", types.join(", "))
    }
}

/// Render struct/variant fields as ` { name: Type, ... }` or ` {}` if empty.
fn render_fields(
    w: &mut impl Write,
    fields: &normalized::Fields<Symbol>,
    tparam_names: &[String],
    indent: &str,
) -> fmt::Result {
    if fields.0.is_empty() {
        return write!(w, " {{}}");
    }

    writeln!(w, " {{")?;
    for (_, field) in &fields.0 {
        writeln!(
            w,
            "{indent}    {}: {},",
            field.name,
            render_type(&field.type_, tparam_names)
        )?;
    }
    write!(w, "{indent}}}")
}

/// Render a type expression as Move source.
fn render_type(ty: &Type<Symbol>, tparam_names: &[String]) -> String {
    match ty {
        Type::Bool => "bool".into(),
        Type::U8 => "u8".into(),
        Type::U16 => "u16".into(),
        Type::U32 => "u32".into(),
        Type::U64 => "u64".into(),
        Type::U128 => "u128".into(),
        Type::U256 => "u256".into(),
        Type::Address => "address".into(),
        Type::Signer => "signer".into(),
        Type::Vector(inner) => format!("vector<{}>", render_type(inner, tparam_names)),
        Type::TypeParameter(idx) => tparam_names
            .get(*idx as usize)
            .cloned()
            .unwrap_or_else(|| format!("T{idx}")),
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
fn render_tparams(params: &[DatatypeTyParameter]) -> String {
    if params.is_empty() {
        return String::new();
    }
    let rendered: Vec<String> = params
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let phantom = if p.is_phantom { "phantom " } else { "" };
            let constraints = render_abilities(&p.constraints, " + ");
            if constraints.is_empty() {
                format!("{phantom}T{i}")
            } else {
                format!("{phantom}T{i}: {constraints}")
            }
        })
        .collect();
    format!("<{}>", rendered.join(", "))
}

/// Render a ` has abilities` clause, or empty string if no abilities.
fn render_has(abilities: &AbilitySet) -> String {
    let s = render_abilities(abilities, ", ");
    if s.is_empty() {
        String::new()
    } else {
        format!(" has {s}")
    }
}

/// Render an ability set with the given `separator` (`, ` for has clauses, ` + ` for constraints).
fn render_abilities(abilities: &AbilitySet, separator: &str) -> String {
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
    parts.join(separator)
}

/// Generate type parameter names `["T0", "T1", ...]` for datatype type parameters.
fn tparam_names(params: &[DatatypeTyParameter]) -> Vec<String> {
    (0..params.len()).map(|i| format!("T{i}")).collect()
}

/// Generate type parameter names for function type parameters.
fn fun_tparam_names(params: &[AbilitySet]) -> Vec<String> {
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

    /// Render the pre-compiled `example.mv` test fixture and snapshot the output.
    ///
    /// The fixture is compiled from `tests/data/stub_test/sources/example.move`. To rebuild:
    /// ```sh
    /// cargo run -p move-cli -- build --path tests/data/stub_test
    /// cp tests/data/stub_test/build/stub_test/bytecode_modules/example.mv \
    ///    tests/data/stub_test/example.mv
    /// rm -rf tests/data/stub_test/build
    /// ```
    #[test]
    fn render_example_stub() {
        let bytes = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/data/stub_test/example.mv"
        ))
        .expect("test fixture exists");
        let compiled =
            CompiledModule::deserialize_with_defaults(&bytes).expect("valid compiled module");
        let normalized = normalized::Module::new(
            &mut TestSymbolPool,
            &compiled,
            /* include code */ false,
        );
        let address = *compiled.self_id().address();
        let mut output = String::new();
        render_module(&mut output, &address, &normalized).unwrap();
        assert_snapshot!(output);
    }
}
