// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This file defines types, data structures and corresponding functions to
//! mark the operation (arithmetic or bitwise) that a variable or a field involves,
//! which will be used later when the correct number type (int or bv<N>) in the boogie program

use itertools::Itertools;
use move_model::{
    ast::{PropertyValue, TempIndex, Value},
    model::{FieldId, FunId, FunctionEnv, ModuleId, NodeId, SpecFunId, StructEnv, StructId},
    pragmas::{BV_PARAM_PROP, BV_RET_PROP},
    ty::Type,
};
use std::{collections::BTreeMap, str};

static PARSING_ERROR: &str = "error happened when parsing the bv pragma";

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub enum NumOperation {
    /// Default value, not involved in arithmetic or bitwise operations
    #[default]
    Bottom,
    /// Involved in arithmetic operations
    Arithmetic,
    /// Involved in bitwise operations
    Bitwise,
}

impl NumOperation {
    /// Check whether two operations are conflicting
    pub fn conflict(&self, other: &NumOperation) -> bool {
        use NumOperation::*;
        (*self == Arithmetic && *other == Bitwise) || (*self == Bitwise && *other == Arithmetic)
    }

    /// Return the operation according to the partial order in NumOperation
    pub fn merge(&self, other: &NumOperation) -> NumOperation {
        if self.ge(other) {
            *self
        } else {
            *other
        }
    }
}

// NumOperation of a variable
pub type OperationMap = BTreeMap<usize, NumOperation>;
pub type ExpMap = BTreeMap<NodeId, NumOperation>;
pub type OperationVec = Vec<NumOperation>;
// NumOperation of a field
pub type StructFieldOperationMap = BTreeMap<FieldId, NumOperation>;
pub type FuncOperationMap = BTreeMap<(ModuleId, FunId), OperationMap>;
pub type SpecFuncOperationMap = BTreeMap<(ModuleId, SpecFunId), (OperationVec, OperationVec)>;
pub type StructOperationMap = BTreeMap<(ModuleId, StructId), StructFieldOperationMap>;

#[derive(Default, Debug, Clone, Eq, PartialEq, PartialOrd)]
pub struct GlobalNumberOperationState {
    // TODO(tengzhang): spec funs and spec vars need to be handled here
    // Each TempIndex for parameters appearing the function has a corresponding NumOperation
    temp_index_operation_map: FuncOperationMap,
    // Each return value in the function has a corresponding NumOperation
    ret_operation_map: FuncOperationMap,
    // Each TempIndex for locals appearing the function has a corresponding NumOperation
    local_oper: FuncOperationMap,
    // local_oper, but for baseline
    local_oper_baseline: FuncOperationMap,
    // Each node id appearing the function has a corresponding NumOperation
    pub exp_operation_map: ExpMap,
    // NumberOperation state for spec functions
    pub spec_fun_operation_map: SpecFuncOperationMap,
    // Each field in the struct has a corresponding NumOperation
    pub struct_operation_map: StructOperationMap,
}

impl GlobalNumberOperationState {
    /// Parse pragma bv=b"..." and pragma bv_ret=b"...", the result is a list of position (starting from 0)
    /// in the argument list of the function
    /// or a struct definition
    fn extract_bv_vars(bv_temp_opt: Option<&PropertyValue>) -> Vec<usize> {
        let mut bv_temp_vec = vec![];
        if let Some(PropertyValue::Value(Value::ByteArray(arr))) = bv_temp_opt {
            let param_str = str::from_utf8(arr).expect(PARSING_ERROR);
            let idx_vec = param_str
                .split(',')
                .map(|s| s.trim().parse::<usize>().expect(PARSING_ERROR))
                .collect_vec();
            bv_temp_vec = idx_vec;
        }
        bv_temp_vec
    }

    pub fn get_ret_map(&self) -> &FuncOperationMap {
        &self.ret_operation_map
    }

    pub fn get_mut_ret_map(&mut self) -> &mut FuncOperationMap {
        &mut self.ret_operation_map
    }

