// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::VecDeque;

use move_binary_format::{errors::PartialVMResult, safe_assert_eq, safe_unwrap};
use move_core_types::{account_address::AccountAddress, gas_algebra::InternalGas};
use move_vm_runtime::{
    execution::values::Value,
    native_charge_gas_early_exit,
    natives::functions::{NativeContext, NativeResult},
    pop_arg,
};
use smallvec::smallvec;
use sui_types::forwarding_address::ForwardingAddress;

use crate::{NativesCostTable, get_extension, object_runtime::ObjectRuntime};

const E_FORWARDING_ADDRESS_UNREGISTERED: u64 = 1;
const E_FORWARDING_ADDRESS_VARIANT_UNSUPPORTED: u64 = 2;

#[derive(Clone)]
pub struct ForwardingAddressResolveCostParams {
    pub base: Option<InternalGas>,
    pub lookup: Option<InternalGas>,
}

#[derive(Clone)]
pub struct ForwardingAddressRegisterCostParams {
    pub base: Option<InternalGas>,
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

    let ForwardingAddressResolveCostParams { base, lookup } =
        get_extension!(context, NativesCostTable)?
            .forwarding_address_resolve_cost_params
            .clone();
    native_charge_gas_early_exit!(context, safe_unwrap!(base));

    let Some(forwarding_address) = ForwardingAddress::parse(recipient.into()) else {
        return Ok(not_forwarded(context, recipient));
    };
    if u64::from(forwarding_address.variant) > max_variant {
        return Ok(NativeResult::err(
            context.gas_used(),
            E_FORWARDING_ADDRESS_VARIANT_UNSUPPORTED,
        ));
    }

    native_charge_gas_early_exit!(context, safe_unwrap!(lookup));
    let Some(master) =
        get_extension!(context, ObjectRuntime)?.forwarding_master(forwarding_address.master_id)?
    else {
        return Ok(NativeResult::err(
            context.gas_used(),
            E_FORWARDING_ADDRESS_UNREGISTERED,
        ));
    };

    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![
            Value::address(AccountAddress::new(master.to_inner())),
            Value::bool(true),
        ],
    ))
}

/// Charges the registration price, which exists to make allocating master IDs expensive.
pub fn charge_registration_fee(
    context: &mut NativeContext,
    ty_args: Vec<move_vm_runtime::execution::Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    safe_assert_eq!(ty_args.len(), 0);
    safe_assert_eq!(args.len(), 0);

    let ForwardingAddressRegisterCostParams { base } = get_extension!(context, NativesCostTable)?
        .forwarding_address_register_cost_params
        .clone();
    native_charge_gas_early_exit!(context, safe_unwrap!(base));

    Ok(NativeResult::ok(context.gas_used(), smallvec![]))
}

fn not_forwarded(context: &NativeContext, recipient: AccountAddress) -> NativeResult {
    NativeResult::ok(
        context.gas_used(),
        smallvec![Value::address(recipient), Value::bool(false)],
    )
}
