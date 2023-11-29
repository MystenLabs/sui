// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

use codespan::ByteIndex;
use codespan_reporting::diagnostic::{Diagnostic, Label, LabelStyle};
use itertools::Itertools;
#[allow(unused_imports)]
use log::warn;
use num::{BigUint, Num};

use builder::module_builder::ModuleBuilder;
use move_binary_format::{
    access::ModuleAccess,
    file_format::{CompiledModule, FunctionDefinitionIndex, StructDefinitionIndex},
};
use move_compiler::{
    self,
    compiled_unit::{self, AnnotatedCompiledUnit},
    diagnostics::{Diagnostics, WarningFilters},
    expansion::ast::{self as E, ModuleIdent, ModuleIdent_},
    parser::ast::{self as P},
    shared::{parse_named_address, unique_map::UniqueMap, NumericalAddress, PackagePaths},
    typing::ast::{self as T},
    Compiler, Flags, PASS_COMPILATION, PASS_EXPANSION, PASS_PARSER, PASS_TYPING,
};
use move_core_types::account_address::AccountAddress;
use move_symbol_pool::Symbol as MoveSymbol;

use crate::{
    ast::{ModuleName, Spec},
    builder::model_builder::ModelBuilder,
    model::{FunId, FunctionData, GlobalEnv, Loc, ModuleData, ModuleId, StructId},
    options::ModelBuilderOptions,
    simplifier::{SpecRewriter, SpecRewriterPipeline},
};

pub mod ast;
mod builder;
pub mod code_writer;
pub mod exp_generator;
pub mod exp_rewriter;
pub mod intrinsics;
pub mod model;
pub mod options;
pub mod pragmas;
pub mod simplifier;
pub mod spec_translator;
pub mod symbol;
pub mod ty;
pub mod well_known;

// =================================================================================================
// Entry Point

/// Build the move model with default compilation flags and default options and no named addresses.
/// This collects transitive dependencies for move sources from the provided directory list.
pub fn run_model_builder<
    Paths: Into<MoveSymbol> + Clone,
    NamedAddress: Into<MoveSymbol> + Clone,
>(
    move_sources: Vec<PackagePaths<Paths, NamedAddress>>,
    deps: Vec<PackagePaths<Paths, NamedAddress>>,
    warning_filter: Option<WarningFilters>,
) -> anyhow::Result<GlobalEnv> {
    run_model_builder_with_options(
        move_sources,
        deps,
        ModelBuilderOptions::default(),
        warning_filter,
    )
}

/// Build the move model with default compilation flags and custom options and a set of provided
/// named addreses.
/// This collects transitive dependencies for move sources from the provided directory list.
pub fn run_model_builder_with_options<
    Paths: Into<MoveSymbol> + Clone,
    NamedAddress: Into<MoveSymbol> + Clone,
>(
    move_sources: Vec<PackagePaths<Paths, NamedAddress>>,
    deps: Vec<PackagePaths<Paths, NamedAddress>>,
    options: ModelBuilderOptions,
    warning_filter: Option<WarningFilters>,
) -> anyhow::Result<GlobalEnv> {
    run_model_builder_with_options_and_compilation_flags(
        move_sources,
        deps,
        options,
        Flags::verification(),
        warning_filter,
    )
}

/// Build the move model with custom compilation flags and custom options
/// This collects transitive dependencies for move sources from the provided directory list.
pub fn run_model_builder_with_options_and_compilation_flags<
    Paths: Into<MoveSymbol> + Clone,
    NamedAddress: Into<MoveSymbol> + Clone,
