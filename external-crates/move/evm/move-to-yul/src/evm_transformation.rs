// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Performs transformations specific for compiling stackless bytecode to
//! EVM bytecode. Right now it only contains conversions of U256 native functions.

use crate::attributes;
use ethnum::U256;
use move_model::model::FunctionEnv;
use move_stackless_bytecode::{
    function_target::FunctionData,
    function_target_pipeline::{FunctionTargetProcessor, FunctionTargetsHolder},
    stackless_bytecode::{Bytecode, Constant, Operation},
};

const CAST_TO_U256_FUNCTION_NAME: &str = "u256_from_words";

pub struct EvmTransformationProcessor {}

impl EvmTransformationProcessor {
    pub fn new() -> Box<Self> {
        Box::new(EvmTransformationProcessor {})
    }
}

impl FunctionTargetProcessor for EvmTransformationProcessor {
    fn process(
        &self,
        _targets: &mut FunctionTargetsHolder,
        func_env: &FunctionEnv,
        mut data: FunctionData,
        _scc_opt: Option<&[FunctionEnv]>,
    ) -> FunctionData {
        if func_env.is_native() || func_env.is_intrinsic() {
            return data;
        }
        for code_offset in 0..data.code.len() {
            let bytecode = &data.code[code_offset];
            if let Bytecode::Call(
                attr_id,
                dst,
                Operation::Function(module_id, fun_id, _),
                src,
                abort_condition,
            ) = bytecode
            {
                if attributes::is_evm_arith_module(&func_env.module_env.env.get_module(*module_id))
                {
                    let fun_name = &*func_env
                        .module_env
                        .env
                        .get_module(*module_id)
                        .get_function(*fun_id)
                        .get_simple_name_string();
                    if fun_name.eq(CAST_TO_U256_FUNCTION_NAME) {
                        assert!(code_offset >= 2);
                        match (
                            get_loaded_u128(&data.code[code_offset - 2]),
                            get_loaded_u128(&data.code[code_offset - 1]),
                        ) {
                            (Some(hi), Some(lo)) => {
                                data.code[code_offset] = Bytecode::Load(
                                    *attr_id,
                                    dst[0],
                                    Constant::U256(U256::from_words(hi, lo)),
                                );
                            }
                            _ => {
                                data.code[code_offset] = Bytecode::Call(
                                    *attr_id,
                                    dst.clone(),
                                    Operation::CastU256,
                                    src.clone(),
                                    abort_condition.clone(),
                                );
                            }
                        }
                    } else if let Some(new_op) = transform_u256_op(fun_name) {
                        data.code[code_offset] = Bytecode::Call(
                            *attr_id,
                            dst.clone(),
                            new_op,
                            src.clone(),
                            abort_condition.clone(),
                        );
                    }
                }
            };
        }
        data
    }

    fn name(&self) -> String {
        "evm_transformation".to_string()
    }
}

fn get_loaded_u128(bc: &Bytecode) -> Option<u128> {
    if let Bytecode::Load(_, _, Constant::U128(x)) = bc {
        return Some(*x);
    }
    None
}

fn transform_u256_op(fun_name: &str) -> Option<Operation> {
    match fun_name {
        "add" => Some(Operation::Add),
        "sub" => Some(Operation::Sub),
        "mul" => Some(Operation::Mul),
        "div" => Some(Operation::Div),
        "mod" => Some(Operation::Mod),
        "eq" => Some(Operation::Eq),
        "ne" => Some(Operation::Neq),
        "gt" => Some(Operation::Gt),
        "lt" => Some(Operation::Lt),
        "ge" => Some(Operation::Ge),
        "le" => Some(Operation::Le),
        "shl" => Some(Operation::Shl),
        "shr" => Some(Operation::Shr),
        _ => None,
    }
}
