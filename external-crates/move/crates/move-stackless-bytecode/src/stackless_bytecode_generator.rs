// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    function_target::FunctionData,
    stackless_bytecode::{
        AssignKind, AttrId,
        Bytecode::{self},
        Constant, Label, Operation,
    },
};
use itertools::Itertools;
use move_binary_format::file_format::{
    Bytecode as MoveBytecode, CodeOffset, CompiledModule, FieldHandleIndex, JumpTableInner,
    SignatureIndex,
};
use move_core_types::{
    language_storage::{self, CORE_CODE_ADDRESS},
    runtime_value::MoveValue,
};
use move_model::{
    ast::TempIndex,
    model::{DatatypeId, FunId, FunctionEnv, Loc, ModuleId, RefType},
    ty::{PrimitiveType, Type},
};
use num::BigUint;
use std::{
    collections::{BTreeMap, BTreeSet},
    convert::TryInto,
    matches,
};

pub struct StacklessBytecodeGenerator<'a> {
    func_env: &'a FunctionEnv<'a>,
    module: &'a CompiledModule,
    temp_count: usize,
    temp_stack: Vec<usize>,
    local_types: Vec<Type>,
    code: Vec<Bytecode>,
    location_table: BTreeMap<AttrId, Loc>,
    loop_invariants: BTreeSet<AttrId>,
    fallthrough_labels: BTreeSet<Label>,
}

impl<'a> StacklessBytecodeGenerator<'a> {
    pub fn new(func_env: &'a FunctionEnv<'a>) -> Self {
        let local_types = (0..func_env.get_local_count())
            .map(|i| func_env.get_local_type(i))
            .collect_vec();
        StacklessBytecodeGenerator {
            func_env,
            module: func_env.module_env.get_verified_module(),
            temp_count: local_types.len(),
            temp_stack: vec![],
            local_types,
            code: vec![],
            location_table: BTreeMap::new(),
            loop_invariants: BTreeSet::new(),
            fallthrough_labels: BTreeSet::new(),
        }
    }

    pub fn generate_function(mut self) -> FunctionData {
        let original_code = self.func_env.get_bytecode();
        let mut label_map = BTreeMap::new();

        // Generate labels.
        for (pos, bytecode) in original_code.iter().enumerate() {
            if let MoveBytecode::BrTrue(code_offset)
            | MoveBytecode::BrFalse(code_offset)
            | MoveBytecode::Branch(code_offset) = bytecode
            {
                let offs = *code_offset as CodeOffset;
                if label_map.get(&offs).is_none() {
                    let label = Label::new(label_map.len());
                    label_map.insert(offs, label);
                }
            }
            if let MoveBytecode::BrTrue(_) | MoveBytecode::BrFalse(_) = bytecode {
                let next_offs = (pos + 1) as CodeOffset;
                if label_map.get(&next_offs).is_none() {
                    let fall_through_label = Label::new(label_map.len());
                    label_map.insert(next_offs, fall_through_label);
                    self.fallthrough_labels.insert(fall_through_label);
                }
            };
        }

        // Generate bytecode.
        for (code_offset, bytecode) in original_code.iter().enumerate() {
            self.generate_bytecode(bytecode, code_offset as CodeOffset, &label_map);
        }

        // Eliminate fall-through for non-branching instructions
        let code = std::mem::take(&mut self.code);
        for bytecode in code.into_iter() {
            if let Bytecode::Label(attr_id, label) = bytecode {
                if !self.code.is_empty() && !self.code[self.code.len() - 1].is_branch() {
                    self.code.push(Bytecode::Jump(attr_id, label));
                }
            }
            self.code.push(bytecode);
        }

        let Self {
            func_env,
            module: _,
            temp_count: _,
            temp_stack: _,
            local_types,
            code,
            location_table,
            loop_invariants,
            ..
        } = self;

        FunctionData::new(
            func_env,
            code,
            local_types,
            func_env.get_return_types(),
            location_table,
            func_env.get_acquires_global_resources(),
            loop_invariants,
        )
    }

    /// Create a new attribute id and populate location table.
    fn new_loc_attr(&mut self, code_offset: CodeOffset) -> AttrId {
        let loc = self.func_env.get_bytecode_loc(code_offset);
        let attr = AttrId::new(self.location_table.len());
        self.location_table.insert(attr, loc);
        attr
    }

    fn get_field_info(&self, field_handle_index: FieldHandleIndex) -> (DatatypeId, usize, Type) {
        let field_handle = self.module.field_handle_at(field_handle_index);
        let struct_id = self.func_env.module_env.get_struct_id(field_handle.owner);
        let struct_env = self.func_env.module_env.get_struct(struct_id);
        let field_env = struct_env.get_field_by_offset(field_handle.field as usize);
        (struct_id, field_handle.field as usize, field_env.get_type())
    }

    fn get_type_params(&self, type_params_index: SignatureIndex) -> Vec<Type> {
        self.func_env
            .module_env
            .get_type_actuals(Some(type_params_index))
    }

