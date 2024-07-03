// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Analysis on partitioning temp variables, struct fields and function parameters according to involved operations (arithmetic or bitwise)
//
// The result of this analysis will be used when generating the boogie code

use crate::{
    dataflow_analysis::{DataflowAnalysis, TransferFunctions},
    dataflow_domains::{AbstractDomain, JoinResult},
    function_target::FunctionTarget,
    function_target_pipeline::{
        FunctionTargetPipeline, FunctionTargetProcessor, FunctionTargetsHolder, FunctionVariant,
    },
    number_operation::{
        GlobalNumberOperationState,
        NumOperation::{self, Arithmetic, Bitwise, Bottom},
    },
    options::ProverOptions,
    stackless_bytecode::{AttrId, Bytecode, Operation},
    stackless_control_flow_graph::StacklessControlFlowGraph,
};
use itertools::Either;
use move_binary_format::file_format::CodeOffset;
use move_model::{
    ast::TempIndex,
    model::{FunId, GlobalEnv, ModuleId},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    str,
};

static CONFLICT_ERROR_MSG: &str = "cannot appear in both arithmetic and bitwise operation";

pub struct NumberOperationProcessor {}

impl NumberOperationProcessor {
    pub fn new() -> Box<Self> {
        Box::new(NumberOperationProcessor {})
    }

    /// Create initial number operation state for expressions
    pub fn create_initial_exp_oper_state(&self, env: &GlobalEnv) {
        let mut default_exp = BTreeMap::new();
        let exp_info_map = env.get_nodes();
        for id in exp_info_map {
            default_exp.insert(id, Bottom);
        }
        let mut global_state = env.get_cloned_extension::<GlobalNumberOperationState>();
        global_state.exp_operation_map = default_exp;
        env.set_extension(global_state);
    }

    /// Entry point of the analysis
    fn analyze<'a>(&self, env: &'a GlobalEnv, targets: &'a FunctionTargetsHolder) {
        self.create_initial_exp_oper_state(env);
        let fun_env_vec = FunctionTargetPipeline::sort_targets_in_topological_order(env, targets);
        for item in &fun_env_vec {
            match item {
                Either::Left(fid) => {
                    let func_env = env.get_function(*fid);
                    for (_, target) in targets.get_targets(&func_env) {
                        if target.data.code.is_empty() {
                            continue;
                        }
                        self.analyze_fun(env, target.clone());
                    }
                }
                Either::Right(scc) => {
                    for fid in scc {
                        let func_env = env.get_function(*fid);
                        for (_, target) in targets.get_targets(&func_env) {
                            if target.data.code.is_empty() {
                                continue;
                            }
                            self.analyze_fun(env, target.clone());
                        }
                    }
                }
            }
        }
    }

    fn analyze_fun(&self, env: &GlobalEnv, target: FunctionTarget) {
        if !target.func_env.is_native() {
            let cfg = StacklessControlFlowGraph::one_block(target.get_bytecode());
            let analyzer = NumberOperationAnalysis {
                func_target: target,
                ban_int_2_bv_conversion: ProverOptions::get(env).ban_int_2_bv,
            };
            analyzer.analyze_function(
                NumberOperationState::create_initial_state(),
                analyzer.func_target.get_bytecode(),
                &cfg,
            );
        }
    }
}

impl FunctionTargetProcessor for NumberOperationProcessor {
    fn is_single_run(&self) -> bool {
        true
    }

    fn run(&self, env: &GlobalEnv, targets: &mut FunctionTargetsHolder) {
        self.analyze(env, targets);
    }

    fn name(&self) -> String {
        "number_operation_analysis".to_string()
    }
}

struct NumberOperationAnalysis<'a> {
    func_target: FunctionTarget<'a>,
    ban_int_2_bv_conversion: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, PartialOrd)]
struct NumberOperationState {
    // Flag to mark whether the global state has been changed in one pass
    pub changed: bool,
}

impl NumberOperationState {
    /// Create a default NumberOperationState
    fn create_initial_state() -> Self {
        NumberOperationState { changed: false }
    }
}

fn vector_table_funs_name_propogate_to_dest(callee_name: &str) -> bool {
    callee_name.contains("borrow")
        || callee_name.contains("borrow_mut")
        || callee_name.contains("pop_back")
        || callee_name.contains("singleton")
        || callee_name.contains("remove")
        || callee_name.contains("swap_remove")
        || callee_name.contains("spec_get")
}

