// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Transformation which mutates code for mutation testing in various ways
//!
//! This transformation should run after code is translated to bytecode, but before any
//!  other bytecode modification
//! It emits instructions in bytecode format, but with changes made
//! Note that this mutation does nothing if mutation flags are not enabled

use move_model::{
    exp_generator::ExpGenerator,
    model::{FunctionEnv, GlobalEnv},
};

use crate::{
    function_data_builder::FunctionDataBuilder,
    function_target::FunctionData,
    function_target_pipeline::{FunctionTargetProcessor, FunctionTargetsHolder},
    options::ProverOptions,
    stackless_bytecode::{Bytecode, Operation},
};

pub struct MutationTester {}

pub struct MutationManager {
    pub mutated: bool,
    pub add_sub: usize,
    pub sub_add: usize,
    pub mul_div: usize,
    pub div_mul: usize,
}

impl MutationTester {
    pub fn new() -> Box<Self> {
        Box::new(Self {})
    }
}

fn mutate_arith(
    call: Bytecode,
    mutation_value: usize,
    global_env: &GlobalEnv,
    mutation_manager: MutationManager,
    bc: Bytecode,
) -> Bytecode {
    if mutation_value > 1 {
        global_env.set_extension(MutationManager {
            mutated: mutation_manager.mutated,
            add_sub: mutation_manager.add_sub,
            sub_add: mutation_manager.sub_add,
            mul_div: mutation_manager.mul_div,
            div_mul: mutation_manager.div_mul,
        });
    }
    if mutation_value == 1 {
        global_env.set_extension(MutationManager {
            mutated: true,
            add_sub: mutation_manager.add_sub,
            sub_add: mutation_manager.sub_add,
            mul_div: mutation_manager.mul_div,
            div_mul: mutation_manager.div_mul,
        });
        call
    } else {
        bc
    }
}

impl FunctionTargetProcessor for MutationTester {
    fn initialize(&self, global_env: &GlobalEnv, _targets: &mut FunctionTargetsHolder) {
        let options = ProverOptions::get(global_env);
        let m = global_env.get_extension::<MutationManager>();
        match m {
            Some(x) => global_env.set_extension(MutationManager { ..*x }),
            None => global_env.set_extension(MutationManager {
                mutated: false,
                add_sub: options.mutation_add_sub,
                sub_add: options.mutation_sub_add,
                mul_div: options.mutation_mul_div,
                div_mul: options.mutation_div_mul,
            }),
        };
    }

    fn process(
        &self,
        _targets: &mut FunctionTargetsHolder,
        fun_env: &FunctionEnv,
        data: FunctionData,
        _scc_opt: Option<&[FunctionEnv]>,
    ) -> FunctionData {
        use Bytecode::*;

        if fun_env.is_native() {
            // Nothing to do
            return data;
        }

        if !data.variant.is_verified() {
            // Only need to instrument if this is a verification variant
            return data;
        }

        let mut builder = FunctionDataBuilder::new(fun_env, data);
        let code = std::mem::take(&mut builder.data.code);

        builder.set_loc(builder.fun_env.get_loc().at_start());
        let global_env = fun_env.module_env.env;
        let m = global_env.get_extension::<MutationManager>().unwrap();

        for bc in code {
            match bc {
                Call(ref attrid, ref indices, Operation::Add, ref srcs, ref dests) => {
                    let call = Call(
                        *attrid,
                        (*indices).clone(),
                        Operation::Sub,
                        (*srcs).clone(),
                        (*dests).clone(),
                    );
                    let mv = m.add_sub;

                    let result = if mv > 0 { mv - 1 } else { mv };
                    let mm = MutationManager {
                        add_sub: result,
                        ..*m
                    };
                    builder.emit(mutate_arith(call, mv, global_env, mm, bc));
                }
                Call(ref attrid, ref indices, Operation::Sub, ref srcs, ref dests) => {
                    let call = Call(
                        *attrid,
                        (*indices).clone(),
                        Operation::Add,
                        (*srcs).clone(),
                        (*dests).clone(),
                    );
                    let mv = m.sub_add;

                    let result = if mv > 0 { mv - 1 } else { mv };
                    let mm = MutationManager {
                        sub_add: result,
                        ..*m
                    };
                    builder.emit(mutate_arith(call, mv, global_env, mm, bc));
                }
                Call(ref attrid, ref indices, Operation::Mul, ref srcs, ref dests) => {
                    let call = Call(
                        *attrid,
                        (*indices).clone(),
                        Operation::Div,
                        (*srcs).clone(),
                        (*dests).clone(),
                    );
                    let mv = m.mul_div;

                    let result = if mv > 0 { mv - 1 } else { mv };
                    let mm = MutationManager {
                        mul_div: result,
                        ..*m
                    };
                    builder.emit(mutate_arith(call, mv, global_env, mm, bc));
                }
                Call(ref attrid, ref indices, Operation::Div, ref srcs, ref dests) => {
                    let call = Call(
                        *attrid,
                        (*indices).clone(),
                        Operation::Mul,
                        (*srcs).clone(),
                        (*dests).clone(),
                    );
                    let mv = m.mul_div;
                    let result = if mv > 0 { mv - 1 } else { mv };
                    let mm = MutationManager {
                        div_mul: result,
                        ..*m
                    };
                    builder.emit(mutate_arith(call, mv, global_env, mm, bc));
                }
                _ => {
                    builder.emit(bc);
                }
            }
        }

        builder.data
    }

    fn name(&self) -> String {
        "mutation_tester".to_string()
    }
}