    #[allow(clippy::cognitive_complexity)]
    pub fn generate_bytecode(
        &mut self,
        bytecode: &MoveBytecode,
        code_offset: CodeOffset,
        label_map: &BTreeMap<CodeOffset, Label>,
    ) {
        // Add label if defined at this code offset.
        if let Some(label) = label_map.get(&code_offset) {
            let label_attr_id = self.new_loc_attr(code_offset);
            self.code.push(Bytecode::Label(label_attr_id, *label));
        }

        let attr_id = self.new_loc_attr(code_offset);

        let global_env = self.func_env.module_env.env;
        let mut vec_module_id_opt: Option<ModuleId> = None;
        let mut mk_vec_function_operation = |name: &str, tys: Vec<Type>| -> Operation {
            let vec_module_env = vec_module_id_opt.get_or_insert_with(|| {
                let vec_module = global_env.to_module_name(&language_storage::ModuleId::new(
                    CORE_CODE_ADDRESS,
                    move_core_types::identifier::Identifier::new("vector").unwrap(),
                ));
                global_env
                    .find_module(&vec_module)
                    .expect("unexpected reference to module not found in global env")
                    .get_id()
            });

            let vec_fun = FunId::new(global_env.symbol_pool().make(name));
            Operation::Function(*vec_module_env, vec_fun, tys)
        };

        let mk_call = |op: Operation, dsts: Vec<usize>, srcs: Vec<usize>| -> Bytecode {
            Bytecode::Call(attr_id, dsts, op, srcs, None)
        };
        let mk_unary = |op: Operation, dst: usize, src: usize| -> Bytecode {
            Bytecode::Call(attr_id, vec![dst], op, vec![src], None)
        };
        let mk_binary = |op: Operation, dst: usize, src1: usize, src2: usize| -> Bytecode {
            Bytecode::Call(attr_id, vec![dst], op, vec![src1, src2], None)
        };

        match bytecode {
            MoveBytecode::Pop => {
                let temp_index = self.temp_stack.pop().unwrap();
                self.code
                    .push(mk_call(Operation::Destroy, vec![], vec![temp_index]));
            }
            MoveBytecode::BrTrue(target) => {
                let temp_index = self.temp_stack.pop().unwrap();
                self.code.push(Bytecode::Branch(
                    attr_id,
                    *label_map.get(target).unwrap(),
                    *label_map.get(&(code_offset + 1)).unwrap(),
                    temp_index,
                ));
            }

            MoveBytecode::BrFalse(target) => {
                let temp_index = self.temp_stack.pop().unwrap();
                self.code.push(Bytecode::Branch(
                    attr_id,
                    *label_map.get(&(code_offset + 1)).unwrap(),
                    *label_map.get(target).unwrap(),
                    temp_index,
                ));
            }

            MoveBytecode::Abort => {
                let error_code_index = self.temp_stack.pop().unwrap();
                self.code.push(Bytecode::Abort(attr_id, error_code_index));
            }

            MoveBytecode::StLoc(idx) => {
                let operand_index = self.temp_stack.pop().unwrap();
                self.code.push(Bytecode::Assign(
                    attr_id,
                    *idx as TempIndex,
                    operand_index,
                    AssignKind::Store,
                ));
            }

            MoveBytecode::Ret => {
                let mut return_temps = vec![];
                for _ in 0..self.func_env.get_return_count() {
                    let return_temp_index = self.temp_stack.pop().unwrap();
                    return_temps.push(return_temp_index);
                }
                return_temps.reverse();
                self.code.push(Bytecode::Ret(attr_id, return_temps));
            }

            MoveBytecode::Branch(target) => {
                // Attempt to eliminate the common pattern `if c goto L1 else L2; L2: goto L3`
                // and replace it with `if c goto L1 else L3`, provided L2 is a fall-through
                // label, i.e. not referenced from elsewhere.
                let target_label = *label_map.get(target).unwrap();
                let at = self.code.len();
                let rewritten = if at >= 2 {
                    match (&self.code[at - 2], &self.code[at - 1]) {
                        (
                            Bytecode::Branch(attr, if_true, if_false, c),
                            Bytecode::Label(_, cont),
                        ) if self.fallthrough_labels.contains(cont) && if_false == cont => {
                            let bc = Bytecode::Branch(*attr, *if_true, target_label, *c);
                            self.code.pop();
                            self.code.pop();
                            self.code.push(bc);
                            true
                        }
                        _ => false,
                    }
                } else {
                    false
                };
                if !rewritten {
                    self.code.push(Bytecode::Jump(attr_id, target_label));
                }
            }

            MoveBytecode::FreezeRef => {
                let mutable_ref_index = self.temp_stack.pop().unwrap();
                let mutable_ref_sig = self.local_types[mutable_ref_index].clone();
                if let Type::Reference(is_mut, signature) = mutable_ref_sig {
                    if is_mut {
                        let immutable_ref_index = self.temp_count;
                        self.temp_stack.push(immutable_ref_index);
                        self.local_types.push(Type::Reference(false, signature));
                        self.code.push(mk_call(
                            Operation::FreezeRef,
                            vec![immutable_ref_index],
                            vec![mutable_ref_index],
                        ));
                        self.temp_count += 1;
                    }
                }
            }

            MoveBytecode::ImmBorrowField(field_handle_index)
            | MoveBytecode::MutBorrowField(field_handle_index) => {
                let struct_ref_index = self.temp_stack.pop().unwrap();
                let (struct_id, field_offset, field_type) =
                    self.get_field_info(*field_handle_index);
                let field_ref_index = self.temp_count;
                self.temp_stack.push(field_ref_index);

                self.code.push(mk_call(
                    Operation::BorrowField(
                        self.func_env.module_env.get_id(),
                        struct_id,
                        vec![],
                        field_offset,
                    ),
                    vec![field_ref_index],
                    vec![struct_ref_index],
                ));
                self.temp_count += 1;
                let is_mut = matches!(bytecode, MoveBytecode::MutBorrowField(..));
                self.local_types
                    .push(Type::Reference(is_mut, Box::new(field_type)));
            }

            MoveBytecode::ImmBorrowFieldGeneric(field_inst_index)
            | MoveBytecode::MutBorrowFieldGeneric(field_inst_index) => {
                let field_inst = self.module.field_instantiation_at(*field_inst_index);
                let struct_ref_index = self.temp_stack.pop().unwrap();
                let (struct_id, field_offset, base_field_type) =
                    self.get_field_info(field_inst.handle);
                let actuals = self.get_type_params(field_inst.type_parameters);
                let field_type = base_field_type.instantiate(&actuals);
                let field_ref_index = self.temp_count;
                self.temp_stack.push(field_ref_index);

                self.code.push(mk_call(
                    Operation::BorrowField(
                        self.func_env.module_env.get_id(),
                        struct_id,
                        actuals,
                        field_offset,
                    ),
                    vec![field_ref_index],
                    vec![struct_ref_index],
                ));
                self.temp_count += 1;
                let is_mut = matches!(bytecode, MoveBytecode::MutBorrowFieldGeneric(..));
                self.local_types
                    .push(Type::Reference(is_mut, Box::new(field_type)));
            }

            MoveBytecode::LdU8(number) => {
                let temp_index = self.temp_count;
                self.temp_stack.push(temp_index);
                self.local_types.push(Type::Primitive(PrimitiveType::U8));
                self.code
                    .push(Bytecode::Load(attr_id, temp_index, Constant::U8(*number)));
                self.temp_count += 1;
            }

            MoveBytecode::LdU16(number) => {
                let temp_index = self.temp_count;
                self.temp_stack.push(temp_index);
                self.local_types.push(Type::Primitive(PrimitiveType::U16));
                self.code
                    .push(Bytecode::Load(attr_id, temp_index, Constant::U16(*number)));
                self.temp_count += 1;
            }

            MoveBytecode::LdU32(number) => {
                let temp_index = self.temp_count;
                self.temp_stack.push(temp_index);
                self.local_types.push(Type::Primitive(PrimitiveType::U32));
                self.code
                    .push(Bytecode::Load(attr_id, temp_index, Constant::U32(*number)));
                self.temp_count += 1;
            }

            MoveBytecode::LdU64(number) => {
                let temp_index = self.temp_count;
                self.temp_stack.push(temp_index);
                self.local_types.push(Type::Primitive(PrimitiveType::U64));
                self.code
                    .push(Bytecode::Load(attr_id, temp_index, Constant::U64(*number)));
                self.temp_count += 1;
            }

            MoveBytecode::LdU256(number) => {
                let temp_index = self.temp_count;
                self.temp_stack.push(temp_index);
                self.local_types.push(Type::Primitive(PrimitiveType::U256));
                self.code.push(Bytecode::Load(
                    attr_id,
                    temp_index,
                    Constant::from(&**number),
                ));
                self.temp_count += 1;
            }

            MoveBytecode::LdU128(number) => {
                let temp_index = self.temp_count;
                self.temp_stack.push(temp_index);
                self.local_types.push(Type::Primitive(PrimitiveType::U128));
                self.code.push(Bytecode::Load(
                    attr_id,
                    temp_index,
                    Constant::U128(**number),
                ));
                self.temp_count += 1;
            }

            MoveBytecode::CastU8 => {
                let operand_index = self.temp_stack.pop().unwrap();
                let temp_index = self.temp_count;
                self.temp_stack.push(temp_index);
                self.local_types.push(Type::Primitive(PrimitiveType::U8));
                self.code
                    .push(mk_unary(Operation::CastU8, temp_index, operand_index));
                self.temp_count += 1;
            }

            MoveBytecode::CastU16 => {
                let operand_index = self.temp_stack.pop().unwrap();
                let temp_index = self.temp_count;
                self.temp_stack.push(temp_index);
                self.local_types.push(Type::Primitive(PrimitiveType::U16));
                self.code
                    .push(mk_unary(Operation::CastU16, temp_index, operand_index));
                self.temp_count += 1;
            }

            MoveBytecode::CastU32 => {
                let operand_index = self.temp_stack.pop().unwrap();
                let temp_index = self.temp_count;
                self.temp_stack.push(temp_index);
                self.local_types.push(Type::Primitive(PrimitiveType::U32));
                self.code
                    .push(mk_unary(Operation::CastU32, temp_index, operand_index));
                self.temp_count += 1;
            }

            MoveBytecode::CastU64 => {
                let operand_index = self.temp_stack.pop().unwrap();
                let temp_index = self.temp_count;
                self.temp_stack.push(temp_index);
                self.local_types.push(Type::Primitive(PrimitiveType::U64));
                self.code
                    .push(mk_unary(Operation::CastU64, temp_index, operand_index));
                self.temp_count += 1;
            }

            MoveBytecode::CastU128 => {
                let operand_index = self.temp_stack.pop().unwrap();
                let temp_index = self.temp_count;
                self.temp_stack.push(temp_index);
                self.local_types.push(Type::Primitive(PrimitiveType::U128));
                self.code
                    .push(mk_unary(Operation::CastU128, temp_index, operand_index));
                self.temp_count += 1;
            }

            MoveBytecode::CastU256 => {
                let operand_index = self.temp_stack.pop().unwrap();
                let temp_index = self.temp_count;
                self.temp_stack.push(temp_index);
                self.local_types.push(Type::Primitive(PrimitiveType::U256));
                self.code
                    .push(mk_unary(Operation::CastU256, temp_index, operand_index));
                self.temp_count += 1;
            }

            MoveBytecode::LdConst(idx) => {
                let temp_index = self.temp_count;
                self.temp_stack.push(temp_index);
                let constant = self.func_env.module_env.get_constant(*idx);
                let ty = self
                    .func_env
                    .module_env
                    .globalize_signature(&constant.type_);
                let value = Self::translate_value(
                    &ty,
                    &self.func_env.module_env.get_constant_value(constant),
                );
                self.local_types.push(ty);
                self.code.push(Bytecode::Load(attr_id, temp_index, value));
                self.temp_count += 1;
            }

            MoveBytecode::LdTrue => {
                let temp_index = self.temp_count;
                self.temp_stack.push(temp_index);
                self.local_types.push(Type::Primitive(PrimitiveType::Bool));
                self.code
                    .push(Bytecode::Load(attr_id, temp_index, Constant::Bool(true)));
                self.temp_count += 1;
            }

            MoveBytecode::LdFalse => {
                let temp_index = self.temp_count;
                self.temp_stack.push(temp_index);
                self.local_types.push(Type::Primitive(PrimitiveType::Bool));
                self.code
                    .push(Bytecode::Load(attr_id, temp_index, Constant::Bool(false)));
                self.temp_count += 1;
            }

            MoveBytecode::CopyLoc(idx) => {
                let signature = self.func_env.get_local_type(*idx as usize);
                let temp_index = self.temp_count;
                self.temp_stack.push(temp_index);
                self.local_types.push(signature); // same type as the value copied
                self.code.push(Bytecode::Assign(
                    attr_id,
                    temp_index,
                    *idx as TempIndex,
                    AssignKind::Copy,
                ));
                self.temp_count += 1;
            }

            MoveBytecode::MoveLoc(idx) => {
                let signature = self.func_env.get_local_type(*idx as usize);
                let temp_index = self.temp_count;
                self.temp_stack.push(temp_index);
                self.local_types.push(signature); // same type as the value copied
                self.code.push(Bytecode::Assign(
                    attr_id,
                    temp_index,
                    *idx as TempIndex,
                    AssignKind::Move,
                ));
                self.temp_count += 1;
            }

            MoveBytecode::MutBorrowLoc(idx) => {
                let signature = self.func_env.get_local_type(*idx as usize);
                let temp_index = self.temp_count;
                self.temp_stack.push(temp_index);
                self.local_types
                    .push(Type::Reference(true, Box::new(signature)));
                self.code.push(mk_unary(
                    Operation::BorrowLoc,
                    temp_index,
                    *idx as TempIndex,
                ));
                self.temp_count += 1;
            }

            MoveBytecode::ImmBorrowLoc(idx) => {
                let signature = self.func_env.get_local_type(*idx as usize);
                let temp_index = self.temp_count;
                self.temp_stack.push(temp_index);
                self.local_types
                    .push(Type::Reference(false, Box::new(signature)));
                self.code.push(mk_unary(
                    Operation::BorrowLoc,
                    temp_index,
                    *idx as TempIndex,
                ));
                self.temp_count += 1;
            }

            MoveBytecode::Call(idx) => {
                let function_handle = self.module.function_handle_at(*idx);
                let parameters = self.module.signature_at(function_handle.parameters);
                let return_ = self.module.signature_at(function_handle.return_);

                let mut arg_temp_indices = vec![];
                let mut return_temp_indices = vec![];
                for _ in &parameters.0 {
                    let arg_temp_index = self.temp_stack.pop().unwrap();
                    arg_temp_indices.push(arg_temp_index);
                }
                for return_signature_token in &return_.0 {
                    let return_temp_index = self.temp_count;
                    let return_type = self
                        .func_env
                        .module_env
                        .globalize_signature(return_signature_token);
                    return_temp_indices.push(return_temp_index);
                    self.temp_stack.push(return_temp_index);
                    self.local_types.push(return_type);
                    self.temp_count += 1;
                }
                arg_temp_indices.reverse();
                let callee_env = self.func_env.module_env.get_used_function(*idx);
                self.code.push(mk_call(
                    Operation::Function(
                        callee_env.module_env.get_id(),
                        callee_env.get_id(),
                        vec![],
                    ),
                    return_temp_indices,
                    arg_temp_indices,
                ))
            }
            MoveBytecode::CallGeneric(idx) => {
                let func_instantiation = self.module.function_instantiation_at(*idx);

                let type_sigs = self.get_type_params(func_instantiation.type_parameters);
                let function_handle = self.module.function_handle_at(func_instantiation.handle);
                let parameters = self.module.signature_at(function_handle.parameters);
                let return_ = self.module.signature_at(function_handle.return_);

                let mut arg_temp_indices = vec![];
                let mut return_temp_indices = vec![];
                for _ in &parameters.0 {
                    let arg_temp_index = self.temp_stack.pop().unwrap();
                    arg_temp_indices.push(arg_temp_index);
                }
                for return_signature_token in &return_.0 {
                    let return_temp_index = self.temp_count;
                    // instantiate type parameters
                    let return_type = self
                        .func_env
                        .module_env
                        .globalize_signature(return_signature_token)
                        .instantiate(&type_sigs);
                    return_temp_indices.push(return_temp_index);
                    self.temp_stack.push(return_temp_index);
                    self.local_types.push(return_type);
                    self.temp_count += 1;
                }
                arg_temp_indices.reverse();
                let callee_env = self
                    .func_env
                    .module_env
                    .get_used_function(func_instantiation.handle);
                self.code.push(mk_call(
                    Operation::Function(
                        callee_env.module_env.get_id(),
                        callee_env.get_id(),
                        type_sigs,
                    ),
                    return_temp_indices,
                    arg_temp_indices,
                ))
            }

            MoveBytecode::Pack(idx) => {
                let struct_env = self.func_env.module_env.get_struct_by_def_idx(*idx);
                let mut field_temp_indices = vec![];
                let struct_temp_index = self.temp_count;
                for _ in struct_env.get_fields() {
                    let field_temp_index = self.temp_stack.pop().unwrap();
                    field_temp_indices.push(field_temp_index);
                }
                self.local_types.push(Type::Datatype(
                    struct_env.module_env.get_id(),
                    struct_env.get_id(),
                    vec![],
                ));
                self.temp_stack.push(struct_temp_index);
                field_temp_indices.reverse();
                self.code.push(mk_call(
                    Operation::Pack(struct_env.module_env.get_id(), struct_env.get_id(), vec![]),
                    vec![struct_temp_index],
                    field_temp_indices,
                ));
                self.temp_count += 1;
            }

            MoveBytecode::PackGeneric(idx) => {
                let struct_instantiation = self.module.struct_instantiation_at(*idx);
                let actuals = self.get_type_params(struct_instantiation.type_parameters);
                let struct_env = self
                    .func_env
                    .module_env
                    .get_struct_by_def_idx(struct_instantiation.def);
                let mut field_temp_indices = vec![];
                let struct_temp_index = self.temp_count;
                for _ in struct_env.get_fields() {
                    let field_temp_index = self.temp_stack.pop().unwrap();
                    field_temp_indices.push(field_temp_index);
                }
                self.local_types.push(Type::Datatype(
                    struct_env.module_env.get_id(),
                    struct_env.get_id(),
                    actuals.clone(),
                ));
                self.temp_stack.push(struct_temp_index);
                field_temp_indices.reverse();
                self.code.push(mk_call(
                    Operation::Pack(struct_env.module_env.get_id(), struct_env.get_id(), actuals),
                    vec![struct_temp_index],
                    field_temp_indices,
                ));
                self.temp_count += 1;
            }

            MoveBytecode::Unpack(idx) => {
                let struct_env = self.func_env.module_env.get_struct_by_def_idx(*idx);
                let mut field_temp_indices = vec![];
                let struct_temp_index = self.temp_stack.pop().unwrap();
                for field_env in struct_env.get_fields() {
                    let field_temp_index = self.temp_count;
                    field_temp_indices.push(field_temp_index);
                    self.temp_stack.push(field_temp_index);
                    self.local_types.push(field_env.get_type());
                    self.temp_count += 1;
                }
                self.code.push(mk_call(
                    Operation::Unpack(struct_env.module_env.get_id(), struct_env.get_id(), vec![]),
                    field_temp_indices,
                    vec![struct_temp_index],
                ));
            }

            MoveBytecode::UnpackGeneric(idx) => {
                let struct_instantiation = self.module.struct_instantiation_at(*idx);
                let actuals = self.get_type_params(struct_instantiation.type_parameters);
                let struct_env = self
                    .func_env
                    .module_env
                    .get_struct_by_def_idx(struct_instantiation.def);
                let mut field_temp_indices = vec![];
                let struct_temp_index = self.temp_stack.pop().unwrap();
                for field_env in struct_env.get_fields() {
                    let field_type = field_env.get_type().instantiate(&actuals);
                    let field_temp_index = self.temp_count;
                    field_temp_indices.push(field_temp_index);
                    self.temp_stack.push(field_temp_index);
                    self.local_types.push(field_type);
                    self.temp_count += 1;
                }
                self.code.push(mk_call(
                    Operation::Unpack(struct_env.module_env.get_id(), struct_env.get_id(), actuals),
                    field_temp_indices,
                    vec![struct_temp_index],
                ));
            }

            MoveBytecode::ReadRef => {
                let operand_index = self.temp_stack.pop().unwrap();
                let operand_sig = self.local_types[operand_index].clone();
                let temp_index = self.temp_count;
                if let Type::Reference(_, signature) = operand_sig {
                    self.local_types.push(*signature);
                }
                self.temp_stack.push(temp_index);
                self.temp_count += 1;
                self.code
                    .push(mk_unary(Operation::ReadRef, temp_index, operand_index));
            }

            MoveBytecode::WriteRef => {
                let ref_operand_index = self.temp_stack.pop().unwrap();
                let val_operand_index = self.temp_stack.pop().unwrap();
                self.code.push(mk_call(
                    Operation::WriteRef,
                    vec![],
                    vec![ref_operand_index, val_operand_index],
                ));
            }

            MoveBytecode::Add
            | MoveBytecode::Sub
            | MoveBytecode::Mul
            | MoveBytecode::Mod
            | MoveBytecode::Div
            | MoveBytecode::BitOr
            | MoveBytecode::BitAnd
            | MoveBytecode::Xor
            | MoveBytecode::Shl
            | MoveBytecode::Shr => {
                let operand2_index = self.temp_stack.pop().unwrap();
                let operand1_index = self.temp_stack.pop().unwrap();
                let operand_type = self.local_types[operand1_index].clone();
                let temp_index = self.temp_count;
                self.local_types.push(operand_type);
                self.temp_stack.push(temp_index);
                self.temp_count += 1;
                match bytecode {
                    MoveBytecode::Add => {
                        self.code.push(mk_binary(
                            Operation::Add,
                            temp_index,
                            operand1_index,
                            operand2_index,
                        ));
                    }
                    MoveBytecode::Sub => {
                        self.code.push(mk_binary(
                            Operation::Sub,
                            temp_index,
                            operand1_index,
                            operand2_index,
                        ));
                    }
                    MoveBytecode::Mul => {
                        self.code.push(mk_binary(
                            Operation::Mul,
                            temp_index,
                            operand1_index,
                            operand2_index,
                        ));
                    }
                    MoveBytecode::Mod => {
                        self.code.push(mk_binary(
                            Operation::Mod,
                            temp_index,
                            operand1_index,
                            operand2_index,
                        ));
                    }
                    MoveBytecode::Div => {
                        self.code.push(mk_binary(
                            Operation::Div,
                            temp_index,
                            operand1_index,
                            operand2_index,
                        ));
                    }
                    MoveBytecode::BitOr => {
                        self.code.push(mk_binary(
                            Operation::BitOr,
                            temp_index,
                            operand1_index,
                            operand2_index,
                        ));
                    }
                    MoveBytecode::BitAnd => {
                        self.code.push(mk_binary(
                            Operation::BitAnd,
                            temp_index,
                            operand1_index,
                            operand2_index,
                        ));
                    }
                    MoveBytecode::Xor => {
                        self.code.push(mk_binary(
                            Operation::Xor,
                            temp_index,
                            operand1_index,
                            operand2_index,
                        ));
                    }
                    MoveBytecode::Shl => {
                        self.code.push(mk_binary(
                            Operation::Shl,
                            temp_index,
                            operand1_index,
                            operand2_index,
                        ));
                    }
                    MoveBytecode::Shr => {
                        self.code.push(mk_binary(
                            Operation::Shr,
                            temp_index,
                            operand1_index,
                            operand2_index,
                        ));
                    }
                    _ => {}
                }
            }
            MoveBytecode::Or => {
                let operand2_index = self.temp_stack.pop().unwrap();
                let operand1_index = self.temp_stack.pop().unwrap();
                let temp_index = self.temp_count;
                self.local_types.push(Type::Primitive(PrimitiveType::Bool));
                self.temp_count += 1;
                self.temp_stack.push(temp_index);
                self.code.push(mk_binary(
                    Operation::Or,
                    temp_index,
                    operand1_index,
                    operand2_index,
                ));
            }

            MoveBytecode::And => {
                let operand2_index = self.temp_stack.pop().unwrap();
                let operand1_index = self.temp_stack.pop().unwrap();
                let temp_index = self.temp_count;
                self.local_types.push(Type::Primitive(PrimitiveType::Bool));
                self.temp_count += 1;
                self.temp_stack.push(temp_index);
                self.code.push(mk_binary(
                    Operation::And,
                    temp_index,
                    operand1_index,
                    operand2_index,
                ));
            }

            MoveBytecode::Not => {
                let operand_index = self.temp_stack.pop().unwrap();
                let temp_index = self.temp_count;
                self.local_types.push(Type::Primitive(PrimitiveType::Bool));
                self.temp_count += 1;
                self.temp_stack.push(temp_index);
                self.code
                    .push(mk_unary(Operation::Not, temp_index, operand_index));
            }
            MoveBytecode::Eq => {
                let operand2_index = self.temp_stack.pop().unwrap();
                let operand1_index = self.temp_stack.pop().unwrap();
                let temp_index = self.temp_count;
                self.local_types.push(Type::Primitive(PrimitiveType::Bool));
                self.temp_count += 1;
                self.temp_stack.push(temp_index);
                self.code.push(mk_binary(
                    Operation::Eq,
                    temp_index,
                    operand1_index,
                    operand2_index,
                ));
            }
            MoveBytecode::Neq => {
                let operand2_index = self.temp_stack.pop().unwrap();
                let operand1_index = self.temp_stack.pop().unwrap();
                let temp_index = self.temp_count;
                self.local_types.push(Type::Primitive(PrimitiveType::Bool));
                self.temp_count += 1;
                self.temp_stack.push(temp_index);
                self.code.push(mk_binary(
                    Operation::Neq,
                    temp_index,
                    operand1_index,
                    operand2_index,
                ));
            }
            MoveBytecode::Lt | MoveBytecode::Gt | MoveBytecode::Le | MoveBytecode::Ge => {
                let operand2_index = self.temp_stack.pop().unwrap();
                let operand1_index = self.temp_stack.pop().unwrap();
                let temp_index = self.temp_count;
                self.local_types.push(Type::Primitive(PrimitiveType::Bool));
                self.temp_count += 1;
                self.temp_stack.push(temp_index);
                match bytecode {
                    MoveBytecode::Lt => {
                        self.code.push(mk_binary(
                            Operation::Lt,
                            temp_index,
                            operand1_index,
                            operand2_index,
                        ));
                    }
                    MoveBytecode::Gt => {
                        self.code.push(mk_binary(
                            Operation::Gt,
                            temp_index,
                            operand1_index,
                            operand2_index,
                        ));
                    }
                    MoveBytecode::Le => {
                        self.code.push(mk_binary(
                            Operation::Le,
                            temp_index,
                            operand1_index,
                            operand2_index,
                        ));
                    }
                    MoveBytecode::Ge => {
                        self.code.push(mk_binary(
                            Operation::Ge,
                            temp_index,
                            operand1_index,
                            operand2_index,
                        ));
                    }
                    _ => {}
                }
            }
            MoveBytecode::ExistsDeprecated(struct_index) => {
                let operand_index = self.temp_stack.pop().unwrap();
                let temp_index = self.temp_count;
                self.local_types.push(Type::Primitive(PrimitiveType::Bool));
                self.temp_count += 1;
                self.temp_stack.push(temp_index);
                self.code.push(mk_unary(
                    Operation::Exists(
                        self.func_env.module_env.get_id(),
                        self.func_env.module_env.get_struct_id(*struct_index),
                        vec![],
                    ),
                    temp_index,
                    operand_index,
                ));
            }

            MoveBytecode::ExistsGenericDeprecated(idx) => {
                let struct_instantiation = self.module.struct_instantiation_at(*idx);
                let operand_index = self.temp_stack.pop().unwrap();
                let temp_index = self.temp_count;
                self.local_types.push(Type::Primitive(PrimitiveType::Bool));
                self.temp_count += 1;
                self.temp_stack.push(temp_index);
                self.code.push(mk_unary(
                    Operation::Exists(
                        self.func_env.module_env.get_id(),
                        self.func_env
                            .module_env
                            .get_struct_id(struct_instantiation.def),
                        self.get_type_params(struct_instantiation.type_parameters),
                    ),
                    temp_index,
                    operand_index,
                ));
            }

            MoveBytecode::MutBorrowGlobalDeprecated(idx)
            | MoveBytecode::ImmBorrowGlobalDeprecated(idx) => {
                let struct_env = self.func_env.module_env.get_struct_by_def_idx(*idx);
                let is_mut = matches!(bytecode, MoveBytecode::MutBorrowGlobalDeprecated(..));
                let operand_index = self.temp_stack.pop().unwrap();
                let temp_index = self.temp_count;
                self.local_types.push(Type::Reference(
                    is_mut,
                    Box::new(Type::Datatype(
                        struct_env.module_env.get_id(),
                        struct_env.get_id(),
                        vec![],
                    )),
                ));
                self.temp_stack.push(temp_index);
                self.temp_count += 1;
                self.code.push(mk_unary(
                    Operation::BorrowGlobal(
                        self.func_env.module_env.get_id(),
                        self.func_env.module_env.get_struct_id(*idx),
                        vec![],
                    ),
                    temp_index,
                    operand_index,
                ));
            }

            MoveBytecode::MutBorrowGlobalGenericDeprecated(idx)
            | MoveBytecode::ImmBorrowGlobalGenericDeprecated(idx) => {
                let struct_instantiation = self.module.struct_instantiation_at(*idx);
                let is_mut = matches!(bytecode, MoveBytecode::MutBorrowGlobalGenericDeprecated(..));
                let struct_env = self
                    .func_env
                    .module_env
                    .get_struct_by_def_idx(struct_instantiation.def);

                let operand_index = self.temp_stack.pop().unwrap();
                let temp_index = self.temp_count;
                let actuals = self.get_type_params(struct_instantiation.type_parameters);
                self.local_types.push(Type::Reference(
                    is_mut,
                    Box::new(Type::Datatype(
                        struct_env.module_env.get_id(),
                        struct_env.get_id(),
                        actuals.clone(),
                    )),
                ));
                self.temp_stack.push(temp_index);
                self.temp_count += 1;
                self.code.push(mk_unary(
                    Operation::BorrowGlobal(
                        self.func_env.module_env.get_id(),
                        self.func_env
                            .module_env
                            .get_struct_id(struct_instantiation.def),
                        actuals,
                    ),
                    temp_index,
                    operand_index,
                ));
            }

            MoveBytecode::MoveFromDeprecated(idx) => {
                let struct_env = self.func_env.module_env.get_struct_by_def_idx(*idx);
                let operand_index = self.temp_stack.pop().unwrap();
                let temp_index = self.temp_count;
                self.temp_stack.push(temp_index);
                self.local_types.push(Type::Datatype(
                    struct_env.module_env.get_id(),
                    struct_env.get_id(),
                    vec![],
                ));
                self.temp_count += 1;
                self.code.push(mk_unary(
                    Operation::MoveFrom(
                        self.func_env.module_env.get_id(),
                        self.func_env.module_env.get_struct_id(*idx),
                        vec![],
                    ),
                    temp_index,
                    operand_index,
                ));
            }

            MoveBytecode::MoveFromGenericDeprecated(idx) => {
                let struct_instantiation = self.module.struct_instantiation_at(*idx);
                let struct_env = self
                    .func_env
                    .module_env
                    .get_struct_by_def_idx(struct_instantiation.def);
                let operand_index = self.temp_stack.pop().unwrap();
                let temp_index = self.temp_count;
                self.temp_stack.push(temp_index);
                let actuals = self.get_type_params(struct_instantiation.type_parameters);
                self.local_types.push(Type::Datatype(
                    struct_env.module_env.get_id(),
                    struct_env.get_id(),
                    actuals.clone(),
                ));
                self.temp_count += 1;
                self.code.push(mk_unary(
                    Operation::MoveFrom(
                        self.func_env.module_env.get_id(),
                        self.func_env
                            .module_env
                            .get_struct_id(struct_instantiation.def),
                        actuals,
                    ),
                    temp_index,
                    operand_index,
                ));
            }

            MoveBytecode::MoveToDeprecated(idx) => {
                let value_operand_index = self.temp_stack.pop().unwrap();
                let signer_operand_index = self.temp_stack.pop().unwrap();
                self.code.push(mk_call(
                    Operation::MoveTo(
                        self.func_env.module_env.get_id(),
                        self.func_env.module_env.get_struct_id(*idx),
                        vec![],
                    ),
                    vec![],
                    vec![value_operand_index, signer_operand_index],
                ));
            }

            MoveBytecode::MoveToGenericDeprecated(idx) => {
                let struct_instantiation = self.module.struct_instantiation_at(*idx);
                let value_operand_index = self.temp_stack.pop().unwrap();
                let signer_operand_index = self.temp_stack.pop().unwrap();
                self.code.push(mk_call(
                    Operation::MoveTo(
                        self.func_env.module_env.get_id(),
                        self.func_env
                            .module_env
                            .get_struct_id(struct_instantiation.def),
                        self.get_type_params(struct_instantiation.type_parameters),
                    ),
                    vec![],
                    vec![value_operand_index, signer_operand_index],
                ));
            }

            MoveBytecode::Nop => self.code.push(Bytecode::Nop(attr_id)),

            // TODO full prover support for vector bytecode instructions
            // These should go to non-functional call operations
            MoveBytecode::VecLen(sig) => {
                let tys = self.get_type_params(*sig);
                let operand_index = self.temp_stack.pop().unwrap();
                let temp_index = self.temp_count;
                self.local_types.push(Type::Primitive(PrimitiveType::U64));
                self.temp_count += 1;
                self.temp_stack.push(temp_index);
                self.code.push(Bytecode::Call(
                    attr_id,
                    vec![temp_index],
                    mk_vec_function_operation("length", tys),
                    vec![operand_index],
                    None,
                ))
            }
            MoveBytecode::VecMutBorrow(sig) | MoveBytecode::VecImmBorrow(sig) => {
                let is_mut = match bytecode {
                    MoveBytecode::VecMutBorrow(_) => true,
                    MoveBytecode::VecImmBorrow(_) => false,
                    _ => unreachable!(),
                };
                let [ty]: [Type; 1] = self.get_type_params(*sig).try_into().unwrap();
                let operand2_index = self.temp_stack.pop().unwrap();
                let operand1_index = self.temp_stack.pop().unwrap();
                let temp_index = self.temp_count;
                self.local_types
                    .push(Type::Reference(is_mut, Box::new(ty.clone())));
                self.temp_count += 1;
                self.temp_stack.push(temp_index);
                let vec_fun = if is_mut { "borrow_mut" } else { "borrow" };
                self.code.push(Bytecode::Call(
                    attr_id,
                    vec![temp_index],
                    mk_vec_function_operation(vec_fun, vec![ty]),
                    vec![operand1_index, operand2_index],
                    None,
                ))
            }
            MoveBytecode::VecPushBack(sig) => {
                let tys = self.get_type_params(*sig);
                let operand2_index = self.temp_stack.pop().unwrap();
                let operand1_index = self.temp_stack.pop().unwrap();
                self.code.push(Bytecode::Call(
                    attr_id,
                    vec![],
                    mk_vec_function_operation("push_back", tys),
                    vec![operand1_index, operand2_index],
                    None,
                ))
            }
            MoveBytecode::VecPopBack(sig) => {
                let [ty]: [Type; 1] = self.get_type_params(*sig).try_into().unwrap();
                let operand_index = self.temp_stack.pop().unwrap();
                let temp_index = self.temp_count;
                self.local_types.push(ty.clone());
                self.temp_count += 1;
                self.temp_stack.push(temp_index);
                self.code.push(Bytecode::Call(
                    attr_id,
                    vec![temp_index],
                    mk_vec_function_operation("pop_back", vec![ty]),
                    vec![operand_index],
                    None,
                ))
            }
            MoveBytecode::VecSwap(sig) => {
                let tys = self.get_type_params(*sig);
                let operand3_index = self.temp_stack.pop().unwrap();
                let operand2_index = self.temp_stack.pop().unwrap();
                let operand1_index = self.temp_stack.pop().unwrap();
                self.code.push(Bytecode::Call(
                    attr_id,
                    vec![],
                    mk_vec_function_operation("swap", tys),
                    vec![operand1_index, operand2_index, operand3_index],
                    None,
                ))
            }
            MoveBytecode::VecPack(sig, n) => {
                let n = *n as usize;
                let [ty]: [Type; 1] = self.get_type_params(*sig).try_into().unwrap();
                let operands = self.temp_stack.split_off(self.temp_stack.len() - n);
                let temp_index = self.temp_count;
                self.local_types.push(Type::Vector(Box::new(ty.clone())));
                self.temp_count += 1;
                self.temp_stack.push(temp_index);

                self.code.push(Bytecode::Call(
                    attr_id,
                    vec![temp_index],
                    mk_vec_function_operation("empty", vec![ty.clone()]),
                    vec![],
                    None,
                ));
                if !operands.is_empty() {
                    let mut_ref_index = self.temp_count;
                    self.local_types.push(Type::Reference(
                        true,
                        Box::new(Type::Vector(Box::new(ty.clone()))),
                    ));
                    self.temp_count += 1;

                    self.code
                        .push(mk_unary(Operation::BorrowLoc, mut_ref_index, temp_index));

                    for operand in operands {
                        self.code.push(Bytecode::Call(
                            attr_id,
                            vec![],
                            mk_vec_function_operation("push_back", vec![ty.clone()]),
                            vec![mut_ref_index, operand],
                            None,
                        ));
                    }
                }
            }
            MoveBytecode::VecUnpack(sig, n) => {
                let n = *n as usize;
                let [ty]: [Type; 1] = self.get_type_params(*sig).try_into().unwrap();
                let operand_index = self.temp_stack.pop().unwrap();
                let temps = (0..n).map(|idx| self.temp_count + idx).collect::<Vec<_>>();
                self.local_types.extend(vec![ty.clone(); n]);
                self.temp_count += n;
                self.temp_stack.extend(&temps);

                if !temps.is_empty() {
                    let mut_ref_index = self.temp_count;
                    self.local_types.push(Type::Reference(
                        true,
                        Box::new(Type::Vector(Box::new(ty.clone()))),
                    ));
                    self.temp_count += 1;

                    self.code
                        .push(mk_unary(Operation::BorrowLoc, mut_ref_index, operand_index));

                    for temp in temps {
                        self.code.push(Bytecode::Call(
                            attr_id,
                            vec![temp],
                            mk_vec_function_operation("pop_back", vec![ty.clone()]),
                            vec![mut_ref_index],
                            None,
                        ));
                    }
                }

                self.code.push(Bytecode::Call(
                    attr_id,
                    vec![],
                    mk_vec_function_operation("destroy_empty", vec![ty]),
                    vec![operand_index],
                    None,
                ))
            }
            MoveBytecode::PackVariant(vhi) => {
                let handle = self
                    .func_env
                    .module_env
                    .get_verified_module()
                    .variant_handle_at(*vhi);
                let enum_env = self
                    .func_env
                    .module_env
                    .get_enum_by_def_idx(handle.enum_def);
                let variant_env = enum_env.get_variant_by_tag(handle.variant as usize);
                let mut field_temp_indices = vec![];
                let variant_temp_index = self.temp_count;
                for _ in variant_env.get_fields() {
                    let field_temp_index = self.temp_stack.pop().unwrap();
                    field_temp_indices.push(field_temp_index);
                }
                self.local_types.push(Type::Datatype(
                    enum_env.module_env.get_id(),
                    enum_env.get_id(),
                    vec![],
                ));
                self.temp_stack.push(variant_temp_index);
                field_temp_indices.reverse();
                self.code.push(mk_call(
                    Operation::PackVariant(
                        enum_env.module_env.get_id(),
                        enum_env.get_id(),
                        variant_env.get_id(),
                        vec![],
                    ),
                    vec![variant_temp_index],
                    field_temp_indices,
                ));
                self.temp_count += 1;
            }
            MoveBytecode::PackVariantGeneric(vhiid) => {
                let handle = self
                    .func_env
                    .module_env
                    .get_verified_module()
                    .variant_instantiation_handle_at(*vhiid);
                let enum_instantiation = self
                    .func_env
                    .module_env
                    .get_verified_module()
                    .enum_instantiation_at(handle.enum_def);
                let actuals = self.get_type_params(enum_instantiation.type_parameters);
                let enum_env = self
                    .func_env
                    .module_env
                    .get_enum_by_def_idx(enum_instantiation.def);
                let variant_env = enum_env.get_variant_by_tag(handle.variant as usize);
                let mut field_temp_indices = vec![];
                let variant_temp_index = self.temp_count;
                for _ in variant_env.get_fields() {
                    let field_temp_index = self.temp_stack.pop().unwrap();
                    field_temp_indices.push(field_temp_index);
                }
                self.local_types.push(Type::Datatype(
                    enum_env.module_env.get_id(),
                    enum_env.get_id(),
                    actuals.clone(),
                ));
                self.temp_stack.push(variant_temp_index);
                field_temp_indices.reverse();
                self.code.push(mk_call(
                    Operation::PackVariant(
                        enum_env.module_env.get_id(),
                        enum_env.get_id(),
                        variant_env.get_id(),
                        actuals,
                    ),
                    vec![variant_temp_index],
                    field_temp_indices,
                ));
                self.temp_count += 1;
            }
            MoveBytecode::UnpackVariant(vhi)
            | MoveBytecode::UnpackVariantImmRef(vhi)
            | MoveBytecode::UnpackVariantMutRef(vhi) => {
                let handle = self
                    .func_env
                    .module_env
                    .get_verified_module()
                    .variant_handle_at(*vhi);
                let enum_env = self
                    .func_env
                    .module_env
                    .get_enum_by_def_idx(handle.enum_def);
                let variant_env = enum_env.get_variant_by_tag(handle.variant as usize);
                let mut field_temp_indices = vec![];
                let unpack_type = |ty| match bytecode {
                    MoveBytecode::UnpackVariantImmRef(_) => Type::Reference(false, Box::new(ty)),
                    MoveBytecode::UnpackVariantMutRef(_) => Type::Reference(true, Box::new(ty)),
                    MoveBytecode::UnpackVariant(_) => ty,
                    _ => unreachable!(),
                };
                let ref_type = match bytecode {
                    MoveBytecode::UnpackVariant(_) => RefType::ByValue,
                    MoveBytecode::UnpackVariantImmRef(_) => RefType::ByImmRef,
                    MoveBytecode::UnpackVariantMutRef(_) => RefType::ByMutRef,
                    _ => unreachable!(),
                };
                let variant_temp_index = self.temp_stack.pop().unwrap();
                for field_env in variant_env.get_fields() {
                    let field_temp_index = self.temp_count;
                    field_temp_indices.push(field_temp_index);
                    self.temp_stack.push(field_temp_index);
                    self.local_types.push(unpack_type(field_env.get_type()));
                    self.temp_count += 1;
                }
                self.code.push(mk_call(
                    Operation::UnpackVariant(
                        enum_env.module_env.get_id(),
                        enum_env.get_id(),
                        variant_env.get_id(),
                        vec![],
                        ref_type,
                    ),
                    field_temp_indices,
                    vec![variant_temp_index],
                ));
            }
            MoveBytecode::UnpackVariantGeneric(vhiid)
            | MoveBytecode::UnpackVariantGenericImmRef(vhiid)
            | MoveBytecode::UnpackVariantGenericMutRef(vhiid) => {
                let handle = self
                    .func_env
                    .module_env
                    .get_verified_module()
                    .variant_instantiation_handle_at(*vhiid);
                let enum_instantiation = self
                    .func_env
                    .module_env
                    .get_verified_module()
                    .enum_instantiation_at(handle.enum_def);
                let actuals = self.get_type_params(enum_instantiation.type_parameters);
                let enum_env = self
                    .func_env
                    .module_env
                    .get_enum_by_def_idx(enum_instantiation.def);
                let variant_env = enum_env.get_variant_by_tag(handle.variant as usize);
                let mut field_temp_indices = vec![];
                let unpack_type = |ty| match bytecode {
                    MoveBytecode::UnpackVariantImmRef(_) => Type::Reference(false, Box::new(ty)),
                    MoveBytecode::UnpackVariantMutRef(_) => Type::Reference(true, Box::new(ty)),
                    MoveBytecode::UnpackVariant(_) => ty,
                    _ => unreachable!(),
                };
                let ref_type = match bytecode {
                    MoveBytecode::UnpackVariant(_) => RefType::ByValue,
                    MoveBytecode::UnpackVariantImmRef(_) => RefType::ByImmRef,
                    MoveBytecode::UnpackVariantMutRef(_) => RefType::ByMutRef,
                    _ => unreachable!(),
                };
                let variant_temp_index = self.temp_stack.pop().unwrap();
                for field_env in variant_env.get_fields() {
                    let field_temp_index = self.temp_count;
                    field_temp_indices.push(field_temp_index);
                    self.temp_stack.push(field_temp_index);
                    self.local_types
                        .push(unpack_type(field_env.get_type().instantiate(&actuals)));
                    self.temp_count += 1;
                }
                self.code.push(mk_call(
                    Operation::UnpackVariant(
                        enum_env.module_env.get_id(),
                        enum_env.get_id(),
                        variant_env.get_id(),
                        actuals,
                        ref_type,
                    ),
                    field_temp_indices,
                    vec![variant_temp_index],
                ));
            }
            MoveBytecode::VariantSwitch(jump_table_idx) => {
                let jump_table = self.func_env.get_jump_tables()[jump_table_idx.0 as usize].clone();
                let temp_index = self.temp_stack.pop().unwrap();
                let JumpTableInner::Full(jump_table) = jump_table.jump_table;
                let labels = jump_table
                    .iter()
                    .map(|off| label_map[off])
                    .collect::<Vec<_>>();
                self.code
                    .push(Bytecode::VariantSwitch(attr_id, temp_index, labels));
            }
        }
    }

