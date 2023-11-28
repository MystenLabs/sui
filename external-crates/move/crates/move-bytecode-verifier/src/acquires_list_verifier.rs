// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module implements a checker for verifying properties about the acquires list on function
//! definitions. Function definitions must annotate the global resources (declared in that module)
//! accesssed by `BorrowGlobal`, `MoveFrom`, and any transitive function calls
//! The list of acquired resources (stored in `FunctionDefinition`'s `acquires_global_resources`
//! field) must have:
//! - No duplicate resources (checked by `check_duplication`)
//! - No missing resources (any resource acquired must be present)
//! - No additional resources (no extraneous resources not actually acquired)

use std::collections::{BTreeSet, HashMap};

use crate::meter::Meter;
use move_binary_format::{
    access::ModuleAccess,
    errors::{PartialVMError, PartialVMResult},
    file_format::{
        Bytecode, CodeOffset, CompiledModule, FunctionDefinition, FunctionDefinitionIndex,
        FunctionHandle, FunctionHandleIndex, StructDefinitionIndex,
    },
    safe_unwrap,
};
use move_core_types::vm_status::StatusCode;

pub(crate) struct AcquiresVerifier<'a> {
    module: &'a CompiledModule,
    current_function: FunctionDefinitionIndex,
    annotated_acquires: BTreeSet<StructDefinitionIndex>,
    actual_acquires: BTreeSet<StructDefinitionIndex>,
    handle_to_def: HashMap<FunctionHandleIndex, &'a FunctionDefinition>,
}

impl<'a> AcquiresVerifier<'a> {
    pub(crate) fn verify(
        module: &'a CompiledModule,
        index: FunctionDefinitionIndex,
        function_definition: &'a FunctionDefinition,
        _meter: &mut impl Meter, // currently unused
    ) -> PartialVMResult<()> {
        let annotated_acquires: BTreeSet<_> = function_definition
            .acquires_global_resources
            .iter()
            .cloned()
            .collect();
        let mut handle_to_def = HashMap::new();
        for func_def in module.function_defs() {
            handle_to_def.insert(func_def.function, func_def);
        }
        let mut verifier = Self {
            module,
            current_function: index,
            annotated_acquires,
            actual_acquires: BTreeSet::new(),
            handle_to_def,
        };

        for (offset, instruction) in safe_unwrap!(function_definition.code.as_ref())
            .code
            .iter()
            .enumerate()
        {
            verifier.verify_instruction(instruction, offset as CodeOffset)?
        }

        for annotation in verifier.annotated_acquires {
            if !verifier.actual_acquires.contains(&annotation) {
                return Err(PartialVMError::new(
                    StatusCode::EXTRANEOUS_ACQUIRES_ANNOTATION,
                ));
            }

            let struct_def = safe_unwrap!(module.struct_defs().get(annotation.0 as usize));
            let struct_handle = module.datatype_handle_at(struct_def.struct_handle);
            if !struct_handle.abilities.has_key() {
                return Err(PartialVMError::new(StatusCode::INVALID_ACQUIRES_ANNOTATION));
            }
        }

        Ok(())
    }

