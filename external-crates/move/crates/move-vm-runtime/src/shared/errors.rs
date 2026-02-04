// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cache::identifier_interner::IdentifierInterner,
    execution::interpreter::state::{CallFrame, CallStack, MachineState},
    jit::execution::ast::Function,
};

use move_binary_format::errors::{HCFError, Location, PartialVMError, VMError};

// -------------------------------------------------------------------------------------------------
// Traits
// -------------------------------------------------------------------------------------------------

pub(crate) trait ErrorLocationInformation<T> {
    fn finish_with_location_info(self, location: T) -> VMError;
}

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl ErrorLocationInformation<(&Function, &IdentifierInterner)> for PartialVMError {
    fn finish_with_location_info(
        self,
        (fun, interner): (&Function, &IdentifierInterner),
    ) -> VMError {
        let module_id = match fun.module_id(interner) {
            Ok(mid) => mid,
            Err(err) => return err.into(),
        };
        self.finish(Location::Module(module_id))
    }
}

impl ErrorLocationInformation<&MachineState> for PartialVMError {
    fn finish_with_location_info(self, state: &MachineState) -> VMError {
        self.finish_with_location_info((&state.call_stack, state.interner.as_ref()))
    }
}

impl ErrorLocationInformation<(&CallStack, &IdentifierInterner)> for PartialVMError {
    fn finish_with_location_info(
        self,
        (stack, interner): (&CallStack, &IdentifierInterner),
    ) -> VMError {
        let frame = &stack.current_frame;
        self.finish_with_location_info((frame, interner))
    }
}

impl ErrorLocationInformation<(&CallFrame, &IdentifierInterner)> for PartialVMError {
    fn finish_with_location_info(
        self,
        (frame, interner): (&CallFrame, &IdentifierInterner),
    ) -> VMError {
        let function = frame.function();
        let err = self.at_code_offset(function.index(), frame.pc);
        err.finish_with_location_info((function, interner))
    }
}

impl<T> ErrorLocationInformation<T> for HCFError {
    fn finish_with_location_info(self, _location: T) -> VMError {
        self.into()
    }
}
