// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::file_format::SignatureToken;

use crate::{execution::values::Value, jit::optimization::ast};

use std::collections::BTreeMap;

pub(crate) fn package(pkg: &mut ast::Package) -> bool {
    let mut changed = false;
    pkg.modules.iter_mut().for_each(|(_, m)| {
        module(&mut changed, m);
    });
    changed
}

fn module(changed: &mut bool, m: &mut ast::Module) {
    let ast::Module {
        functions,
        compiled_module,
    } = m;
    let constants: BTreeMap<usize, Value> = compiled_module
        .constant_pool()
        .iter()
        .enumerate()
        .filter_map(|(ndx, constant)| {
            if is_primitive_type(&constant.type_) {
                Value::deserialize_constant(constant).map(|const_| (ndx, const_))
            } else {
                None
            }
        })
        .collect::<BTreeMap<_, _>>();
    functions.iter_mut().for_each(|(_ndx, code)| {
        code.code
            .iter_mut()
            .for_each(|code| blocks(changed, &constants, &mut code.code))
    });
}

fn is_primitive_type(ty: &SignatureToken) -> bool {
    match ty {
        SignatureToken::Bool
        | SignatureToken::U8
        | SignatureToken::U64
        | SignatureToken::U128
        | SignatureToken::U16
        | SignatureToken::U32
        | SignatureToken::U256 => true,
        SignatureToken::Address
        | SignatureToken::Signer
        | SignatureToken::Vector(_)
        | SignatureToken::Datatype(_)
        | SignatureToken::DatatypeInstantiation(_)
        | SignatureToken::Reference(_)
        | SignatureToken::MutableReference(_)
        | SignatureToken::TypeParameter(_) => false,
    }
}

fn blocks(
    changed: &mut bool,
    constants: &BTreeMap<usize, Value>,
    blocks: &mut BTreeMap<ast::Label, Vec<ast::Bytecode>>,
) {
    for (_, block_code) in blocks.iter_mut() {
        substitute_constants(changed, constants, block_code);
    }
}

fn substitute_constants(
    changed: &mut bool,
    constants: &BTreeMap<usize, Value>,
    code: &mut [ast::Bytecode],
) {
    use ast::Bytecode;
    for instr in code.iter_mut() {
        if let Bytecode::LdConst(ndx) = &instr {
            if let Some(constant) = constants.get(&(ndx.0 as usize)) {
                *changed = true;
                match constant {
                    Value::U8(value) => *instr = Bytecode::LdU8(*value),
                    Value::U16(value) => *instr = Bytecode::LdU16(*value),
                    Value::U32(value) => *instr = Bytecode::LdU32(*value),
                    Value::U64(value) => *instr = Bytecode::LdU64(*value),
                    Value::U128(value) => *instr = Bytecode::LdU128(Box::new(**value)),
                    Value::U256(value) => *instr = Bytecode::LdU256(Box::new(**value)),
                    Value::Bool(true) => *instr = Bytecode::LdTrue,
                    Value::Bool(false) => *instr = Bytecode::LdFalse,
                    Value::Address(_)
                    | Value::Invalid
                    | Value::Container(_)
                    | Value::Reference(_) => unreachable!(),
                }
            }
        }
    }
}
