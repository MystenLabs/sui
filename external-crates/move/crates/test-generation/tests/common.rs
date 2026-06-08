// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

extern crate test_generation;
use move_binary_format::file_format::{Bytecode, FunctionInstantiation, StructDefInstantiation};
use test_generation::{
    abstract_state::AbstractState,
    summaries::{Effects, instruction_summary},
};

pub fn run_instruction(
    instruction: Bytecode,
    mut initial_state: AbstractState,
) -> (AbstractState, Bytecode) {
    let summary = instruction_summary(instruction.clone(), false);
    let unsatisfied_preconditions = summary
        .preconditions
        .iter()
        .any(|precondition| !precondition(&initial_state));
    assert!(
        !unsatisfied_preconditions,
        "preconditions of instruction not satisfied"
    );
    match summary.effects {
        Effects::TyParams(instantiation, effect, instantiation_application) => {
            let (struct_idx, instantiation) = instantiation(&initial_state);
            let index = initial_state.module.add_instantiation(instantiation);
            let struct_inst = StructDefInstantiation {
                def: struct_idx,
                type_parameters: index,
            };
            let str_inst_idx = initial_state.module.add_struct_instantiation(struct_inst);
            let effects = effect(str_inst_idx);
            let instruction = instantiation_application(str_inst_idx);
            (
                effects.iter().fold(initial_state, |acc, effect| {
                    effect(&acc)
                        .unwrap_or_else(|err| panic!("Error applying instruction effect: {}", err))
                }),
                instruction,
            )
        }
        Effects::TyParamsCall(instantiation, effect, instantiation_application) => {
            let (fh_idx, instantiation) = instantiation(&initial_state);
            let index = initial_state.module.add_instantiation(instantiation);
            let func_inst = FunctionInstantiation {
                handle: fh_idx,
                type_parameters: index,
            };
            let func_inst_idx = initial_state.module.add_function_instantiation(func_inst);
            let effects = effect(func_inst_idx);
            let instruction = instantiation_application(func_inst_idx);
            (
                effects.iter().fold(initial_state, |acc, effect| {
                    effect(&acc)
                        .unwrap_or_else(|err| panic!("Error applying instruction effect: {}", err))
                }),
                instruction,
            )
        }
        Effects::NoTyParams(effects) => (
            effects.iter().fold(initial_state, |acc, effect| {
                effect(&acc)
                    .unwrap_or_else(|err| panic!("Error applying instruction effect: {}", err))
            }),
            instruction,
        ),
    }
}
