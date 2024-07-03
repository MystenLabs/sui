// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module verifies the usage of the "fake native" functions. These functions are declared
//! as 'native`, but do not appear in the compiled module. For developer sanity, they must be marked
//! with the `FAKE_NATIVE_ATTR`

use std::convert::TryInto;

use crate::{
    diag,
    expansion::ast::{Address, ModuleIdent, ModuleIdent_},
    naming::ast as N,
    parser::ast::FunctionName,
    shared::{known_attributes::NativeAttribute, CompilationEnv, Identifier},
};
use move_ir_types::ast as IR;
use move_symbol_pool::symbol;

/// verify fake native attribute usage usage
pub fn function(
    env: &mut CompilationEnv,
    module: ModuleIdent,
    function_name: FunctionName,
    function: &N::Function,
) {
    let loc = match function
        .attributes
        .get_loc_(&NativeAttribute::BytecodeInstruction.into())
    {
        None => return,
        Some(loc) => *loc,
    };
    if resolve_builtin(&module, &function_name).is_none() {
        let attr_msg = format!(
            "Invalid usage of '{}' attribute to map function to bytecode instruction.",
            NativeAttribute::BYTECODE_INSTRUCTION
        );
        let name_msg = format!(
            "No known mapping of '{}::{}' to bytecode instruction",
            module, function_name
        );
        let diag = diag!(
            Attributes::InvalidBytecodeInst,
            (loc, attr_msg),
            (function_name.loc(), name_msg),
        );
        env.add_diag(diag);
    }
    match &function.body.value {
        N::FunctionBody_::Native => (),
        N::FunctionBody_::Defined(_) => {
            let attr_msg = format!(
                "Invalid usage of '{}' attribute on non-native function",
                NativeAttribute::BYTECODE_INSTRUCTION
            );
            let diag = diag!(Attributes::InvalidBytecodeInst, (loc, attr_msg));
            env.add_diag(diag);
        }
    }
}

/// Resolve the mapping for a module + function name to a bytecode instruction.
/// The function should already be verified by `function` above
pub fn resolve_builtin(
    module: &ModuleIdent,
    function: &FunctionName,
) -> Option<fn(Vec<IR::Type>) -> IR::Bytecode_> {
    let sp!(_, ModuleIdent_ { address, module }) = module;
    // Only resolve if either (a) the address is named "std" or (b) its value is 0x1
    match address {
        Address::Numerical {
            name: Some(sp!(_, n)),
            ..
        }
        | Address::NamedUnassigned(sp!(_, n))
            if *n == symbol!("std") => {}
        _ => return None,
    };
    Some(match (module.value().as_str(), function.value().as_str()) {
        ("vector", "empty") => |tys| IR::Bytecode_::VecPack(expect_one_ty_arg(tys), 0),
        ("vector", "length") => |tys| IR::Bytecode_::VecLen(expect_one_ty_arg(tys)),
        ("vector", "borrow") => |tys| IR::Bytecode_::VecImmBorrow(expect_one_ty_arg(tys)),
        ("vector", "push_back") => |tys| IR::Bytecode_::VecPushBack(expect_one_ty_arg(tys)),
        ("vector", "borrow_mut") => |tys| IR::Bytecode_::VecMutBorrow(expect_one_ty_arg(tys)),
        ("vector", "pop_back") => |tys| IR::Bytecode_::VecPopBack(expect_one_ty_arg(tys)),
        ("vector", "destroy_empty") => |tys| IR::Bytecode_::VecUnpack(expect_one_ty_arg(tys), 0),
        ("vector", "swap") => |tys| IR::Bytecode_::VecSwap(expect_one_ty_arg(tys)),
        _ => return None,
    })
}

fn expect_one_ty_arg(ty_args: Vec<IR::Type>) -> IR::Type {
    let [ty]: [IR::Type; 1] = ty_args
        .try_into()
        .expect("ICE native bytecode function expected a single type argument");
    ty
}