>(
    move_sources: Vec<PackagePaths<Paths, NamedAddress>>,
    deps: Vec<PackagePaths<Paths, NamedAddress>>,
    options: ModelBuilderOptions,
    flags: Flags,
    warning_filter: Option<WarningFilters>,
) -> anyhow::Result<GlobalEnv> {
    let mut env = GlobalEnv::new();
    env.set_extension(options);

    // Step 1: parse the program to get comments and a separation of targets and dependencies.
    let (files, comments_and_compiler_res) = Compiler::from_package_paths(move_sources, deps)?
        .set_flags(flags)
        .set_warning_filter(warning_filter)
        .run::<PASS_PARSER>()?;
    let (comment_map, compiler) = match comments_and_compiler_res {
        Err(diags) => {
            // Add source files so that the env knows how to translate locations of parse errors
            let empty_alias = Rc::new(BTreeMap::new());
            for (fhash, (fname, fsrc)) in &files {
                env.add_source(
                    *fhash,
                    empty_alias.clone(),
                    fname.as_str(),
                    fsrc,
                    /* is_dep */ false,
                );
            }
            add_move_lang_diagnostics(&mut env, diags);
            return Ok(env);
        }
        Ok(res) => res,
    };
    let (compiler, parsed_prog) = compiler.into_ast();

    // Add source files for targets and dependencies
    let dep_files: BTreeSet<_> = parsed_prog
        .lib_definitions
        .iter()
        .map(|p| p.def.file_hash())
        .collect();

    for member in parsed_prog
        .source_definitions
        .iter()
        .chain(parsed_prog.lib_definitions.iter())
    {
        let fhash = member.def.file_hash();
        let (fname, fsrc) = files.get(&fhash).unwrap();
        let is_dep = dep_files.contains(&fhash);
        let aliases = parsed_prog
            .named_address_maps
            .get(member.named_address_map)
            .iter()
            .map(|(symbol, addr)| (env.symbol_pool().make(symbol.as_str()), *addr))
            .collect();
        env.add_source(fhash, Rc::new(aliases), fname.as_str(), fsrc, is_dep);
    }

    // If a move file does not contain any definition, it will not appear in `parsed_prog`. Add them explicitly.
    for fhash in files.keys().sorted() {
        if env.get_file_id(*fhash).is_none() {
            let (fname, fsrc) = files.get(fhash).unwrap();
            let is_dep = dep_files.contains(fhash);
            env.add_source(
                *fhash,
                Rc::new(BTreeMap::new()),
                fname.as_str(),
                fsrc,
                is_dep,
            );
        }
    }

    // Add any documentation comments found by the Move compiler to the env.
    for (fhash, documentation) in comment_map {
        let file_id = env.get_file_id(fhash).expect("file name defined");
        env.add_documentation(
            file_id,
            documentation
                .into_iter()
                .map(|(idx, s)| (ByteIndex(idx), s))
                .collect(),
        )
    }

    // Step 2: run the compiler up to expansion
    let parsed_prog = {
        let P::Program {
            named_address_maps,
            mut source_definitions,
            lib_definitions,
        } = parsed_prog;
        source_definitions.extend(lib_definitions);
        P::Program {
            named_address_maps,
            source_definitions,
            lib_definitions: vec![],
        }
    };
    let (compiler, expansion_ast) = match compiler.at_parser(parsed_prog).run::<PASS_EXPANSION>() {
        Err(diags) => {
            add_move_lang_diagnostics(&mut env, diags);
            return Ok(env);
        }
        Ok(compiler) => compiler.into_ast(),
    };
    let (compiler, typing_ast) = match compiler
        .at_expansion(expansion_ast.clone())
        .run::<PASS_TYPING>()
    {
        Err(diags) => {
            add_move_lang_diagnostics(&mut env, diags);
            return Ok(env);
        }
        Ok(compiler) => compiler.into_ast(),
    };

    // Extract the module/script closure
    let mut visited_modules = BTreeSet::new();
    for (_, mident, mdef) in &typing_ast.inner.modules {
        let src_file_hash = mdef.loc.file_hash();
        if !dep_files.contains(&src_file_hash) {
            collect_related_modules_recursive(
                mident,
                &typing_ast.inner.modules,
                &mut visited_modules,
            );
        }
    }

    // Step 3: selective compilation.
    let expansion_ast = {
        let E::Program { modules } = expansion_ast;
        let modules = modules.filter_map(|mident, mut mdef| {
            visited_modules.contains(&mident.value).then(|| {
                mdef.is_source_module = true;
                mdef
            })
        });
        E::Program { modules }
    };
    let typing_ast = {
        let T::Program { info, inner } = typing_ast;
        let T::Program_ { modules } = inner;
        let modules = modules.filter_map(|mident, mut mdef| {
            visited_modules.contains(&mident.value).then(|| {
                mdef.is_source_module = true;
                mdef
            })
        });
        let inner = T::Program_ { modules };
        T::Program { info, inner }
    };

    // Run the compiler fully to the compiled units
    let units = match compiler.at_typing(typing_ast).run::<PASS_COMPILATION>() {
        Err(diags) => {
            add_move_lang_diagnostics(&mut env, diags);
            return Ok(env);
        }
        Ok(compiler) => {
            let (units, warnings) = compiler.into_compiled_units();
            if !warnings.is_empty() {
                // NOTE: these diagnostics are just warnings. it should be feasible to continue the
                // model building here. But before that, register the warnings to the `GlobalEnv`
                // first so we get a chance to report these warnings as well.
                add_move_lang_diagnostics(&mut env, warnings);
            }
            units
        }
    };

    // Check for bytecode verifier errors (there should not be any)
    let diags = compiled_unit::verify_units(&units);
    if !diags.is_empty() {
        add_move_lang_diagnostics(&mut env, diags);
        return Ok(env);
    }

    // Now that it is known that the program has no errors, run the spec checker on verified units
    // plus expanded AST. This will populate the environment including any errors.
    run_spec_checker(&mut env, units, expansion_ast);
    Ok(env)
}

