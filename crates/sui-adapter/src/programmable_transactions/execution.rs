// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use move_core_types::resolver::{ModuleResolver, ResourceResolver};
use move_vm_runtime::move_vm::MoveVM;
use sui_cost_tables::bytecode_tables::GasStatus;
use sui_types::{
    balance::Balance,
    base_types::{ObjectID, SuiAddress, TxContext},
    coin::Coin,
    error::ExecutionError,
    id::UID,
    messages::{Command, ProgrammableTransaction},
    storage::{ChildObjectResolver, ParentSync, Storage},
};

use super::{context::*, types::*};

pub fn execute<
    E: fmt::Debug,
    S: ResourceResolver<Error = E>
        + ModuleResolver<Error = E>
        + Storage
        + ParentSync
        + ChildObjectResolver,
>(
    vm: &MoveVM,
    state_view: &mut S,
    ctx: &mut TxContext,
    gas_status: &mut GasStatus,
    gas_coin: ObjectID,
    pt: ProgrammableTransaction,
) -> Result<(), ExecutionError> {
    let ProgrammableTransaction { inputs, commands } = pt;
    let mut context = ExecutionContext::new(vm, state_view, ctx, gas_status, gas_coin, inputs)?;
    for command in commands {
        execute_command(&mut context, command)?;
    }
    Ok(())
}

fn execute_command<
    E: fmt::Debug,
    S: ResourceResolver<Error = E>
        + ModuleResolver<Error = E>
        + Storage
        + ParentSync
        + ChildObjectResolver,
>(
    context: &mut ExecutionContext<E, S>,
    command: Command,
) -> Result<(), ExecutionError> {
    let is_transfer_objects = matches!(command, Command::TransferObjects(_, _));
    let results = match command {
        Command::TransferObjects(objs, addr_arg) => {
            let objs: Vec<ObjectValue> = context.take_args(objs)?;
            let addr: SuiAddress = context.take_arg(addr_arg)?;
            for obj in objs {
                obj.ensure_public_transfer_eligible()?;
                context.transfer_object(obj, addr)?;
            }
            vec![]
        }
        Command::SplitCoin(coin_arg, amount_arg) => {
            let mut obj: ObjectValue = context.take_arg(coin_arg)?;
            let ObjectContents::Coin(coin) = &mut obj.contents else {
                panic!("not a coin")
            };
            let amount: u64 = context.take_arg(amount_arg)?;
            let new_coin_id = context.fresh_id()?;
            let new_coin = coin.split_coin(amount, UID::new(new_coin_id))?;
            let coin_type = obj.type_.clone();
            context.restore_arg(coin_arg, Value::Object(obj))?;
            vec![Some(Value::Object(ObjectValue::coin(coin_type, new_coin)?))]
        }
        Command::MergeCoins(target_arg, coin_args) => {
            let mut target: ObjectValue = context.take_arg(target_arg)?;
            let ObjectContents::Coin(target_coin) = &mut target.contents else {
                panic!("not a coin")
            };
            let coins: Vec<ObjectValue> = context.take_args(coin_args)?;
            for coin in coins {
                let ObjectContents::Coin(Coin { id, balance }) = coin.contents else {
                    panic!("not a coin")
                };
                context.delete_id(*id.object_id())?;
                let Some(new_value) = target_coin.balance.value().checked_add(balance.value())
                    else {
                        panic!("coin overflow")
                    };
                target_coin.balance = Balance::new(new_value);
            }
            context.restore_arg(target_arg, Value::Object(target))?;
            vec![]
        }
        Command::MoveCall(_) => todo!(),
        Command::Publish(_) => todo!(),
    };
    context.results.push(results);
    if !is_transfer_objects && context.gas.is_none() {
        panic!("todo gas taken error")
    }
    Ok(())
}
