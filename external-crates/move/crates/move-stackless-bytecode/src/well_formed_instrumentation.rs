// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Transformation which injects well-formed assumptions at top-level entry points of verified
//! functions. These assumptions are both about parameters and any memory referred to by
//! the code. For ghost memory, the transformation also assumes initial values if provided.
//!
//! This needs to be run *after* function specifications and global invariants have been
//! injected because only then we know all accessed memory.
//!
//! This phase need to be run *before* data invariant instrumentation, because the latter relies
//! on the well-formed assumptions, augmenting them with the data invariant.
//! Because data invariants cannot refer to global memory, they are not relevant for memory
//! usage, and their injection therefore can happen after this phase.

use num::{BigUint, Zero};

use move_model::{
    ast::{Operation, QuantKind},
    exp_generator::ExpGenerator,
    model::FunctionEnv,
    ty::BOOL_TYPE,
};

use crate::{
    function_data_builder::FunctionDataBuilder,
    function_target::FunctionData,
    function_target_pipeline::{FunctionTargetProcessor, FunctionTargetsHolder},
    stackless_bytecode::PropKind,
    usage_analysis::UsageProcessor,
};

pub struct WellFormedInstrumentationProcessor {}

impl WellFormedInstrumentationProcessor {
    pub fn new() -> Box<Self> {
        Box::new(Self {})
    }
}

impl FunctionTargetProcessor for WellFormedInstrumentationProcessor {
    fn process(
        &self,
        targets: &mut FunctionTargetsHolder,
        fun_env: &FunctionEnv,
        data: FunctionData,
        _scc_opt: Option<&[FunctionEnv]>,
    ) -> FunctionData {
        if !data.variant.is_verified() {
            // only need to do this for verified functions
            return data;
        }
        // Rerun usage analysis for this function.
        let usage = UsageProcessor::analyze(targets, fun_env, &data);
        let mut builder = FunctionDataBuilder::new(fun_env, data);
        builder.set_loc(fun_env.get_loc().at_start());
        let old_code = std::mem::take(&mut builder.data.code);

        // Inject well-formedness assumptions for parameters.
        for param in 0..builder.fun_env.get_parameter_count() {
            let exp = builder.mk_call(
                &BOOL_TYPE,
                Operation::WellFormed,
                vec![builder.mk_temporary(param)],
            );
            builder.emit_prop(PropKind::Assume, exp);
        }

        // Inject well-formedness assumption for used memory.
        for mem in usage.accessed.all {
            let struct_env = builder.global_env().get_struct_qid(mem.to_qualified_id());
            if struct_env.is_native_or_intrinsic() {
                // If this is native or intrinsic memory, skip this.
                continue;
            }
            let exp = builder
                .mk_inst_mem_quant_opt(QuantKind::Forall, &mem, &mut |val| {
                    Some(builder.mk_call(&BOOL_TYPE, Operation::WellFormed, vec![val]))
                })
                .expect("quant defined");
            builder.emit_prop(PropKind::Assume, exp);

            // If this is ghost memory, assume it exists, and if it has an initializer,
            // assume it has this value.
            if let Some(spec_var) = struct_env.get_ghost_memory_spec_var() {
                let mem_ty = mem.to_type();
                let zero_addr = builder.mk_address_const(BigUint::zero());
                let exists = builder.mk_call_with_inst(
                    &BOOL_TYPE,
                    vec![mem_ty.clone()],
                    Operation::Exists(None),
                    vec![zero_addr.clone()],
                );
                builder.emit_prop(PropKind::Assume, exists);
                let svar_module = builder.global_env().get_module(spec_var.module_id);
                let svar = svar_module.get_spec_var(spec_var.id);
                if let Some(init) = &svar.init {
                    let mem_val = builder.mk_call_with_inst(
                        &mem_ty,
                        mem.inst.clone(),
                        Operation::Pack(mem.module_id, mem.id),
                        vec![init.clone()],
                    );
                    let mem_access = builder.mk_call_with_inst(
                        &mem_ty,
                        vec![mem_ty.clone()],
                        Operation::Global(None),
                        vec![zero_addr],
                    );
                    let eq_with_init = builder.mk_identical(mem_access, mem_val);
                    builder.emit_prop(PropKind::Assume, eq_with_init);
                }
            }
        }

        // Append the old code
        for bc in old_code {
            builder.emit(bc);
        }

        builder.data
    }

    fn name(&self) -> String {
        "entry_point_instrumenter".to_string()
    }
}
