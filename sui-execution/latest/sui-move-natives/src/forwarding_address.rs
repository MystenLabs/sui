// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::VecDeque;

use move_binary_format::{
    checked_as,
    errors::{PartialVMError, PartialVMResult},
    safe_assert_eq, safe_unwrap,
};
use move_core_types::{
    account_address::AccountAddress, gas_algebra::InternalGas, language_storage::TypeTag,
    vm_status::StatusCode,
};
use move_vm_runtime::{
    execution::values::Value,
    native_charge_gas_early_exit,
    natives::functions::{NativeContext, NativeResult},
    pop_arg,
};
use smallvec::smallvec;
use sui_types::{
    SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
    forwarding_address::{ForwardingAddress, MasterRecordKey},
};

use crate::{
    NativesCostTable, get_extension, get_extension_mut,
    object_runtime::{
        ObjectRuntime,
        object_store::{CacheInfo, ObjectResult},
    },
};

const E_FORWARDING_ADDRESS_UNREGISTERED: u64 = 1;
const E_FORWARDING_ADDRESS_VARIANT_UNSUPPORTED: u64 = 2;

#[derive(Clone)]
pub struct ForwardingAddressResolveCostParams {
    pub base: Option<InternalGas>,
    pub per_byte: Option<InternalGas>,
}

pub fn resolve_impl(
    context: &mut NativeContext,
    ty_args: Vec<move_vm_runtime::execution::Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    safe_assert_eq!(ty_args.len(), 0);
    safe_assert_eq!(args.len(), 1);

    let recipient = pop_arg!(args, AccountAddress);
    let protocol_config = get_extension!(context, ObjectRuntime)?.protocol_config;
    if !protocol_config.enable_forwarding_addresses() {
        return Ok(not_forwarded(context, recipient));
    }
    let max_variant = safe_unwrap!(protocol_config.forwarding_address_max_variant_as_option());

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
    if u64::from(forwarding_address.variant) > max_variant {
        return Ok(NativeResult::err(
            context.gas_used(),
            E_FORWARDING_ADDRESS_VARIANT_UNSUPPORTED,
        ));
    }

    let registry = get_extension_mut!(context, ObjectRuntime)?
        .load_implicitly_read_system_object(&SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID)?;
    native_charge_gas_early_exit!(
        context,
        per_byte * checked_as!(registry.object_size_for_gas_metering(), u64)?.into()
    );

    let record = MasterRecordKey(forwarding_address.master_id);
    let field_id = record.object_id().map_err(|error| {
        PartialVMError::new(StatusCode::VALUE_SERIALIZATION_ERROR).with_message(format!(
            "failed to derive forwarding registry field ID: {error}"
        ))
    })?;
    let field_type = MasterRecordKey::object_type();
    let type_tag = TypeTag::from(field_type.clone());
    let layout = context
        .type_tag_to_type_layout(&type_tag)
        .ok_or_else(|| invariant_violation("forwarding registry field layout is unavailable"))?;
    let annotated_layout = context
        .type_tag_to_annotated_type_layout(&type_tag)
        .ok_or_else(|| {
            invariant_violation("forwarding registry field annotated layout is unavailable")
        })?;

    let (cache_info, contents) = match get_extension_mut!(context, ObjectRuntime)?
        .load_child_object_bytes(
            registry.id(),
            field_id,
            &layout,
            &annotated_layout,
            field_type,
        )? {
        ObjectResult::MismatchedType => {
            return Err(invariant_violation(
                "forwarding registry field has an unexpected type",
            ));
        }
        ObjectResult::Loaded(loaded) => loaded,
    };
    let Some(contents) = contents else {
        return Ok(NativeResult::err(
            context.gas_used(),
            E_FORWARDING_ADDRESS_UNREGISTERED,
        ));
    };
    let master = record
        .decode(&contents)
        .map_err(|error| {
            invariant_violation(&format!("corrupt forwarding master record: {error}"))
        })?
        .master;
    if let CacheInfo::Loaded(Some(size)) = cache_info {
        native_charge_gas_early_exit!(context, per_byte * checked_as!(size, u64)?.max(1).into());
    }

    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![
            Value::address(AccountAddress::new(master.to_inner())),
            Value::bool(true),
        ],
    ))
}

fn not_forwarded(context: &NativeContext, recipient: AccountAddress) -> NativeResult {
    NativeResult::ok(
        context.gas_used(),
        smallvec![Value::address(recipient), Value::bool(false)],
    )
}

fn invariant_violation(message: &str) -> PartialVMError {
    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
        .with_message(message.to_owned())
}
