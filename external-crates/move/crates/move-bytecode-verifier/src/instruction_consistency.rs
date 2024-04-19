// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module defines the transfer functions for verifying consistency of each bytecode
//! instruction, in particular, for the bytecode instructions that come in both generic and
//! non-generic flavors. It also checks constraints on instructions like VecPack/VecUnpack.

use move_binary_format::{
    errors::{Location, PartialVMError, PartialVMResult, VMResult},
    file_format::{
        Bytecode, CodeOffset, CodeUnit, CompiledModule, DatatypeHandleIndex, EnumDefinitionIndex,
        FieldHandleIndex, FunctionDefinitionIndex, FunctionHandleIndex, StructDefinitionIndex,
        TableIndex,
    },
};
use move_core_types::vm_status::StatusCode;

pub struct InstructionConsistency<'a> {
    module: &'a CompiledModule,
    current_function: Option<FunctionDefinitionIndex>,
}

impl<'a> InstructionConsistency<'a> {
    pub fn verify_module(module: &'a CompiledModule) -> VMResult<()> {
        Self::verify_module_impl(module).map_err(|e| e.finish(Location::Module(module.self_id())))
    }

    fn verify_module_impl(module: &'a CompiledModule) -> PartialVMResult<()> {
        for (idx, func_def) in module.function_defs().iter().enumerate() {
            match &func_def.code {
                None => (),
                Some(code) => {
                    let checker = Self {
                        module,
                        current_function: Some(FunctionDefinitionIndex(idx as TableIndex)),
                    };
                    checker.check_instructions(code)?
                }
            }
        }
        Ok(())
    }

    fn check_instructions(&self, code: &CodeUnit) -> PartialVMResult<()> {
        for (offset, instr) in code.code.iter().enumerate() {
            use Bytecode::*;

            match instr {
                MutBorrowField(field_handle_index) => {
                    self.check_field_op(offset, *field_handle_index, /* generic */ false)?;
                }
                MutBorrowFieldGeneric(field_inst_index) => {
                    let field_inst = self.module.field_instantiation_at(*field_inst_index);
                    self.check_field_op(offset, field_inst.handle, /* generic */ true)?;
                }
                ImmBorrowField(field_handle_index) => {
                    self.check_field_op(offset, *field_handle_index, /* generic */ false)?;
                }
                ImmBorrowFieldGeneric(field_inst_index) => {
                    let field_inst = self.module.field_instantiation_at(*field_inst_index);
                    self.check_field_op(offset, field_inst.handle, /* non_ */ true)?;
                }
                Call(idx) => {
                    self.check_function_op(offset, *idx, /* generic */ false)?;
                }
                CallGeneric(idx) => {
                    let func_inst = self.module.function_instantiation_at(*idx);
                    self.check_function_op(offset, func_inst.handle, /* generic */ true)?;
                }
                Pack(idx) => {
                    self.check_struct_type_op(offset, *idx, /* generic */ false)?;
                }
                PackGeneric(idx) => {
                    let struct_inst = self.module.struct_instantiation_at(*idx);
                    self.check_struct_type_op(offset, struct_inst.def, /* generic */ true)?;
                }
                Unpack(idx) => {
                    self.check_struct_type_op(offset, *idx, /* generic */ false)?;
                }
                UnpackGeneric(idx) => {
                    let struct_inst = self.module.struct_instantiation_at(*idx);
                    self.check_struct_type_op(offset, struct_inst.def, /* generic */ true)?;
                }
                MutBorrowGlobalDeprecated(idx) => {
                    self.check_struct_type_op(offset, *idx, /* generic */ false)?;
                }
                MutBorrowGlobalGenericDeprecated(idx) => {
                    let struct_inst = self.module.struct_instantiation_at(*idx);
                    self.check_struct_type_op(offset, struct_inst.def, /* generic */ true)?;
                }
                ImmBorrowGlobalDeprecated(idx) => {
                    self.check_struct_type_op(offset, *idx, /* generic */ false)?;
                }
                ImmBorrowGlobalGenericDeprecated(idx) => {
                    let struct_inst = self.module.struct_instantiation_at(*idx);
                    self.check_struct_type_op(offset, struct_inst.def, /* generic */ true)?;
                }
                ExistsDeprecated(idx) => {
                    self.check_struct_type_op(offset, *idx, /* generic */ false)?;
                }
                ExistsGenericDeprecated(idx) => {
                    let struct_inst = self.module.struct_instantiation_at(*idx);
                    self.check_struct_type_op(offset, struct_inst.def, /* generic */ true)?;
                }
                MoveFromDeprecated(idx) => {
                    self.check_struct_type_op(offset, *idx, /* generic */ false)?;
                }
                MoveFromGenericDeprecated(idx) => {
                    let struct_inst = self.module.struct_instantiation_at(*idx);
                    self.check_struct_type_op(offset, struct_inst.def, /* generic */ true)?;
                }
                MoveToDeprecated(idx) => {
                    self.check_struct_type_op(offset, *idx, /* generic */ false)?;
                }
                MoveToGenericDeprecated(idx) => {
                    let struct_inst = self.module.struct_instantiation_at(*idx);
                    self.check_struct_type_op(offset, struct_inst.def, /* generic */ true)?;
                }
                VecPack(_, num) | VecUnpack(_, num) => {
                    if *num > u16::MAX as u64 {
                        return Err(PartialVMError::new(StatusCode::CONSTRAINT_NOT_SATISFIED)
                            .at_code_offset(self.current_function(), offset as CodeOffset)
                            .with_message("VecPack/VecUnpack argument out of range".to_string()));
                    }
                }

                // List out the other options explicitly so there's a compile error if a new
                // bytecode gets added.
                FreezeRef | Pop | Ret | Branch(_) | BrTrue(_) | BrFalse(_) | LdU8(_) | LdU16(_)
                | LdU32(_) | LdU64(_) | LdU128(_) | LdU256(_) | LdConst(_) | CastU8 | CastU16
                | CastU32 | CastU64 | CastU128 | CastU256 | LdTrue | LdFalse | ReadRef
                | WriteRef | Add | Sub | Mul | Mod | Div | BitOr | BitAnd | Xor | Shl | Shr
                | Or | And | Not | Eq | Neq | Lt | Gt | Le | Ge | CopyLoc(_) | MoveLoc(_)
                | StLoc(_) | MutBorrowLoc(_) | ImmBorrowLoc(_) | VecLen(_) | VecImmBorrow(_)
                | VecMutBorrow(_) | VecPushBack(_) | VecPopBack(_) | VecSwap(_) | Abort | Nop
                | VariantSwitch(_) => (),
                PackVariant(v_handle)
                | UnpackVariant(v_handle)
                | UnpackVariantImmRef(v_handle)
                | UnpackVariantMutRef(v_handle) => {
                    let handle = self.module.variant_handle_at(*v_handle);
                    self.check_enum_type_op(offset, handle.enum_def, /* generic */ false)?;
                }
                PackVariantGeneric(vi_handle)
                | UnpackVariantGeneric(vi_handle)
                | UnpackVariantGenericImmRef(vi_handle)
                | UnpackVariantGenericMutRef(vi_handle) => {
                    let handle = self.module.variant_instantiation_handle_at(*vi_handle);
                    let enum_inst = self.module.enum_instantiation_at(handle.enum_def);
                    self.check_enum_type_op(offset, enum_inst.def, /* generic */ true)?;
                }
            }
        }
        Ok(())
    }

