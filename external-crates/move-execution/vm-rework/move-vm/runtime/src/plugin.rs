// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{
    errors::{PartialVMResult, VMResult},
    file_format::Bytecode,
};
use move_vm_types::{loaded_data::runtime_types::Type, values::Locals};

use crate::{
    interpreter::{FrameInterface, InstrRet, InterpreterInterface},
    loader::{Function, Resolver},
};

pub(crate) trait InterpreterHook {
    fn is_critical(&self) -> bool;

    fn pre_entrypoint(
        &mut self,
        function: &Function,
        ty_args: &[Type],
        resolver: &Resolver,
    ) -> VMResult<()>;

    fn post_entrypoint(&mut self) -> VMResult<()>;

    fn pre_fn(
        &mut self,
        interpreter: &dyn InterpreterInterface,
        current_frame: &dyn FrameInterface,
        function: &Function,
        ty_args: Option<&[Type]>,
        resolver: &Resolver,
    ) -> VMResult<()>;

    fn post_fn(
        &mut self,
        // gas_meter: &mut impl GasMeter, TODO(wlmyng): GasMeter has a bunch of generic types that are incompatible with trait objects
        function: &Function,
    ) -> VMResult<()>;

    fn pre_instr(
        &mut self,
        interpreter: &dyn InterpreterInterface,
        // gas_meter: &mut impl GasMeter,
        function: &Function,
        instruction: &Bytecode,
        locals: &Locals,
        ty_args: &[Type],
        resolver: &Resolver,
    ) -> PartialVMResult<()>;

    fn post_instr(
        &mut self,
        interpreter: &dyn InterpreterInterface,
        // gas_meter: &mut impl GasMeter,
        function: &Function,
        instruction: &Bytecode,
        ty_args: &[Type],
        resolver: &Resolver,
        r: &InstrRet,
    ) -> PartialVMResult<()>;
}