    fn verify_instruction(
        &mut self,
        instruction: &Bytecode,
        offset: CodeOffset,
    ) -> PartialVMResult<()> {
        match instruction {
            Bytecode::Call(idx) => self.call_acquire(*idx, offset),
            Bytecode::CallGeneric(idx) => {
                let fi = self.module.function_instantiation_at(*idx);
                self.call_acquire(fi.handle, offset)
            }
            Bytecode::MoveFrom(idx)
            | Bytecode::MutBorrowGlobal(idx)
            | Bytecode::ImmBorrowGlobal(idx) => self.struct_acquire(*idx, offset),
            Bytecode::MoveFromGeneric(idx)
            | Bytecode::MutBorrowGlobalGeneric(idx)
            | Bytecode::ImmBorrowGlobalGeneric(idx) => {
                let si = self.module.struct_instantiation_at(*idx);
                self.struct_acquire(si.def, offset)
            }

            Bytecode::Pop
            | Bytecode::BrTrue(_)
            | Bytecode::BrFalse(_)
            | Bytecode::Abort
            | Bytecode::Branch(_)
            | Bytecode::Nop
            | Bytecode::Ret
            | Bytecode::StLoc(_)
            | Bytecode::MoveLoc(_)
            | Bytecode::CopyLoc(_)
            | Bytecode::ImmBorrowLoc(_)
            | Bytecode::MutBorrowLoc(_)
            | Bytecode::FreezeRef
            | Bytecode::MutBorrowField(_)
            | Bytecode::MutBorrowFieldGeneric(_)
            | Bytecode::ImmBorrowField(_)
            | Bytecode::ImmBorrowFieldGeneric(_)
            | Bytecode::LdU8(_)
            | Bytecode::LdU16(_)
            | Bytecode::LdU32(_)
            | Bytecode::LdU64(_)
            | Bytecode::LdU128(_)
            | Bytecode::LdU256(_)
            | Bytecode::LdConst(_)
            | Bytecode::LdTrue
            | Bytecode::LdFalse
            | Bytecode::Pack(_)
            | Bytecode::PackGeneric(_)
            | Bytecode::Unpack(_)
            | Bytecode::UnpackGeneric(_)
            | Bytecode::ReadRef
            | Bytecode::WriteRef
            | Bytecode::CastU8
            | Bytecode::CastU16
            | Bytecode::CastU32
            | Bytecode::CastU64
            | Bytecode::CastU128
            | Bytecode::CastU256
            | Bytecode::Add
            | Bytecode::Sub
            | Bytecode::Mul
            | Bytecode::Mod
            | Bytecode::Div
            | Bytecode::BitOr
            | Bytecode::BitAnd
            | Bytecode::Xor
            | Bytecode::Shl
            | Bytecode::Shr
            | Bytecode::Or
            | Bytecode::And
            | Bytecode::Not
            | Bytecode::Eq
            | Bytecode::Neq
            | Bytecode::Lt
            | Bytecode::Gt
            | Bytecode::Le
            | Bytecode::Ge
            | Bytecode::Exists(_)
            | Bytecode::ExistsGeneric(_)
            | Bytecode::MoveTo(_)
            | Bytecode::MoveToGeneric(_)
            | Bytecode::VecPack(..)
            | Bytecode::VecLen(_)
            | Bytecode::VecImmBorrow(_)
            | Bytecode::VecMutBorrow(_)
            | Bytecode::VecPushBack(_)
            | Bytecode::VecPopBack(_)
            | Bytecode::VecUnpack(..)
            | Bytecode::VecSwap(_)
            | Bytecode::PackVariant(_, _)
            | Bytecode::PackVariantGeneric(_, _)
            | Bytecode::UnpackVariant(_, _)
            | Bytecode::UnpackVariantImmRef(_, _)
            | Bytecode::UnpackVariantMutRef(_, _)
            | Bytecode::UnpackVariantGeneric(_, _)
            | Bytecode::UnpackVariantGenericImmRef(_, _)
            | Bytecode::UnpackVariantGenericMutRef(_, _)
            | Bytecode::VariantSwitch(_) => Ok(()),
        }
    }

    fn call_acquire(
        &mut self,
        fh_idx: FunctionHandleIndex,
        offset: CodeOffset,
    ) -> PartialVMResult<()> {
        let function_handle = self.module.function_handle_at(fh_idx);
        let mut function_acquired_resources =
            self.function_acquired_resources(function_handle, fh_idx);
        for acquired_resource in &function_acquired_resources {
            if !self.annotated_acquires.contains(acquired_resource) {
                return Err(self.error(StatusCode::MISSING_ACQUIRES_ANNOTATION, offset));
            }
        }
        self.actual_acquires
            .append(&mut function_acquired_resources);
        Ok(())
    }

    fn struct_acquire(
        &mut self,
        sd_idx: StructDefinitionIndex,
        offset: CodeOffset,
    ) -> PartialVMResult<()> {
        if self.annotated_acquires.contains(&sd_idx) {
            self.actual_acquires.insert(sd_idx);
            Ok(())
        } else {
            Err(self.error(StatusCode::MISSING_ACQUIRES_ANNOTATION, offset))
        }
    }

    fn function_acquired_resources(
        &self,
        function_handle: &FunctionHandle,
        fh_idx: FunctionHandleIndex,
    ) -> BTreeSet<StructDefinitionIndex> {
        if function_handle.module != self.module.self_handle_idx() {
            return BTreeSet::new();
        }
        match self.handle_to_def.get(&fh_idx) {
            Some(func_def) => func_def.acquires_global_resources.iter().cloned().collect(),
            None => BTreeSet::new(),
        }
    }

    fn error(&self, status: StatusCode, offset: CodeOffset) -> PartialVMError {
        PartialVMError::new(status).at_code_offset(self.current_function, offset)
    }
}
