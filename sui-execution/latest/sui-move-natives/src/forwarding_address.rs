// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::VecDeque;

use move_binary_format::{checked_as, safe_assert_eq, safe_unwrap};
use move_core_types::{
    account_address::AccountAddress, gas_algebra::InternalGas, language_storage::TypeTag,
    vm_status::StatusCode,
};
use move_vm_runtime::{
    execution::values::{Reference, Value},
    native_charge_gas_early_exit,
    natives::functions::{NativeContext, NativeResult},
    pop_arg,
};
use smallvec::smallvec;
use sui_types::{
    SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
    base_types::MoveObjectType,
    dynamic_field::{DynamicFieldInfo, derive_dynamic_field_id},
    forwarding_address::ForwardingAddress,
};

use crate::{
    NativesCostTable, get_extension, get_extension_mut, get_nested_struct_field,
    object_runtime::{
        ObjectRuntime,
        object_store::{CacheInfo, ObjectResult},
    },
};

const E_FORWARDING_ADDRESS_UNREGISTERED: u64 = 1;

#[derive(Clone)]
pub struct ForwardingAddressResolveCostParams {
    pub base: Option<InternalGas>,
    pub per_byte: Option<InternalGas>,
}

pub fn resolve_impl(
    context: &mut NativeContext,
    ty_args: Vec<move_vm_runtime::execution::Type>,
    mut args: VecDeque<Value>,
) -> move_binary_format::errors::PartialVMResult<NativeResult> {
    safe_assert_eq!(ty_args.len(), 0);
    safe_assert_eq!(args.len(), 1);

    let recipient = pop_arg!(args, AccountAddress);
    if !get_extension!(context, ObjectRuntime)?
        .protocol_config
        .enable_forwarding_addresses()
    {
        return Ok(not_forwarded(context, recipient));
    }

    let ForwardingAddressResolveCostParams { base, per_byte } =
        get_extension!(context, NativesCostTable)?
            .forwarding_address_resolve_cost_params
            .clone();
    let base = safe_unwrap!(base);
    let per_byte = safe_unwrap!(per_byte);
    native_charge_gas_early_exit!(context, base);

    let Some(forwarding_address) = ForwardingAddress::parse(recipient.into()) else {
        return Ok(not_forwarded(context, recipient));
    };

    let Some(registry_object) = get_extension_mut!(context, ObjectRuntime)?
        .load_runtime_system_object(&SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID)
    else {
        // Direct protocol-version jumps can enable forwarding one epoch before the registry is
        // created. Reject reserved forwarding addresses during that transition rather than
        // transferring funds to an address the master does not control.
        return Ok(NativeResult::err(
            context.gas_used(),
            E_FORWARDING_ADDRESS_UNREGISTERED,
        ));
    };
    native_charge_gas_early_exit!(
        context,
        per_byte * checked_as!(registry_object.object_size_for_gas_metering(), u64)?.into()
    );

    let key_type = TypeTag::U64;
    let field_type = DynamicFieldInfo::dynamic_field_type(key_type.clone(), TypeTag::Address);
    let type_tag = TypeTag::Struct(Box::new(field_type.clone()));
    let layout = context.type_tag_to_type_layout(&type_tag).ok_or_else(|| {
        move_binary_format::errors::PartialVMError::new(
            StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
        )
        .with_message("forwarding address registry field layout is unavailable")
    })?;
    let annotated_layout = context
        .type_tag_to_annotated_type_layout(&type_tag)
        .ok_or_else(|| {
            move_binary_format::errors::PartialVMError::new(
                StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
            )
            .with_message("forwarding address registry field annotated layout is unavailable")
        })?;
    let child_id = derive_dynamic_field_id(
        registry_object.id(),
        &key_type,
        &bcs::to_bytes(&forwarding_address.master_id).unwrap(),
    )
    .map_err(|error| {
        move_binary_format::errors::PartialVMError::new(StatusCode::VALUE_SERIALIZATION_ERROR)
            .with_message(format!(
                "failed to derive forwarding registry field ID: {error}"
            ))
    })?;

    let (cache_info, master) = {
        let object_runtime: &mut ObjectRuntime = get_extension_mut!(context)?;
        let (cache_info, field) = match object_runtime.get_or_fetch_child_object(
            registry_object.id(),
            child_id,
            &layout,
            &annotated_layout,
            MoveObjectType::from(field_type),
        )? {
            ObjectResult::MismatchedType => {
                return Err(move_binary_format::errors::PartialVMError::new(
                    StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
                )
                .with_message("forwarding address registry field has an unexpected type"));
            }
            ObjectResult::Loaded(field) => field,
        };
        let master = if field.exists()? {
            let field_value = field.borrow_global()?.value_as::<Reference>()?.read_ref()?;
            Some(get_nested_struct_field(field_value, &[2])?.value_as::<AccountAddress>()?)
        } else {
            None
        };
        (cache_info, master)
    };
    let Some(master) = master else {
        return Ok(NativeResult::err(
            context.gas_used(),
            E_FORWARDING_ADDRESS_UNREGISTERED,
        ));
    };
    if let CacheInfo::Loaded(Some(size)) = cache_info {
        native_charge_gas_early_exit!(context, per_byte * checked_as!(size, u64)?.max(1).into());
    }

    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![
            Value::address(master),
            Value::u128(forwarding_address.tag),
            Value::bool(true),
        ],
    ))
}

fn not_forwarded(context: &NativeContext, recipient: AccountAddress) -> NativeResult {
    NativeResult::ok(
        context.gas_used(),
        smallvec![
            Value::address(recipient),
            Value::u128(0),
            Value::bool(false)
        ],
    )
}
