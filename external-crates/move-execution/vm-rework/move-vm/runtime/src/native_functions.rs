// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{interpreter::Interpreter, loader::Resolver};
use move_binary_format::errors::{ExecutionState, PartialVMResult};
use move_core_types::{
    gas_algebra::InternalGas, language_storage::TypeTag, value::MoveTypeLayout,
    vm_status::StatusType,
};
use move_vm_config::runtime::VMRuntimeLimitsConfig;
use move_vm_types::{
    data_store::DataStore,
    loaded_data::runtime_types::Type,
    natives::{native_extensions::NativeContextExtensions, native_functions::NativeContext},
    values::Value,
};
use std::{cell::RefCell, fmt::Write};

pub struct NativeContextImpl<'a, 'b> {
    interpreter: &'a mut Interpreter,
    data_store: &'a mut dyn DataStore,
    resolver: &'a Resolver<'a>,
    extensions: &'a mut NativeContextExtensions<'b>,
    gas_left: RefCell<InternalGas>,
    gas_budget: InternalGas,
}

impl<'a, 'b> NativeContextImpl<'a, 'b> {
    pub(crate) fn new(
        interpreter: &'a mut Interpreter,
        data_store: &'a mut dyn DataStore,
        resolver: &'a Resolver<'a>,
        extensions: &'a mut NativeContextExtensions<'b>,
        gas_budget: InternalGas,
    ) -> Self {
        Self {
            interpreter,
            data_store,
            resolver,
            extensions,
            gas_left: RefCell::new(gas_budget),
            gas_budget,
        }
    }
}

impl<'a, 'b> NativeContext<'b> for NativeContextImpl<'a, 'b> {
    /// Limits imposed at runtime
    fn runtime_limits_config(&self) -> &VMRuntimeLimitsConfig {
        self.interpreter.runtime_limits_config()
    }

    fn print_stack_trace(&self, buf: &mut dyn Write) -> PartialVMResult<()> {
        self.interpreter
            .debug_print_stack_trace(buf, self.resolver.loader())
    }

    fn save_event(
        &mut self,
        guid: Vec<u8>,
        seq_num: u64,
        ty: Type,
        val: Value,
    ) -> PartialVMResult<bool> {
        match self.data_store.emit_event(guid, seq_num, ty, val) {
            Ok(()) => Ok(true),
            Err(e) if e.major_status().status_type() == StatusType::InvariantViolation => Err(e),
            Err(_) => Ok(false),
        }
    }

    fn events(&self) -> &Vec<(Vec<u8>, u64, Type, MoveTypeLayout, Value)> {
        self.data_store.events()
    }

    fn type_to_type_tag(&self, ty: &Type) -> PartialVMResult<TypeTag> {
        self.resolver.loader().type_to_type_tag(ty)
    }

    fn type_to_runtime_type_tag(&self, ty: &Type) -> PartialVMResult<TypeTag> {
        self.resolver.loader().type_to_runtime_type_tag(ty)
    }

    fn type_to_type_layout(&self, ty: &Type) -> PartialVMResult<Option<MoveTypeLayout>> {
        match self.resolver.type_to_type_layout(ty) {
            Ok(ty_layout) => Ok(Some(ty_layout)),
            Err(e) if e.major_status().status_type() == StatusType::InvariantViolation => Err(e),
            Err(_) => Ok(None),
        }
    }

    fn type_to_fully_annotated_layout(&self, ty: &Type) -> PartialVMResult<Option<MoveTypeLayout>> {
        match self.resolver.type_to_fully_annotated_layout(ty) {
            Ok(ty_layout) => Ok(Some(ty_layout)),
            Err(e) if e.major_status().status_type() == StatusType::InvariantViolation => Err(e),
            Err(_) => Ok(None),
        }
    }

    fn extensions(&self) -> &NativeContextExtensions<'b> {
        self.extensions
    }

    fn extensions_mut(&mut self) -> &mut NativeContextExtensions<'b> {
        self.extensions
    }

    /// Get count stack frames, including the one of the called native function. This
    /// allows a native function to reflect about its caller.
    fn stack_frames(&self, count: usize) -> ExecutionState {
        self.interpreter.get_stack_frames(count)
    }

    fn charge_gas(&self, amount: InternalGas) -> bool {
        let mut gas_left = self.gas_left.borrow_mut();

        match gas_left.checked_sub(amount) {
            Some(x) => {
                *gas_left = x;
                true
            }
            None => false,
        }
    }

    fn gas_budget(&self) -> InternalGas {
        self.gas_budget
    }

    fn gas_used(&self) -> InternalGas {
        self.gas_budget.saturating_sub(*self.gas_left.borrow())
    }
}