fn collect_related_modules_recursive<'a>(
    mident: &'a ModuleIdent_,
    modules: &'a UniqueMap<ModuleIdent, T::ModuleDefinition>,
    visited_modules: &mut BTreeSet<ModuleIdent_>,
) {
    if visited_modules.contains(mident) {
        return;
    }
    let mdef = modules.get_(mident).unwrap();
    visited_modules.insert(*mident);
    for (_, next_mident, _) in &mdef.immediate_neighbors {
        collect_related_modules_recursive(next_mident, modules, visited_modules);
    }
}

/// Build a `GlobalEnv` from a collection of `CompiledModule`'s. The `modules` list must be
/// topologically sorted by the dependency relation (i.e., a child node in the dependency graph
/// should appear earlier in the vector than its parents).
pub fn run_bytecode_model_builder<'a>(
    modules: impl IntoIterator<Item = &'a CompiledModule>,
) -> anyhow::Result<GlobalEnv> {
    let mut env = GlobalEnv::new();
    for (i, m) in modules.into_iter().enumerate() {
        let id = m.self_id();
        let addr = addr_to_big_uint(id.address());
        let module_name = ModuleName::new(addr, env.symbol_pool().make(id.name().as_str()));
        let module_id = ModuleId::new(i);
        let mut module_data = ModuleData::stub(module_name.clone(), module_id, m.clone());

        // add functions
        for (i, def) in m.function_defs().iter().enumerate() {
            let def_idx = FunctionDefinitionIndex(i as u16);
            let name = m.identifier_at(m.function_handle_at(def.function).name);
            let symbol = env.symbol_pool().make(name.as_str());
            let fun_id = FunId::new(symbol);
            let data = FunctionData::stub(symbol, def_idx, def.function);
            module_data.function_data.insert(fun_id, data);
            module_data.function_idx_to_id.insert(def_idx, fun_id);
        }

        // add structs
        for (i, def) in m.struct_defs().iter().enumerate() {
            let def_idx = StructDefinitionIndex(i as u16);
            let name = m.identifier_at(m.datatype_handle_at(def.struct_handle).name);
            let symbol = env.symbol_pool().make(name.as_str());
            let struct_id = StructId::new(symbol);
            let data = env.create_move_struct_data(
                m,
                def_idx,
                symbol,
                Loc::default(),
                Vec::default(),
                Spec::default(),
            );
            module_data.struct_data.insert(struct_id, data);
            module_data.struct_idx_to_id.insert(def_idx, struct_id);
        }

        env.module_data.push(module_data);
    }
    Ok(env)
}

