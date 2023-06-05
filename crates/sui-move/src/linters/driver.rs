// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use codespan_reporting::{diagnostic::Severity, term::termcolor::Buffer};
use std::collections::BTreeSet;

use move_binary_format::file_format::Visibility;
use move_model::{ast::Attribute, model::GlobalEnv};
use move_stackless_bytecode::{
    stackless_bytecode::{Bytecode, Operation},
    stackless_bytecode_generator::StacklessBytecodeGenerator,
    stackless_control_flow_graph::StacklessControlFlowGraph,
};

use sui_types::base_types::ObjectID;

use super::{self_transfer::SelfTransferAnalysis, share_owned::ShareOwnedAnalysis};

fn has_no_lint_attr(env: &GlobalEnv, attributes: &[Attribute]) -> bool {
    attributes.iter().any(|attr| {
        if let Attribute::Apply(_, sym, allow_attributes) = attr {
            if sym.display(env.symbol_pool()).to_string() == "allow" && allow_attributes.len() == 1
            {
                if let Attribute::Apply(_, attr_sym, _) = allow_attributes[0] {
                    if attr_sym.display(env.symbol_pool()).to_string() == "all" {
                        return true;
                    }
                }
            }
        }
        false
    })
}

pub fn lint_execute(env: GlobalEnv, published_addr: ObjectID, color: bool) -> String {
    // check for unused functions
    for module_env in env.get_modules() {
        if has_no_lint_attr(&env, module_env.get_attributes()) {
            // linting suppressed on a per-module basis
            continue;
        }
        if ObjectID::from_address(*module_env.self_address()) != published_addr {
            // do not look at dependencies
            continue;
        }
        for func_env in module_env.get_functions() {
            if has_no_lint_attr(&env, func_env.get_attributes()) {
                // linting suppressed on a per-function basis
                continue;
            }
            // module inits are supposed to be unused
            if func_env.visibility() != Visibility::Public
                && func_env.get_name_str() != "init"
                && func_env.get_calling_functions().is_empty()
            {
                env.diag(Severity::Error, &func_env.get_loc(), &format!("Unused private or `friend` function {}. This function should be called or deleted", func_env.get_full_name_str()))
            }
        }
    }

    for module_env in env.get_modules() {
        if has_no_lint_attr(&env, module_env.get_attributes()) {
            // linting suppressed on a per-module basis
            continue;
        }
        if ObjectID::from_address(*module_env.self_address()) != published_addr {
            // do not lint dependencies
            continue;
        }
        let mut packed_types = BTreeSet::new();
        for func_env in module_env.get_functions() {
            if has_no_lint_attr(&env, func_env.get_attributes()) {
                // linting suppressed on a per-function basis
                continue;
            }
            if func_env.is_native() {
                // do not lint on native functions
                continue;
            }
            let generator = StacklessBytecodeGenerator::new(&func_env);
            let fun_data = generator.generate_function();
            for instr in &fun_data.code {
                if let Bytecode::Call(_, _, Operation::Pack(_, sid, _), ..) = instr {
                    packed_types.insert(*sid);
                }
            }
            let cfg = StacklessControlFlowGraph::new_forward(&fun_data.code);
            // warn on calls of `public_transfer(.., tx_context::sender())`
            SelfTransferAnalysis::analyze(&func_env, &fun_data, &cfg);
            ShareOwnedAnalysis::analyze(&func_env, &fun_data, &cfg);
            // calls to additional linters should go here
        }
        // check for unused types
        for t in module_env.get_structs() {
            if has_no_lint_attr(&env, t.get_attributes()) {
                // linting suppressed on a per-type basis
                continue;
            }
            // TODO: better check for one-time witness. for now, we just use all caps as a
            // proxy. this will catch all OTW's, but will miss some unused structs
            if !packed_types.contains(&t.get_id())
                && t.get_name_string() != t.get_name_string().to_ascii_uppercase()
            {
                env.diag(
                    Severity::Error,
                    &t.get_loc(),
                    &format!(
                        "Unused struct type {}. This type should be used or deleted",
                        t.get_full_name_str()
                    ),
                )
            }
        }
    }
    let mut error_writer = if color {
        Buffer::ansi()
    } else {
        Buffer::no_color()
    };
    env.report_diag(&mut error_writer, Severity::Warning);
    String::from_utf8_lossy(&error_writer.into_inner()).to_string()
}
