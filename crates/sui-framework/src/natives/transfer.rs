// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::legacy_emit_cost;
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::{
    account_address::AccountAddress, language_storage::TypeTag, vm_status::StatusCode,
};
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    loaded_data::runtime_types::Type, natives::function::NativeResult, pop_arg, values::Value,
};
use smallvec::smallvec;
use std::collections::VecDeque;
use sui_types::object::Owner;

use super::object_runtime::ObjectRuntime;
/// Implementation of Move native function
/// `transfer_internal<T: key>(obj: T, recipient: vector<u8>, to_object: bool)`
/// Here, we simply emit this event. The sui adapter
/// treats this as a special event that is handled
/// differently from user events:
/// the adapter will change the owner of the object
/// in question to `recipient`.
pub fn transfer_internal(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 3);

    let ty = ty_args.pop().unwrap();
    let to_object = pop_arg!(args, bool);
    let recipient = pop_arg!(args, AccountAddress);
    let obj = args.pop_back().unwrap();
    let owner = if to_object {
        Owner::ObjectOwner(recipient.into())
    } else {
        Owner::AddressOwner(recipient.into())
    };
    object_runtime_transfer(context, owner, ty, obj)?;
    // Charge a constant native gas cost here, since
    // we will charge it properly when processing
    // all the events in adapter.
    // TODO: adjust native_gas cost size base.
    let cost = legacy_emit_cost();
    Ok(NativeResult::ok(cost, smallvec![]))
}

/// Implementation of Move native function
/// `freeze_object<T: key>(obj: T)`
pub fn freeze_object(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 1);

    let ty = ty_args.pop().unwrap();
    let obj = args.pop_back().unwrap();
    object_runtime_transfer(context, Owner::Immutable, ty, obj)?;
    let cost = legacy_emit_cost();
    Ok(NativeResult::ok(cost, smallvec![]))
}

/// Implementation of Move native function
/// `share_object<T: key>(obj: T)`
pub fn share_object(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 1);

    let ty = ty_args.pop().unwrap();
    let obj = args.pop_back().unwrap();
    object_runtime_transfer(context, Owner::Shared, ty, obj)?;
    let cost = legacy_emit_cost();
    Ok(NativeResult::ok(cost, smallvec![]))
}

fn object_runtime_transfer(
    context: &mut NativeContext,
    owner: Owner,
    ty: Type,
    obj: Value,
) -> PartialVMResult<()> {
    let tag = match context.type_to_type_tag(&ty)? {
        TypeTag::Struct(s) => s,
        _ => {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Sui verifier guarantees this is a struct".to_string()),
            )
        }
    };
    let obj_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    obj_runtime.transfer(owner, ty, tag, obj)?;
    Ok(())
}
