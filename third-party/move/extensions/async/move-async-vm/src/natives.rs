// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::async_vm::Message;
use better_any::{Tid, TidAble};
use move_binary_format::errors::PartialVMResult;
use move_core_types::{
    account_address::AccountAddress,
    gas_algebra::{InternalGas, InternalGasPerByte, NumBytes},
    identifier::Identifier,
};
use move_vm_runtime::{
    native_functions,
    native_functions::{NativeContext, NativeFunction},
};
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    pop_arg,
    values::{Value, Vector},
};
use smallvec::smallvec;
use std::{collections::VecDeque, sync::Arc};

/// Environment extension for the Move VM which we pass down to native functions,
/// to implement message sending and retrieval of actor address.
#[derive(Tid)]
pub struct AsyncExtension {
    pub current_actor: AccountAddress,
    pub sent: Vec<Message>,
    pub virtual_time: u128,
    pub in_initializer: bool,
}

#[derive(Clone, Debug)]
pub struct GasParameters {
    pub self_: SelfGasParameters,
    pub send: SendGasParameters,
    pub virtual_time: VirtualTimeGasParameters,
}

impl GasParameters {
    pub fn zeros() -> Self {
        Self {
            self_: SelfGasParameters {
                base_cost: 0.into(),
            },
            send: SendGasParameters {
                base_cost: 0.into(),
                unit_cost: 0.into(),
            },
            virtual_time: VirtualTimeGasParameters {
                base_cost: 0.into(),
            },
        }
    }
}

pub fn actor_natives(
    async_addr: AccountAddress,
    gas_params: GasParameters,
) -> Vec<(AccountAddress, Identifier, Identifier, NativeFunction)> {
    let natives = [
        ("Actor", "self", make_native_self(gas_params.self_)),
        (
            "Actor",
            "virtual_time",
            make_native_virtual_time(gas_params.virtual_time),
        ),
        (
            "Runtime",
            "send__0",
            make_native_send(gas_params.send.clone()),
        ),
        (
            "Runtime",
            "send__1",
            make_native_send(gas_params.send.clone()),
        ),
        (
            "Runtime",
            "send__2",
            make_native_send(gas_params.send.clone()),
        ),
        (
            "Runtime",
            "send__3",
            make_native_send(gas_params.send.clone()),
        ),
        (
            "Runtime",
            "send__4",
            make_native_send(gas_params.send.clone()),
        ),
        (
            "Runtime",
            "send__5",
            make_native_send(gas_params.send.clone()),
        ),
        (
            "Runtime",
            "send__6",
            make_native_send(gas_params.send.clone()),
        ),
        (
            "Runtime",
            "send__7",
            make_native_send(gas_params.send.clone()),
        ),
        ("Runtime", "send__8", make_native_send(gas_params.send)),
    ];
    native_functions::make_table_from_iter(async_addr, natives)
}

#[derive(Clone, Debug)]
pub struct SelfGasParameters {
    base_cost: InternalGas,
}

fn native_self(
    gas_params: &SelfGasParameters,
    context: &mut NativeContext,
    mut _ty_args: Vec<Type>,
    mut _args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    let ext = context.extensions().get::<AsyncExtension>();
    Ok(NativeResult::ok(
        gas_params.base_cost,
        smallvec![Value::address(ext.current_actor)],
    ))
}

fn make_native_self(gas_params: SelfGasParameters) -> NativeFunction {
    Arc::new(move |context, ty_args, args| native_self(&gas_params, context, ty_args, args))
}

#[derive(Clone, Debug)]
pub struct SendGasParameters {
    base_cost: InternalGas,
    unit_cost: InternalGasPerByte,
}

fn native_send(
    gas_params: &SendGasParameters,
    context: &mut NativeContext,
    mut _ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    let ext = context.extensions_mut().get_mut::<AsyncExtension>();
    let mut bcs_args = vec![];
    while args.len() > 2 {
        bcs_args.push(pop_arg!(args, Vector).to_vec_u8()?);
    }
    bcs_args.reverse();
    let message_hash = pop_arg!(args, u64);
    let target = pop_arg!(args, AccountAddress);
    ext.sent.push((target, message_hash, bcs_args));

    let cost = gas_params.base_cost + gas_params.unit_cost * NumBytes::new(args.len() as u64);

    Ok(NativeResult::ok(cost, smallvec![]))
}

fn make_native_send(gas_params: SendGasParameters) -> NativeFunction {
    Arc::new(move |context, ty_args, args| native_send(&gas_params, context, ty_args, args))
}

#[derive(Clone, Debug)]
pub struct VirtualTimeGasParameters {
    base_cost: InternalGas,
}

fn native_virtual_time(
    gas_params: &VirtualTimeGasParameters,
    context: &mut NativeContext,
    mut _ty_args: Vec<Type>,
    mut _args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    let ext = context.extensions().get::<AsyncExtension>();
    Ok(NativeResult::ok(
        gas_params.base_cost,
        smallvec![Value::u128(ext.virtual_time)],
    ))
}

fn make_native_virtual_time(gas_params: VirtualTimeGasParameters) -> NativeFunction {
    Arc::new(move |context, ty_args, args| native_virtual_time(&gas_params, context, ty_args, args))
}