    //
    // Helpers for instructions that come in a generic and non generic form.
    // Verifies the generic form uses a generic member and the non generic form
    // a non generic one.
    //

    fn check_field_op(
        &self,
        offset: usize,
        field_handle_index: FieldHandleIndex,
        generic: bool,
    ) -> PartialVMResult<()> {
        let field_handle = self.module.field_handle_at(field_handle_index);
        self.check_struct_type_op(offset, field_handle.owner, generic)
    }

    fn current_function(&self) -> FunctionDefinitionIndex {
        self.current_function.unwrap_or(FunctionDefinitionIndex(0))
    }

    fn check_struct_type_op(
        &self,
        offset: usize,
        struct_def_index: StructDefinitionIndex,
        generic: bool,
    ) -> PartialVMResult<()> {
        let struct_def = self.module.struct_def_at(struct_def_index);
        self.check_type_op_(offset, struct_def.struct_handle, generic)
    }

    fn check_enum_type_op(
        &self,
        offset: usize,
        enum_def_index: EnumDefinitionIndex,
        generic: bool,
    ) -> PartialVMResult<()> {
        let enum_def = self.module.enum_def_at(enum_def_index);
        self.check_type_op_(offset, enum_def.enum_handle, generic)
    }

    fn check_type_op_(
        &self,
        offset: usize,
        datatype_handle_index: DatatypeHandleIndex,
        generic: bool,
    ) -> PartialVMResult<()> {
        let datatype_handle = self.module.datatype_handle_at(datatype_handle_index);
        if datatype_handle.type_parameters.is_empty() == generic {
            return Err(
                PartialVMError::new(StatusCode::GENERIC_MEMBER_OPCODE_MISMATCH)
                    .at_code_offset(self.current_function(), offset as CodeOffset),
            );
        }
        Ok(())
    }

    fn check_function_op(
        &self,
        offset: usize,
        func_handle_index: FunctionHandleIndex,
        generic: bool,
    ) -> PartialVMResult<()> {
        let function_handle = self.module.function_handle_at(func_handle_index);
        if function_handle.type_parameters.is_empty() == generic {
            return Err(
                PartialVMError::new(StatusCode::GENERIC_MEMBER_OPCODE_MISMATCH)
                    .at_code_offset(self.current_function(), offset as CodeOffset),
            );
        }
        Ok(())
    }
}
