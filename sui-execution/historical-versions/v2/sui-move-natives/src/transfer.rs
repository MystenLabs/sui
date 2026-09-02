// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::object_runtime::{ObjectRuntime, TransferResult};
use crate::{
    get_receiver_object_id, get_tag_and_layouts, object_runtime::object_store::ObjectResult,
    NativesCostTable,
};
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::{
    account_address::AccountAddress, gas_algebra::InternalGas, language_storage::TypeTag,
    vm_status::StatusCode,
};
use move_vm_runtime::{native_charge_gas_early_exit, native_functions::NativeContext};
use move_vm_types::{
    loaded_data::runtime_types::Type, natives::function::NativeResult, pop_arg, values::Value,
};
use smallvec::smallvec;
use std::collections::VecDeque;
use sui_types::{
    base_types::{MoveObjectType, ObjectID, SequenceNumber},
    object::Owner,
};

const E_SHARED_NON_NEW_OBJECT: u64 = 0;
const E_BCS_SERIALIZATION_FAILURE: u64 = 1;
const E_RECEIVING_OBJECT_TYPE_MISMATCH: u64 = 2;
// Represents both the case where the object does not exist and the case where the object is not
// able to be accessed through the parent that is passed-in.
const E_UNABLE_TO_RECEIVE_OBJECT: u64 = 3;

#[derive(Clone, Debug)]
pub struct TransferReceiveObjectInternalCostParams {
    pub transfer_receive_object_internal_cost_base: InternalGas,
}
/***************************************************************************************************
* native fun receive_object_internal
* Implementation of the Move native function `receive_object_internal<T: key>(parent: &mut UID, rec: Receiver<T>): T`
*   gas cost: transfer_receive_object_internal_cost_base |  covers various fixed costs in the oper
**************************************************************************************************/

pub fn receive_object_internal(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 3);
    let transfer_receive_object_internal_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()
        .transfer_receive_object_internal_cost_params
        .clone();
    native_charge_gas_early_exit!(
        context,
        transfer_receive_object_internal_cost_params.transfer_receive_object_internal_cost_base
    );
    let child_ty = ty_args.pop().unwrap();
    let child_receiver_sequence_number: SequenceNumber = pop_arg!(args, u64).into();
    let child_receiver_object_id = args.pop_back().unwrap();
    let parent = pop_arg!(args, AccountAddress).into();
    assert!(args.is_empty());
    let child_id: ObjectID = get_receiver_object_id(child_receiver_object_id.copy_value().unwrap())
        .unwrap()
        .value_as::<AccountAddress>()
        .unwrap()
        .into();
    assert!(ty_args.is_empty());

    let Some((tag, layout, annotated_layout)) = get_tag_and_layouts(context, &child_ty)? else {
        return Ok(NativeResult::err(
            context.gas_used(),
            E_BCS_SERIALIZATION_FAILURE,
        ));
    };

    let object_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    let child = match object_runtime.receive_object(
        parent,
        child_id,
        child_receiver_sequence_number,
        &child_ty,
        &layout,
        &annotated_layout,
        MoveObjectType::from(tag),
    ) {
        // NB: Loaded and doesn't exist and inauthenticated read should lead to the exact same error
        Ok(None) => {
            return Ok(NativeResult::err(
                context.gas_used(),
                E_UNABLE_TO_RECEIVE_OBJECT,
            ))
        }
        Ok(Some(ObjectResult::Loaded(gv))) => gv,
        Ok(Some(ObjectResult::MismatchedType)) => {
            return Ok(NativeResult::err(
                context.gas_used(),
                E_RECEIVING_OBJECT_TYPE_MISMATCH,
            ))
        }
        Err(x) => return Err(x),
    };

    Ok(NativeResult::ok(context.gas_used(), smallvec![child]))
}

#[derive(Clone, Debug)]
pub struct TransferInternalCostParams {
    pub transfer_transfer_internal_cost_base: InternalGas,
}
/***************************************************************************************************
* native fun transfer_impl
* Implementation of the Move native function `transfer_impl<T: key>(obj: T, recipient: address)`
*   gas cost: transfer_transfer_internal_cost_base                  |  covers various fixed costs in the oper
**************************************************************************************************/
pub fn transfer_internal(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 2);

    let transfer_transfer_internal_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()
        .transfer_transfer_internal_cost_params
        .clone();

    native_charge_gas_early_exit!(
        context,
        transfer_transfer_internal_cost_params.transfer_transfer_internal_cost_base
    );

    let ty = ty_args.pop().unwrap();
    let recipient = pop_arg!(args, AccountAddress);
    let obj = args.pop_back().unwrap();

    let owner = Owner::AddressOwner(recipient.into());
    object_runtime_transfer(context, owner, ty, obj)?;
    let cost = context.gas_used();
    Ok(NativeResult::ok(cost, smallvec![]))
}

#[derive(Clone, Debug)]
pub struct TransferFreezeObjectCostParams {
    pub transfer_freeze_object_cost_base: InternalGas,
}
/***************************************************************************************************
* native fun freeze_object
* Implementation of the Move native function `freeze_object<T: key>(obj: T)`
*   gas cost: transfer_freeze_object_cost_base                  |  covers various fixed costs in the oper
**************************************************************************************************/
pub fn freeze_object(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 1);

    let transfer_freeze_object_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()
        .transfer_freeze_object_cost_params
        .clone();

    native_charge_gas_early_exit!(
        context,
        transfer_freeze_object_cost_params.transfer_freeze_object_cost_base
    );

    let ty = ty_args.pop().unwrap();
    let obj = args.pop_back().unwrap();

    object_runtime_transfer(context, Owner::Immutable, ty, obj)?;

    Ok(NativeResult::ok(context.gas_used(), smallvec![]))
}

#[derive(Clone, Debug)]
pub struct TransferShareObjectCostParams {
    pub transfer_share_object_cost_base: InternalGas,
}
/***************************************************************************************************
* native fun share_object
* Implementation of the Move native function `share_object<T: key>(obj: T)`
*   gas cost: transfer_share_object_cost_base                  |  covers various fixed costs in the oper
**************************************************************************************************/
pub fn share_object(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 1);

    let transfer_share_object_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()
        .transfer_share_object_cost_params
        .clone();

    native_charge_gas_early_exit!(
        context,
        transfer_share_object_cost_params.transfer_share_object_cost_base
    );

    let ty = ty_args.pop().unwrap();
    let obj = args.pop_back().unwrap();
    let transfer_result = object_runtime_transfer(
        context,
        // Dummy version, to be filled with the correct initial version when the effects of the
        // transaction are written to storage.
        Owner::Shared {
            initial_shared_version: SequenceNumber::new(),
        },
        ty,
        obj,
    )?;
    let cost = context.gas_used();
    Ok(match transfer_result {
        // New means the ID was created in this transaction
        // SameOwner means the object was previously shared and was re-shared
        TransferResult::New | TransferResult::SameOwner => NativeResult::ok(cost, smallvec![]),
        TransferResult::OwnerChanged => NativeResult::err(cost, E_SHARED_NON_NEW_OBJECT),
    })
}

fn object_runtime_transfer(
    context: &mut NativeContext,
    owner: Owner,
    ty: Type,
    obj: Value,
) -> PartialVMResult<TransferResult> {
    if !matches!(context.type_to_type_tag(&ty)?, TypeTag::Struct(_)) {
        return Err(
            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                .with_message("Sui verifier guarantees this is a struct".to_string()),
        );
    }

    let obj_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    obj_runtime.transfer(owner, ty, obj)
}
