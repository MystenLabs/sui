// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    interpreter::state::{CallFrame, MachineState},
    loader::{arena::ArenaPointer, ast::Function, Loader},
};
use move_binary_format::errors::*;
use move_vm_profiler::{profile_close_frame, profile_open_frame};
use move_vm_types::{
    data_store::DataStore, gas::GasMeter, loaded_data::runtime_types::Type, values::Value,
};

use crate::native_extensions::NativeContextExtensions;

mod eval;
pub(crate) mod state;

/// Entrypoint into the interpreter. All external calls need to be routed through this
/// function.
pub(crate) fn run(
    function: ArenaPointer<Function>,
    ty_args: Vec<Type>,
    args: Vec<Value>,
    data_store: &impl DataStore,
    gas_meter: &mut impl GasMeter,
    extensions: &mut NativeContextExtensions,
    loader: &Loader,
) -> VMResult<Vec<Value>> {
    let runtime_limits_config = &loader.vm_config().runtime_limits_config;
    // TODO: Why does the VM config live on the loader?
    let fun_ref = function.to_ref();
    profile_open_frame!(gas_meter, fun_ref.pretty_string());

    if fun_ref.is_native() {
        let link_context = data_store.link_context();
        let resolver = fun_ref.get_resolver(link_context, loader);

        let return_values = eval::call_native_with_args(
            None,
            &resolver,
            gas_meter,
            runtime_limits_config,
            extensions,
            fun_ref,
            &ty_args,
            args.into(),
        )
        .map_err(|e| {
            e.at_code_offset(fun_ref.index(), 0)
                .finish(Location::Module(fun_ref.module_id().clone()))
        })?;

        profile_close_frame!(gas_meter, fun_ref.pretty_string());

        Ok(return_values.into_iter().collect())
    } else {
        let initial_frame = CallFrame::new(function, ty_args, args);
        let state = MachineState::new(runtime_limits_config.clone(), initial_frame);
        eval::run(state, data_store, gas_meter, extensions, loader)
    }
}