fn vector_funs_name_propogate_to_srcs(callee_name: &str) -> bool {
    callee_name == "contains"
        || callee_name == "index_of"
        || callee_name == "append"
        || callee_name == "push_back"
        || callee_name == "insert"
}

fn table_funs_name_propogate_to_srcs(callee_name: &str) -> bool {
    callee_name == "add" || callee_name == "borrow_mut_with_default" || callee_name == "upsert"
}

impl<'a> NumberOperationAnalysis<'a> {
    /// Check whether operations in s conflicting
    fn check_conflict_set(&self, s: &BTreeSet<&NumOperation>) -> bool {
        if self.ban_int_2_bv_conversion {
            let mut arith_flag = false;
            let mut bitwise_flag = false;
            for &oper in s {
                if *oper == Arithmetic {
                    arith_flag = true;
                }
                if *oper == Bitwise {
                    bitwise_flag = true;
                }
            }
            arith_flag && bitwise_flag
        } else {
            false
        }
    }

    /// Check whether oper_1 and oper_2 conflict
    fn check_conflict(&self, oper_1: &NumOperation, oper_2: &NumOperation) -> bool {
        if self.ban_int_2_bv_conversion {
            oper_1.conflict(oper_2)
        } else {
            false
        }
    }

    /// Check whether operation of dest and src conflict, if not propagate the merged operation
    fn check_and_propagate(
        &self,
        id: &AttrId,
        state: &mut NumberOperationState,
        dest: &TempIndex,
        src: &TempIndex,
        mid: ModuleId,
        fid: FunId,
        global_state: &mut GlobalNumberOperationState,
        baseline_flag: bool,
    ) {
        // Each TempIndex has a default operation in the map, can unwrap
        let dest_oper = global_state
            .get_temp_index_oper(mid, fid, *dest, baseline_flag)
            .unwrap();
        let src_oper = global_state
            .get_temp_index_oper(mid, fid, *src, baseline_flag)
            .unwrap();
        if self.check_conflict(dest_oper, src_oper) {
            self.func_target
                .func_env
                .module_env
                .env
                .error(&self.func_target.get_bytecode_loc(*id), CONFLICT_ERROR_MSG);
        } else {
            let merged_oper = dest_oper.merge(src_oper);
            if merged_oper != *dest_oper || merged_oper != *src_oper {
                state.changed = true;
            }
            *global_state
                .get_mut_temp_index_oper(mid, fid, *dest, baseline_flag)
                .unwrap() = merged_oper;
            *global_state
                .get_mut_temp_index_oper(mid, fid, *src, baseline_flag)
                .unwrap() = merged_oper;
        }
    }

    /// Update operation in dests and srcs using oper
    fn check_and_update_oper(
        &self,
        id: &AttrId,
        state: &mut NumberOperationState,
        dests: &[TempIndex],
        srcs: &[TempIndex],
        oper: NumOperation,
        mid: ModuleId,
        fid: FunId,
        global_state: &mut GlobalNumberOperationState,
        baseline_flag: bool,
    ) {
        let op_srcs_0 = global_state
            .get_temp_index_oper(mid, fid, srcs[0], baseline_flag)
            .unwrap();
        let op_srcs_1 = global_state
            .get_temp_index_oper(mid, fid, srcs[1], baseline_flag)
            .unwrap();
        let op_dests_0 = global_state
            .get_temp_index_oper(mid, fid, dests[0], baseline_flag)
            .unwrap();
        // Check conflicts among dests and srcs
        let mut state_set = BTreeSet::new();
        state_set.insert(op_srcs_0);
        state_set.insert(op_srcs_1);
        state_set.insert(op_dests_0);
        if self.check_conflict_set(&state_set) {
            self.func_target
                .func_env
                .module_env
                .env
                .error(&self.func_target.get_bytecode_loc(*id), CONFLICT_ERROR_MSG);
            return;
        }
        if oper != *op_srcs_0 || oper != *op_srcs_1 || oper != *op_dests_0 {
            state.changed = true;
        }
        *global_state
            .get_mut_temp_index_oper(mid, fid, srcs[0], baseline_flag)
            .unwrap() = oper;
        *global_state
            .get_mut_temp_index_oper(mid, fid, srcs[1], baseline_flag)
            .unwrap() = oper;
        *global_state
            .get_mut_temp_index_oper(mid, fid, dests[0], baseline_flag)
            .unwrap() = oper;
    }

