// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Transformation which injects trace instructions which are used to visualize execution.
//!
//! This transformation should run before copy propagation and any other bytecode modifications.
//! It emits instructions of the form `trace_local[original_idx](idx)`. Initially
//! `original_idx == idx`, where the temp `idx` is a named variable from the Move
//! compiler. Later transformations may replace `idx` but `original_idx` will be preserved so
//! the user sees the value of their named variable.

use std::collections::BTreeSet;

use itertools::Itertools;

use move_model::model::FunctionEnv;

use crate::{
    exp_generator::ExpGenerator,
    function_data_builder::FunctionDataBuilder,
    function_target::FunctionData,
    function_target_pipeline::{FunctionTargetProcessor, FunctionTargetsHolder},
    spec_global_variable_analysis,
    stackless_bytecode::{Bytecode, Constant, Operation},
};

pub struct DebugInstrumenter {}

impl DebugInstrumenter {
    pub fn new() -> Box<Self> {
        Box::new(Self {})
    }
}

impl FunctionTargetProcessor for DebugInstrumenter {
    fn process(
        &self,
        targets: &mut FunctionTargetsHolder,
        fun_env: &FunctionEnv,
        data: FunctionData,
        _scc_opt: Option<&[FunctionEnv]>,
    ) -> FunctionData {
        use Bytecode::*;

        if fun_env.is_native() {
            // Nothing to do
            return data;
        }

        let mut builder = FunctionDataBuilder::new(fun_env, data);
        let code = std::mem::take(&mut builder.data.code);

        // Emit trace instructions for parameters at entry.
        builder.set_loc(builder.fun_env.get_loc().at_start());
        for i in 0..builder.fun_env.get_parameter_count() {
            builder.emit_with(|id| Call(id, vec![], Operation::TraceLocal(i), vec![i], None));
        }

        // For spec functions, emit trace instructions for all global variables at entry.
        if targets.is_spec(&fun_env.get_qualified_id()) {
            for tys in spec_global_variable_analysis::get_info(&builder.data)
                .all_vars()
                .cloned()
                .collect_vec()
            {
                builder.emit_with(|id| {
                    Call(
                        id,
                        vec![],
                        Operation::TraceGhost(tys[0].clone(), tys[1].clone()),
                        vec![],
                        None,
                    )
                });
            }
        }

        for bc in code {
            let bc_clone = bc.clone();
            match &bc {
                Ret(id, locals) => {
                    // Emit trace instructions for return values.
                    builder.set_loc_from_attr(*id);
                    for (i, l) in locals.iter().enumerate() {
                        builder.emit_with(|id| {
                            Call(id, vec![], Operation::TraceReturn(i), vec![*l], None)
                        });
                    }
                    builder.emit(bc);
                }
                Abort(id, l) => {
                    builder.set_loc_from_attr(*id);
                    builder.emit_with(|id| Call(id, vec![], Operation::TraceAbort, vec![*l], None));
                    builder.emit(bc);
                }
                Call(_, _, Operation::WriteRef, srcs, _) if srcs[0] < fun_env.get_local_count() => {
                    builder.set_loc_from_attr(bc.get_attr_id());
                    builder.emit(bc.clone());
                    builder.emit_with(|id| {
                        Call(
                            id,
                            vec![],
                            Operation::TraceLocal(srcs[0]),
                            vec![srcs[0]],
                            None,
                        )
                    });
                }
                Call(_, dests, Operation::Function(mid, fid, _), srcs, _)
                    if mid.qualified(*fid) == fun_env.module_env.env.log_text_qid() =>
                {
                    assert!(dests.is_empty());
                    assert_eq!(1, srcs.len());
                    let message = match builder.data.code.last() {
                        Some(Bytecode::Load(_, last_dest, Constant::ByteArray(bytes)))
                            if srcs[0] == *last_dest =>
                        {
                            String::from_utf8_lossy(bytes)
                                .to_string()
                                .escape_debug()
                                .to_string()
                        }
                        _ => panic!("log text should be preceded by load byte array"),
                    };
                    builder.set_loc_from_attr(bc.get_attr_id());
                    builder.emit_with(|id| {
                        Call(id, vec![], Operation::TraceMessage(message), vec![], None)
                    });
                }
                Call(_, dests, Operation::Function(mid, fid, _), srcs, _)
                    if mid.qualified(*fid) == fun_env.module_env.env.log_var_qid() =>
                {
                    assert!(dests.is_empty());
                    assert_eq!(1, srcs.len());
                    let var = match builder.data.code.last() {
                        Some(Bytecode::Call(_, last_dests, Operation::BorrowLoc, last_srcs, _))
                            if srcs[0] == last_dests[0]
                                && 1 == last_dests.len()
                                && 1 == last_srcs.len()
                                // check the argument is a local
                                && !fun_env.is_temporary(last_srcs[0]) =>
                        {
                            last_srcs[0]
                        }
                        _ => panic!("log variable should be preceded by borrow local"),
                    };
                    builder.set_loc_from_attr(bc.get_attr_id());
                    builder.emit_with(|id| {
                        Call(id, vec![], Operation::TraceLocal(var), vec![var], None)
                    });
                }
                Call(_, dests, Operation::Function(mid, fid, tys), srcs, _)
                    if mid.qualified(*fid) == fun_env.module_env.env.log_ghost_qid() =>
                {
                    assert!(dests.is_empty());
                    assert!(srcs.is_empty());
                    assert_eq!(2, tys.len());
                    builder.set_loc_from_attr(bc.get_attr_id());
                    builder.emit_with(|id| {
                        Call(
                            id,
                            vec![],
                            Operation::TraceGhost(tys[0].clone(), tys[1].clone()),
                            vec![],
                            None,
                        )
                    });
                }
                _ => {
                    builder.set_loc_from_attr(bc.get_attr_id());
                    builder.emit(bc.clone());
                    // Emit trace instructions for modified values.
                    let (val_targets, mut_targets) = bc.modifies(&builder.get_target());
                    let affected_variables: BTreeSet<_> = val_targets
                        .into_iter()
                        .chain(mut_targets.into_iter().map(|(idx, _)| idx))
                        .collect();
                    for idx in affected_variables {
                        // Only emit this for user declared locals, not for ones introduced
                        // by stack elimination.
                        if !fun_env.is_temporary(idx) {
                            builder.emit_with(|id| {
                                Call(id, vec![], Operation::TraceLocal(idx), vec![idx], None)
                            });
                        }
                    }
                }
            }
        }

        builder.data
    }

    fn name(&self) -> String {
        "debug_instrumenter".to_string()
    }
}