    fn translate_value(ty: &Type, value: &MoveValue) -> Constant {
        match (ty, &value) {
            (Type::Vector(inner), MoveValue::Vector(vs)) => match **inner {
                Type::Primitive(PrimitiveType::U8) => {
                    let b = vs
                        .iter()
                        .map(|v| match Self::translate_value(inner, v) {
                            Constant::U8(u) => u,
                            _ => panic!("Expected u8, but found: {:?}", inner),
                        })
                        .collect::<Vec<u8>>();
                    Constant::ByteArray(b)
                }
                Type::Primitive(PrimitiveType::Address) => {
                    let b = vs
                        .iter()
                        .map(|v| match Self::translate_value(inner, v) {
                            Constant::Address(a) => a,
                            _ => panic!("Expected address, but found: {:?}", inner),
                        })
                        .collect::<Vec<BigUint>>();
                    Constant::AddressArray(b)
                }
                _ => {
                    let b = vs
                        .iter()
                        .map(|v| Self::translate_value(inner, v))
                        .collect::<Vec<Constant>>();
                    Constant::Vector(b)
                }
            },
            (Type::Primitive(PrimitiveType::Bool), MoveValue::Bool(b)) => Constant::Bool(*b),
            (Type::Primitive(PrimitiveType::U8), MoveValue::U8(b)) => Constant::U8(*b),
            (Type::Primitive(PrimitiveType::U16), MoveValue::U16(b)) => Constant::U16(*b),
            (Type::Primitive(PrimitiveType::U32), MoveValue::U32(b)) => Constant::U32(*b),
            (Type::Primitive(PrimitiveType::U64), MoveValue::U64(b)) => Constant::U64(*b),
            (Type::Primitive(PrimitiveType::U128), MoveValue::U128(b)) => Constant::U128(*b),
            (Type::Primitive(PrimitiveType::U256), MoveValue::U256(b)) => Constant::U256(b.into()),
            (Type::Primitive(PrimitiveType::Address), MoveValue::Address(a)) => {
                Constant::Address(move_model::addr_to_big_uint(a))
            }
            _ => panic!("Unexpected (and possibly invalid) constant type: {:?}", ty),
        }
    }
}