    fn check_and_update_oper_dest(
        &self,
        state: &mut NumberOperationState,
        dests: &[TempIndex],
        oper: NumOperation,
        mid: ModuleId,
        fid: FunId,
        global_state: &mut GlobalNumberOperationState,
        baseline_flag: bool,
    ) {
        let op_dests_0 = global_state
            .get_temp_index_oper(mid, fid, dests[0], baseline_flag)
            .unwrap();
        if oper != *op_dests_0 {
            state.changed = true;
        }
        *global_state
            .get_mut_temp_index_oper(mid, fid, dests[0], baseline_flag)
            .unwrap() = oper;
    }

    /// Generate default num_oper for all non-parameter locals
    fn populate_non_param_oper(&self, global_state: &mut GlobalNumberOperationState) {
        let mid = self.func_target.func_env.module_env.get_id();
        let fid = self.func_target.func_env.get_id();
        let non_param_range = self.func_target.get_non_parameter_locals();
        let baseline_flag = self.func_target.data.variant == FunctionVariant::Baseline;
        for i in non_param_range {
            if !global_state
                .get_non_param_local_map(mid, fid, baseline_flag)
                .contains_key(&i)
            {
                global_state
                    .get_mut_non_param_local_map(mid, fid, baseline_flag)
                    .insert(i, Bottom);
            }
        }
    }
}

impl<'a> TransferFunctions for NumberOperationAnalysis<'a> {
    type State = NumberOperationState;
    const BACKWARD: bool = false;