fn add_move_lang_diagnostics(env: &mut GlobalEnv, diags: Diagnostics) {
    let mk_label = |is_primary: bool, (loc, msg): (move_ir_types::location::Loc, String)| {
        let style = if is_primary {
            LabelStyle::Primary
        } else {
            LabelStyle::Secondary
        };
        let loc = env.to_loc(&loc);
        Label::new(style, loc.file_id(), loc.span()).with_message(msg)
    };
    for (severity, msg, primary_label, secondary_labels, notes) in diags.into_codespan_format() {
        let diag = Diagnostic::new(severity)
            .with_labels(vec![mk_label(true, primary_label)])
            .with_message(msg.to_string())
            .with_labels(
                secondary_labels
                    .into_iter()
                    .map(|e| mk_label(false, e))
                    .collect(),
            )
            .with_notes(notes);
        env.add_diag(diag);
    }
}

#[allow(deprecated)]
fn run_spec_checker(env: &mut GlobalEnv, units: Vec<AnnotatedCompiledUnit>, mut eprog: E::Program) {
    let mut builder = ModelBuilder::new(env);
    // Merge the compiled units with the expanded program, preserving the order of the compiled
    // units which is topological w.r.t. use relation.
    let modules = units
        .into_iter()
        .flat_map(|unit| {
            let module_ident = unit.module_ident();
            let expanded_module = match eprog.modules.remove(&module_ident) {
                Some(m) => m,
                None => {
                    warn!(
                        "[internal] cannot associate bytecode module `{}` with AST",
                        module_ident
                    );
                    return None;
                }
            };
            Some((
                module_ident,
                expanded_module,
                unit.named_module.module,
                unit.named_module.source_map,
                unit.function_infos,
            ))
        })
        .enumerate();
    for (module_count, (module_id, expanded_module, compiled_module, source_map, function_infos)) in
        modules
    {
        let loc = builder.to_loc(&expanded_module.loc);
        let addr_bytes = builder.resolve_address(&loc, &module_id.value.address);
        let module_name = ModuleName::from_address_bytes_and_name(
            addr_bytes,
            builder
                .env
                .symbol_pool()
                .make(&module_id.value.module.0.value),
        );
        let module_id = ModuleId::new(module_count);
        let mut module_translator = ModuleBuilder::new(&mut builder, module_id, module_name);
        module_translator.translate(
            loc,
            expanded_module,
            compiled_module,
            source_map,
            function_infos,
        );
    }

    // Populate GlobalEnv with model-level information
    builder.populate_env();

    // After all specs have been processed, warn about any unused schemas.
    builder.warn_unused_schemas();

    // Apply simplification passes
    run_spec_simplifier(env);
}

fn run_spec_simplifier(env: &mut GlobalEnv) {
    let options = env
        .get_extension::<ModelBuilderOptions>()
        .expect("options for model builder");
    let mut rewriter = SpecRewriterPipeline::new(&options.simplification_pipeline);
    rewriter
        .override_with_rewrite(env)
        .unwrap_or_else(|e| panic!("Failed to run spec simplification: {}", e))
}

// =================================================================================================
// Helpers

/// Converts an address identifier to a number representing the address.
pub fn addr_to_big_uint(addr: &AccountAddress) -> BigUint {
    BigUint::from_str_radix(&addr.to_string(), 16).unwrap()
}

/// Converts a biguint into an account address
pub fn big_uint_to_addr(i: &BigUint) -> AccountAddress {
    // TODO: do this in more efficient way (e.g., i.to_le_bytes() and pad out the resulting Vec<u8>
    // to ADDRESS_LENGTH
    AccountAddress::from_hex_literal(&format!("{:#x}", i)).unwrap()
}

pub fn parse_addresses_from_options(
    named_addr_strings: Vec<String>,
) -> anyhow::Result<BTreeMap<String, NumericalAddress>> {
    named_addr_strings
        .iter()
        .map(|x| parse_named_address(x))
        .collect()
}

// =================================================================================================
// Crate Helpers

/// Helper to project the 1st element from a vector of pairs.
pub(crate) fn project_1st<T: Clone, R>(v: &[(T, R)]) -> Vec<T> {
    v.iter().map(|(x, _)| x.clone()).collect()
}

/// Helper to project the 2nd element from a vector of pairs.
pub(crate) fn project_2nd<T, R: Clone>(v: &[(T, R)]) -> Vec<R> {
    v.iter().map(|(_, x)| x.clone()).collect()
}
