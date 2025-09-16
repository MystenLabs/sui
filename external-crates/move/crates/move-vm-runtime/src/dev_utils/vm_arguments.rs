// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0
use crate::{
    execution::{
        TypeSubst as _,
        dispatch_tables::VMDispatchTables,
        interpreter::locals::{BaseHeap, BaseHeapId},
        values::Value,
        vm::MoveVM,
    },
    jit::execution::ast::Type,
    shared::gas::GasMeter,
};
use move_binary_format::errors::{Location, PartialVMError, PartialVMResult, VMResult};
use move_core_types::{identifier::IdentStr, language_storage::ModuleId, vm_status::StatusCode};
use move_trace_format::format::MoveTraceBuilder;
use std::{borrow::Borrow, collections::BTreeMap};
use tracing::warn;

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub struct ValueFrame {
    pub heap: BaseHeap,
    /// The value of any arguments that were mutably borrowed.
    /// Non-mut borrowed values are not included in the domain of this map.
    /// Mapping is from argument index to the heap location
    pub heap_mut_refs: BTreeMap<u16, BaseHeapId>,
    /// The value of any arguments that were immutably borrowed.
    /// Mutably borrowed values are not included in the domain of this map.
    /// Mapping is from argument index to the heap location
    pub heap_imm_refs: BTreeMap<u16, BaseHeapId>,
    /// The values passed in with any references taken.
    pub values: Vec<Value>,
}

// -------------------------------------------------------------------------------------------------
// Value Serialization and Deserialization
// -------------------------------------------------------------------------------------------------

impl ValueFrame {
    pub fn serialized_call(
        vm: &mut MoveVM<'_>,
        runtime_id: &ModuleId,
        function_name: &IdentStr,
        ty_args: Vec<Type>,
        serialized_args: Vec<impl Borrow<[u8]>>,
        gas_meter: &mut impl GasMeter,
        tracer: Option<&mut MoveTraceBuilder>,
        bypass_declared_entry_check: bool,
    ) -> VMResult<Self> {
        let mut frame = Self {
            heap: BaseHeap::new(),
            heap_mut_refs: BTreeMap::new(),
            heap_imm_refs: BTreeMap::new(),
            values: vec![],
        };
        let fun = vm.find_function(runtime_id, function_name, &ty_args)?;
        let arg_types = fun
            .parameters
            .into_iter()
            .map(|ty| ty.subst(&ty_args))
            .collect::<PartialVMResult<Vec<_>>>()
            .map_err(|err| err.finish(Location::Undefined))?;
        frame
            .deserialize_args(&vm.virtual_tables, arg_types, serialized_args)
            .map_err(|e| e.finish(Location::Undefined))?;
        let return_values = if bypass_declared_entry_check {
            vm.execute_function_bypass_visibility(
                runtime_id,
                function_name,
                ty_args,
                frame.values,
                gas_meter,
                tracer,
            )?
        } else {
            debug_assert!(tracer.is_none());
            vm.execute_entry_function(runtime_id, function_name, ty_args, frame.values, gas_meter)?
        };
        frame.values = return_values;
        Ok(frame)
    }

    /// Returns the list of mutable references plus the vector of values.
    fn deserialize_args(
        &mut self,
        vtables: &VMDispatchTables,
        arg_tys: Vec<Type>,
        serialized_args: Vec<impl Borrow<[u8]>>,
    ) -> PartialVMResult<()> {
        if arg_tys.len() != serialized_args.len() {
            return Err(
                PartialVMError::new(StatusCode::NUMBER_OF_ARGUMENTS_MISMATCH).with_message(
                    format!(
                        "argument length mismatch: expected {} got {}",
                        arg_tys.len(),
                        serialized_args.len()
                    ),
                ),
            );
        }

        // Arguments for the invoked function. These can be owned values or references
        let deserialized_args = arg_tys
            .into_iter()
            .zip(serialized_args)
            .enumerate()
            .map(|(idx, (arg_ty, arg_bytes))| match &arg_ty {
                Type::MutableReference(inner_t) | Type::Reference(inner_t) => {
                    // Each ref-arg value stored on the base heap, borrowed, and passed by
                    // reference to the invoked function.
                    let (ndx, value) = self
                        .heap
                        .allocate_and_borrow_loc(deserialize_value(vtables, inner_t, arg_bytes)?)?;
                    match arg_ty {
                        Type::Reference(_) => {
                            // Record the immutable reference in the map
                            assert!(self.heap_imm_refs.insert(idx as u16, ndx).is_none());
                            assert!(!self.heap_mut_refs.contains_key(&(idx as u16)));
                        }
                        Type::MutableReference(_) => {
                            // Record the mutable reference in the map
                            assert!(self.heap_mut_refs.insert(idx as u16, ndx).is_none());
                            assert!(!self.heap_imm_refs.contains_key(&(idx as u16)));
                        }
                        _ => unreachable!(),
                    }
                    Ok(value)
                }
                _ => deserialize_value(vtables, &arg_ty, arg_bytes),
            })
            .collect::<PartialVMResult<Vec<_>>>()?;
        self.values.extend(deserialized_args);
        Ok(())
    }
}

fn deserialize_value(
    vtables: &VMDispatchTables,
    ty: &Type,
    arg: impl Borrow<[u8]>,
) -> PartialVMResult<Value> {
    let layout = match vtables.type_to_type_layout(ty) {
        Ok(layout) => layout,
        Err(_err) => {
            warn!("[VM] failed to get layout from type");
            return Err(PartialVMError::new(
                StatusCode::INVALID_PARAM_TYPE_FOR_DESERIALIZATION,
            ));
        }
    };

    match Value::simple_deserialize(arg.borrow(), &layout) {
        Some(val) => Ok(val),
        None => {
            warn!("[VM] failed to deserialize argument");
            Err(PartialVMError::new(
                StatusCode::FAILED_TO_DESERIALIZE_ARGUMENT,
            ))
        }
    }
}