    pub fn get_non_param_local_map(
        &self,
        mid: ModuleId,
        fid: FunId,
        baseline_flag: bool,
    ) -> &OperationMap {
        if baseline_flag {
            self.local_oper_baseline.get(&(mid, fid)).unwrap()
        } else {
            self.local_oper.get(&(mid, fid)).unwrap()
        }
    }

    pub fn get_mut_non_param_local_map(
        &mut self,
        mid: ModuleId,
        fid: FunId,
        baseline_flag: bool,
    ) -> &mut OperationMap {
        if baseline_flag {
            self.local_oper_baseline.get_mut(&(mid, fid)).unwrap()
        } else {
            self.local_oper.get_mut(&(mid, fid)).unwrap()
        }
    }

    pub fn get_temp_index_oper(
        &self,
        mid: ModuleId,
        fid: FunId,
        idx: TempIndex,
        baseline_flag: bool,
    ) -> Option<&NumOperation> {
        if baseline_flag {
            if self
                .local_oper_baseline
                .get(&(mid, fid))
                .unwrap()
                .contains_key(&idx)
            {
                self.local_oper_baseline.get(&(mid, fid)).unwrap().get(&idx)
            } else {
                self.temp_index_operation_map
                    .get(&(mid, fid))
                    .unwrap()
                    .get(&idx)
            }
        } else if self.local_oper.get(&(mid, fid)).unwrap().contains_key(&idx) {
            self.local_oper.get(&(mid, fid)).unwrap().get(&idx)
        } else {
            self.temp_index_operation_map
                .get(&(mid, fid))
                .unwrap()
                .get(&idx)
        }
    }

    pub fn get_mut_temp_index_oper(
        &mut self,
        mid: ModuleId,
        fid: FunId,
        idx: TempIndex,
        baseline_flag: bool,
    ) -> Option<&mut NumOperation> {
        if baseline_flag {
            if self
                .local_oper_baseline
                .get(&(mid, fid))
                .unwrap()
                .contains_key(&idx)
            {
                self.local_oper_baseline
                    .get_mut(&(mid, fid))
                    .unwrap()
                    .get_mut(&idx)
            } else {
                self.temp_index_operation_map
                    .get_mut(&(mid, fid))
                    .unwrap()
                    .get_mut(&idx)
            }
        } else if self.local_oper.get(&(mid, fid)).unwrap().contains_key(&idx) {
            self.local_oper.get_mut(&(mid, fid)).unwrap().get_mut(&idx)
        } else {
            self.temp_index_operation_map
                .get_mut(&(mid, fid))
                .unwrap()
                .get_mut(&idx)
        }
    }