    /// Update global state of num_operation by analyzing each instruction
    fn execute(&self, state: &mut NumberOperationState, instr: &Bytecode, _offset: CodeOffset) {
        use Bytecode::*;
        use Operation::*;
        let mut global_state = self
            .func_target
            .global_env()
            .get_cloned_extension::<GlobalNumberOperationState>();
        self.populate_non_param_oper(&mut global_state);
        let baseline_flag = self.func_target.data.variant == FunctionVariant::Baseline;
        let cur_mid = self.func_target.func_env.module_env.get_id();
        let cur_fid = self.func_target.func_env.get_id();
        match instr {
            Assign(id, dest, src, _) => {
                self.check_and_propagate(
                    id,
                    state,
                    dest,
                    src,
                    cur_mid,
                    cur_fid,
                    &mut global_state,
                    baseline_flag,
                );
            }
            // Check and update operations of rets in temp_index_operation_map and operations in ret_operation_map
            Ret(id, rets) => {
                let ret_types = self.func_target.get_return_types();
                for ((i, _), ret) in ret_types.iter().enumerate().zip(rets) {
                    let ret_oper = global_state
                        .get_ret_map()
                        .get(&(cur_mid, cur_fid))
                        .unwrap()
                        .get(&i)
                        .unwrap();
                    let idx_oper = global_state
                        .get_temp_index_oper(cur_mid, cur_fid, *ret, baseline_flag)
                        .unwrap();

                    if self.check_conflict(idx_oper, ret_oper) {
                        self.func_target
                            .func_env
                            .module_env
                            .env
                            .error(&self.func_target.get_bytecode_loc(*id), CONFLICT_ERROR_MSG);
                    } else {
                        let merged = idx_oper.merge(ret_oper);
                        if merged != *idx_oper || merged != *ret_oper {
                            state.changed = true;
                        }
                        *global_state
                            .get_mut_temp_index_oper(cur_mid, cur_fid, *ret, baseline_flag)
                            .unwrap() = merged;
                        global_state
                            .get_mut_ret_map()
                            .get_mut(&(cur_mid, cur_fid))
                            .unwrap()
                            .insert(i, merged);
                    }
                }
            }
            Call(id, dests, oper, srcs, _) => {
                match oper {
                    BorrowLoc | ReadRef | CastU8 | CastU16 | CastU32 | CastU64 | CastU128
                    | CastU256 => {
                        self.check_and_propagate(
                            id,
                            state,
                            &dests[0],
                            &srcs[0],
                            cur_mid,
                            cur_fid,
                            &mut global_state,
                            baseline_flag,
                        );
                    }
                    WriteRef | Lt | Le | Gt | Ge | Eq | Neq => {
                        self.check_and_propagate(
                            id,
                            state,
                            &srcs[0],
                            &srcs[1],
                            cur_mid,
                            cur_fid,
                            &mut global_state,
                            baseline_flag,
                        );
                    }
                    Add | Sub | Mul | Div | Mod => {
                        let mut num_oper = Arithmetic;
                        if !self.ban_int_2_bv_conversion {
                            let op_srcs_0 = global_state
                                .get_temp_index_oper(cur_mid, cur_fid, srcs[0], baseline_flag)
                                .unwrap();
                            let op_srcs_1 = global_state
                                .get_temp_index_oper(cur_mid, cur_fid, srcs[1], baseline_flag)
                                .unwrap();
                            let op_dests_0 = global_state
                                .get_temp_index_oper(cur_mid, cur_fid, dests[0], baseline_flag)
                                .unwrap();
                            // If there is conflict among operations, merged will not be used for updating
                            num_oper = op_srcs_0.merge(op_srcs_1).merge(op_dests_0);
                        }
                        self.check_and_update_oper(
                            id,
                            state,
                            dests,
                            srcs,
                            num_oper,
                            cur_mid,
                            cur_fid,
                            &mut global_state,
                            baseline_flag,
                        );
                    }
                    BitOr | BitAnd | Xor => {
                        if self.ban_int_2_bv_conversion {
                            self.check_and_update_oper(
                                id,
                                state,
                                dests,
                                srcs,
                                Bitwise,
                                cur_mid,
                                cur_fid,
                                &mut global_state,
                                baseline_flag,
                            );
                        } else {
                            self.check_and_update_oper_dest(
                                state,
                                dests,
                                Bitwise,
                                cur_mid,
                                cur_fid,
                                &mut global_state,
                                baseline_flag,
                            )
                        }
                    }
                    Shl | Shr => {
                        let op_srcs_0 = global_state
                            .get_temp_index_oper(cur_mid, cur_fid, srcs[0], baseline_flag)
                            .unwrap();
                        let op_srcs_1 = global_state
                            .get_temp_index_oper(cur_mid, cur_fid, srcs[1], baseline_flag)
                            .unwrap();
                        let op_dests_0 = global_state
                            .get_temp_index_oper(cur_mid, cur_fid, dests[0], baseline_flag)
                            .unwrap();
                        // If there is conflict among operations, merged will not be used for updating
                        let merged = op_srcs_0.merge(op_srcs_1).merge(op_dests_0);
                        self.check_and_update_oper(
                            id,
                            state,
                            dests,
                            srcs,
                            merged,
                            cur_mid,
                            cur_fid,
                            &mut global_state,
                            baseline_flag,
                        );
                    }
                    // Checking and operations in the struct_operation_map when packing
                    Pack(msid, sid, _) => {
                        let struct_env = self
                            .func_target
                            .global_env()
                            .get_module(*msid)
                            .into_struct(*sid);
                        for (i, field) in struct_env.get_fields().enumerate() {
                            let current_field_oper = global_state
                                .struct_operation_map
                                .get(&(*msid, *sid))
                                .unwrap()
                                .get(&field.get_id())
                                .unwrap();
                            let pack_oper = global_state
                                .get_temp_index_oper(cur_mid, cur_fid, srcs[i], baseline_flag)
                                .unwrap();
                            if self.check_conflict(current_field_oper, pack_oper) {
                                self.func_target.func_env.module_env.env.error(
                                    &self.func_target.get_bytecode_loc(*id),
                                    CONFLICT_ERROR_MSG,
                                );
                            } else {
                                let merged = current_field_oper.merge(pack_oper);
                                if merged != *current_field_oper || merged != *pack_oper {
                                    state.changed = true;
                                }
                                *global_state
                                    .get_mut_temp_index_oper(
                                        cur_mid,
                                        cur_fid,
                                        srcs[i],
                                        baseline_flag,
                                    )
                                    .unwrap() = merged;
                                global_state
                                    .struct_operation_map
                                    .get_mut(&(*msid, *sid))
                                    .unwrap()
                                    .insert(field.get_id(), merged);
                            }
                        }
                    }
                    // Checking and operations in the struct_operation_map when unpacking
                    Unpack(msid, sid, _) => {
                        let struct_env = self
                            .func_target
                            .global_env()
                            .get_module(*msid)
                            .into_struct(*sid);
                        for (i, field) in struct_env.get_fields().enumerate() {
                            let current_field_oper = global_state
                                .struct_operation_map
                                .get(&(*msid, *sid))
                                .unwrap()
                                .get(&field.get_id())
                                .unwrap();
                            let pack_oper = global_state
                                .get_temp_index_oper(cur_mid, cur_fid, dests[i], baseline_flag)
                                .unwrap();
                            if self.check_conflict(current_field_oper, pack_oper) {
                                self.func_target.func_env.module_env.env.error(
                                    &self.func_target.get_bytecode_loc(*id),
                                    CONFLICT_ERROR_MSG,
                                );
                            } else {
                                let merged = current_field_oper.merge(pack_oper);
                                if merged != *current_field_oper || merged != *pack_oper {
                                    state.changed = true;
                                }
                                *global_state
                                    .get_mut_temp_index_oper(
                                        cur_mid,
                                        cur_fid,
                                        dests[i],
                                        baseline_flag,
                                    )
                                    .unwrap() = merged;
                                global_state
                                    .struct_operation_map
                                    .get_mut(&(*msid, *sid))
                                    .unwrap()
                                    .insert(field.get_id(), merged);
                            }
                        }
                    }
                    GetField(msid, sid, _, offset) | BorrowField(msid, sid, _, offset) => {
                        let dests_oper = global_state
                            .get_temp_index_oper(cur_mid, cur_fid, dests[0], baseline_flag)
                            .unwrap();
                        let field_oper = global_state
                            .struct_operation_map
                            .get(&(*msid, *sid))
                            .unwrap()
                            .get(
                                &self
                                    .func_target
                                    .func_env
                                    .module_env
                                    .get_struct(*sid)
                                    .get_field_by_offset(*offset)
                                    .get_id(),
                            )
                            .unwrap();

                        if self.check_conflict(dests_oper, field_oper) {
                            self.func_target
                                .func_env
                                .module_env
                                .env
                                .error(&self.func_target.get_bytecode_loc(*id), CONFLICT_ERROR_MSG);
                        } else {
                            let merged_oper = dests_oper.merge(field_oper);
                            if merged_oper != *field_oper || merged_oper != *dests_oper {
                                state.changed = true;
                            }
                            *global_state
                                .get_mut_temp_index_oper(cur_mid, cur_fid, dests[0], baseline_flag)
                                .unwrap() = merged_oper;
                            global_state
                                .struct_operation_map
                                .get_mut(&(*msid, *sid))
                                .unwrap()
                                .insert(
                                    self.func_target
                                        .func_env
                                        .module_env
                                        .get_struct(*sid)
                                        .get_field_by_offset(*offset)
                                        .get_id(),
                                    merged_oper,
                                );
                        }
                    }
                    Function(msid, fsid, _) => {
                        let module_env = &self.func_target.global_env().get_module(*msid);
                        // Vector functions are handled separately
                        if !module_env.is_std_vector() && !module_env.is_table() {
                            for (i, src) in srcs.iter().enumerate() {
                                let cur_oper = global_state
                                    .get_temp_index_oper(cur_mid, cur_fid, *src, baseline_flag)
                                    .unwrap();
                                let callee_oper = global_state
                                    .get_temp_index_oper(*msid, *fsid, i, true)
                                    .unwrap();

                                if self.check_conflict(cur_oper, callee_oper) {
                                    self.func_target.func_env.module_env.env.error(
                                        &self.func_target.get_bytecode_loc(*id),
                                        CONFLICT_ERROR_MSG,
                                    );
                                } else {
                                    let merged = cur_oper.merge(callee_oper);
                                    if merged != *cur_oper || merged != *callee_oper {
                                        state.changed = true;
                                    }
                                    *global_state
                                        .get_mut_temp_index_oper(
                                            cur_mid,
                                            cur_fid,
                                            *src,
                                            baseline_flag,
                                        )
                                        .unwrap() = merged;
                                    *global_state
                                        .get_mut_temp_index_oper(*msid, *fsid, i, true)
                                        .unwrap() = merged;
                                }
                            }
                            for (i, dest) in dests.iter().enumerate() {
                                let cur_oper = global_state
                                    .get_temp_index_oper(cur_mid, cur_fid, *dest, baseline_flag)
                                    .unwrap();
                                let callee_oper = global_state
                                    .get_ret_map()
                                    .get(&(*msid, *fsid))
                                    .unwrap()
                                    .get(&i)
                                    .unwrap();
                                if self.check_conflict(cur_oper, callee_oper) {
                                    self.func_target.func_env.module_env.env.error(
                                        &self.func_target.get_bytecode_loc(*id),
                                        CONFLICT_ERROR_MSG,
                                    );
                                } else {
                                    let merged = cur_oper.merge(callee_oper);
                                    if merged != *cur_oper || merged != *callee_oper {
                                        state.changed = true;
                                    }
                                    *global_state
                                        .get_mut_temp_index_oper(
                                            cur_mid,
                                            cur_fid,
                                            *dest,
                                            baseline_flag,
                                        )
                                        .unwrap() = merged;
                                    global_state
                                        .get_mut_ret_map()
                                        .get_mut(&(*msid, *fsid))
                                        .unwrap()
                                        .insert(i, merged);
                                }
                            }
                        } else {
                            let callee = module_env.get_function(*fsid);
                            let callee_name = callee.get_name_str();
                            let check_and_update_bitwise =
                                |idx: &TempIndex,
                                 global_state: &mut GlobalNumberOperationState,
                                 state: &mut NumberOperationState| {
                                    let cur_oper = global_state
                                        .get_temp_index_oper(cur_mid, cur_fid, *idx, baseline_flag)
                                        .unwrap();

                                    if self.check_conflict(cur_oper, &Bitwise) {
                                        self.func_target.func_env.module_env.env.error(
                                            &self.func_target.get_bytecode_loc(*id),
                                            CONFLICT_ERROR_MSG,
                                        );
                                    } else if *cur_oper != Bitwise {
                                        state.changed = true;
                                        *global_state
                                            .get_mut_temp_index_oper(
                                                cur_mid,
                                                cur_fid,
                                                *idx,
                                                baseline_flag,
                                            )
                                            .unwrap() = Bitwise;
                                    }
                                };
                            if !srcs.is_empty() {
                                // First element
                                let first_oper = global_state
                                    .get_temp_index_oper(cur_mid, cur_fid, srcs[0], baseline_flag)
                                    .unwrap();
                                // Bitwise is specified explicitly in the fun or struct spec
                                if vector_table_funs_name_propogate_to_dest(&callee_name) {
                                    if *first_oper == Bitwise {
                                        // Do not consider the method remove_return_key where the first return value is k
                                        for dest in dests.iter() {
                                            check_and_update_bitwise(
                                                dest,
                                                &mut global_state,
                                                state,
                                            );
                                        }
                                    }
                                } else {
                                    let mut second_oper = first_oper;
                                    let mut src_idx = 0;
                                    if module_env.is_std_vector()
                                        && vector_funs_name_propogate_to_srcs(&callee_name)
                                    {
                                        assert!(srcs.len() > 1);
                                        second_oper = global_state
                                            .get_temp_index_oper(
                                                cur_mid,
                                                cur_fid,
                                                srcs[1],
                                                baseline_flag,
                                            )
                                            .unwrap();
                                        src_idx = 1;
                                    } else if table_funs_name_propogate_to_srcs(&callee_name) {
                                        assert!(srcs.len() > 2);
                                        second_oper = global_state
                                            .get_temp_index_oper(
                                                cur_mid,
                                                cur_fid,
                                                srcs[2],
                                                baseline_flag,
                                            )
                                            .unwrap();
                                        src_idx = 2;
                                    }
                                    if *first_oper == Bitwise || *second_oper == Bitwise {
                                        check_and_update_bitwise(
                                            &srcs[0],
                                            &mut global_state,
                                            state,
                                        );
                                        check_and_update_bitwise(
                                            &srcs[src_idx],
                                            &mut global_state,
                                            state,
                                        );
                                    }
                                }
                            } // empty, do nothing
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        self.func_target.global_env().set_extension(global_state);
    }
}

impl<'a> DataflowAnalysis for NumberOperationAnalysis<'a> {}

impl AbstractDomain for NumberOperationState {
    fn join(&mut self, other: &Self) -> JoinResult {
        let mut result = JoinResult::Unchanged;
        self.changed = false;
        if other.changed {
            result = JoinResult::Changed;
        }
        result
    }
}
