use bimap::BiBTreeMap;
use itertools::Itertools;
use std::collections::BTreeMap;

use move_model::model::{FunctionEnv, GlobalEnv};

use crate::{
    function_data_builder::FunctionDataBuilder,
    function_target::FunctionData,
    function_target_pipeline::{FunctionTargetProcessor, FunctionTargetsHolder},
    stackless_bytecode::{Bytecode, Operation},
};

pub struct MoveLoopInvariantsProcessor {}

impl MoveLoopInvariantsProcessor {
    pub fn new() -> Box<Self> {
        Box::new(Self {})
    }
}

impl FunctionTargetProcessor for MoveLoopInvariantsProcessor {
    fn process(
        &self,
        _targets: &mut FunctionTargetsHolder,
        func_env: &FunctionEnv,
        data: FunctionData,
        _scc_opt: Option<&[FunctionEnv]>,
    ) -> FunctionData {
        if func_env.is_native() {
            return data;
        }

        let invariants = get_invariants(&func_env.module_env.env, &data.code);

        let mut builder = FunctionDataBuilder::new(func_env, data);
        let code = std::mem::take(&mut builder.data.code);
        let invariant_labels = invariants
            .iter()
            .map(|(begin, end)| {
                if matches!(code[*end + 1], Bytecode::Label(..)) {
                    // TODO: check if the label is the header of a loop
                    (*begin, code[*end + 1].clone())
                } else {
                    panic!("A loop invariant should end with a label")
                }
            })
            .collect::<BTreeMap<_, _>>();
        for (offset, bc) in code.into_iter().enumerate() {
            if let Some(label_bc) = invariant_labels.get(&offset) {
                builder.emit(label_bc.clone());
            }
            if invariants.contains_right(&(offset - 1)) {
                continue;
            }
            builder.emit(bc);
        }

        builder.data
    }

    fn name(&self) -> String {
        "move_loop_invariant".to_string()
    }
}

pub fn get_invariants(env: &GlobalEnv, code: &[Bytecode]) -> BiBTreeMap<usize, usize> {
    let invariant_begin_function = Operation::apply_fun_qid(&env.invariant_begin_qid(), vec![]);
    let invariant_end_function = Operation::apply_fun_qid(&env.invariant_end_qid(), vec![]);
    let begin_offsets = code.iter().enumerate().filter_map(|(i, bc)| match bc {
        Bytecode::Call(_, _, op, _, _) if *op == invariant_begin_function => Some(i),
        _ => None,
    });
    let end_offsets = code.iter().enumerate().filter_map(|(i, bc)| match bc {
        Bytecode::Call(_, _, op, _, _) if *op == invariant_end_function => Some(i),
        _ => None,
    });
    begin_offsets
        // TODO: check if the begin_offsets and end_offsets are well paired
        .zip_eq(end_offsets)
        .collect()
}