    /// Create the initial NumberOperationState
    pub fn create_initial_func_oper_state(&mut self, func_env: &FunctionEnv) {
        use NumOperation::*;

        // Obtain positions that are marked as Bitwise by analyzing the pragma
        let para_sym = &func_env.module_env.env.symbol_pool().make(BV_PARAM_PROP);
        let ret_sym = &func_env.module_env.env.symbol_pool().make(BV_RET_PROP);
        let number_param_property = func_env.get_spec().properties.get(para_sym);
        let number_ret_property = func_env.get_spec().properties.get(ret_sym);
        let para_idx_vec = Self::extract_bv_vars(number_param_property);
        let ret_idx_vec = Self::extract_bv_vars(number_ret_property);

        let mid = func_env.module_env.get_id();
        let fid = func_env.get_id();
        let mut default_map = BTreeMap::new();
        let mut default_ret_operation_map = BTreeMap::new();

        // Set initial state for tempIndex
        for i in 0..func_env.get_parameter_count() {
            if para_idx_vec.contains(&i) {
                default_map.insert(i, Bitwise);
            } else {
                // If not appearing in the pragma, mark it as Arithmetic or Bottom
                // Similar logic when populating ret_operation_map below
                let local_ty = func_env.get_local_type(i);
                let arith_flag = if let Type::Reference(_, tr) = local_ty {
                    tr.is_number()
                } else if let Type::Vector(tr) = local_ty {
                    tr.is_number()
                } else {
                    local_ty.is_number()
                };
                if arith_flag {
                    default_map.insert(i, Arithmetic);
                } else {
                    default_map.insert(i, Bottom);
                }
            }
        }

        // Set initial state for ret_operation_map
        for i in 0..func_env.get_return_count() {
            if ret_idx_vec.contains(&i) {
                default_ret_operation_map.insert(i, Bitwise);
            } else {
                let ret_ty = func_env.get_return_type(i);
                let arith_flag = if let Type::Reference(_, tr) = ret_ty {
                    tr.is_number()
                } else if let Type::Vector(tr) = ret_ty {
                    tr.is_number()
                } else {
                    ret_ty.is_number()
                };
                if arith_flag {
                    default_ret_operation_map.insert(i, Arithmetic);
                } else {
                    default_ret_operation_map.insert(i, Bottom);
                }
            }
        }

        self.temp_index_operation_map
            .insert((mid, fid), default_map);
        self.local_oper_baseline.insert((mid, fid), BTreeMap::new());
        self.local_oper.insert((mid, fid), BTreeMap::new());
        self.ret_operation_map
            .insert((mid, fid), default_ret_operation_map);
    }

    /// Populate default state for struct operation map
    pub fn create_initial_struct_oper_state(&mut self, struct_env: &StructEnv) {
        use NumOperation::*;

        // Obtain positions that are marked as Bitwise by analyzing the pragma
        let para_sym = &struct_env.module_env.env.symbol_pool().make(BV_PARAM_PROP);
        let bv_struct_opt = struct_env.get_spec().properties.get(para_sym);
        let field_idx_vec = Self::extract_bv_vars(bv_struct_opt);

        let mid = struct_env.module_env.get_id();
        let sid = struct_env.get_id();
        let struct_env = struct_env.module_env.env.get_module(mid).into_struct(sid);
        let mut field_oper_map = BTreeMap::new();

        for (i, field) in struct_env.get_fields().enumerate() {
            if field_idx_vec.contains(&i) {
                field_oper_map.insert(field.get_id(), Bitwise);
            } else {
                let field_ty = field.get_type();
                let arith_flag = if let Type::Reference(_, tr) = field_ty {
                    tr.is_number()
                } else if let Type::Vector(tr) = field_ty {
                    tr.is_number()
                } else {
                    field_ty.is_number()
                };
                if arith_flag {
                    field_oper_map.insert(field.get_id(), Arithmetic);
                } else {
                    field_oper_map.insert(field.get_id(), Bottom);
                }
            }
        }
        self.struct_operation_map.insert((mid, sid), field_oper_map);
    }

    /// Updates the number operation for the given node id.
    pub fn update_node_oper(
        &mut self,
        node_id: NodeId,
        num_oper: NumOperation,
        allow: bool,
    ) -> bool {
        let mods = &mut self.exp_operation_map;
        let oper = mods.get_mut(&node_id).expect("node exist");
        if !allow && oper.conflict(&num_oper) {
            false
        } else {
            *oper = num_oper;
            true
        }
    }

    /// Gets the number operation of the given node.
    pub fn get_node_num_oper(&self, node_id: NodeId) -> NumOperation {
        self.get_node_num_oper_opt(node_id)
            .expect("node number oper defined")
    }

    /// Gets the number operation of the given node, if available.
    pub fn get_node_num_oper_opt(&self, node_id: NodeId) -> Option<NumOperation> {
        self.exp_operation_map.get(&node_id).copied()
    }

    pub fn update_spec_ret(&mut self, mid: &ModuleId, fid: &SpecFunId, oper: NumOperation) {
        let ret_num_oper_vec = &mut self
            .spec_fun_operation_map
            .get_mut(&(*mid, *fid))
            .unwrap()
            .1;
        ret_num_oper_vec[0] = oper;
    }
}
