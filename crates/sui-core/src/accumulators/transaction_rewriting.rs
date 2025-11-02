// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use sui_types::base_types::SuiAddress;
use sui_types::coin::{
    COIN_MODULE_NAME, COIN_REDEEM_FUNDS_FUNCTION_NAME, COIN_SEND_FUNDS_FUNCTION_NAME,
};
use sui_types::coin_reservation::{self, CoinReservationResolverTrait};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{
    Argument, CallArg, Command, ObjectArg, ProgrammableMoveCall, ProgrammableTransaction,
    TransactionKind, WithdrawalTypeArg,
};

pub fn rewrite_transaction_for_coin_reservations(
    coin_reservation_resolver: &dyn CoinReservationResolverTrait,
    sender: SuiAddress,
    transaction_kind: TransactionKind,
) -> TransactionKind {
    match transaction_kind {
        TransactionKind::ProgrammableTransaction(pt) => TransactionKind::ProgrammableTransaction(
            rewrite_programmable_transaction_for_coin_reservations(
                coin_reservation_resolver,
                sender,
                pt,
            ),
        ),
        _ => transaction_kind,
    }
}

fn rewrite_programmable_transaction_for_coin_reservations(
    coin_reservation_resolver: &dyn CoinReservationResolverTrait,
    sender: SuiAddress,
    pt: ProgrammableTransaction,
) -> ProgrammableTransaction {
    // TODO:
    // - For each input in ProgrammableTransaction
    //   - check if it is a coin reservation with `coin_reservation::is_coin_reservation_digest`
    //   - if it is, resolve the coin reservation to a FundsWithdrawalArg with coin_reservation_resolver.resolve_funds_withdrawal
    //   - record the input index and the resolved FundsWithdrawalArg

    let mut builder = ProgrammableTransactionBuilder::new();

    let mut rewritten_inputs = BTreeMap::new();
    let mut ephemeral_coins = Vec::new();

    for (index, input) in pt.inputs.into_iter().enumerate() {
        let index: u16 = index.try_into().expect("too many inputs");
        match input {
            CallArg::Object(ObjectArg::ImmOrOwnedObject(object_ref))
                if coin_reservation::is_coin_reservation_digest(&object_ref.2) =>
            {
                let withdraw = coin_reservation_resolver
                    .resolve_funds_withdrawal(sender, object_ref)
                    .unwrap();

                // type_input is T as in Balance<T>
                let balance_type_input = match &withdraw.type_arg {
                    WithdrawalTypeArg::Balance(type_input) => type_input.clone(),
                };

                let withdraw_arg = builder
                    .funds_withdrawal(withdraw.clone())
                    .expect("failed to add withdrawal");

                // redeem the withdrawal
                let coin_result =
                    builder.command(Command::MoveCall(Box::new(ProgrammableMoveCall {
                        package: SUI_FRAMEWORK_PACKAGE_ID,
                        module: COIN_MODULE_NAME.to_string(),
                        function: COIN_REDEEM_FUNDS_FUNCTION_NAME.to_string(),
                        type_arguments: vec![balance_type_input.clone()],
                        arguments: vec![withdraw_arg],
                    })));

                ephemeral_coins.push((balance_type_input, coin_result));

                // any command that refers to the coin reservation should now refer to the ephemeral coin
                rewritten_inputs.insert(index, coin_result);
            }
            input => {
                builder.input(input).unwrap();
            }
        }
    }

    // how much we need to offset any result arguments by
    // both expects are safe because we reject any transaction where
    // num_commands + coin_reservation_obj_refs.len() * 2 > u16::MAX
    let num_commands: u16 = builder
        .num_commands()
        .try_into()
        .expect("too many commands");
    let offset_result = |result: u16| result.checked_add(num_commands).expect("too many commands");

    let fixup_arg = |arg: Argument| match arg {
        Argument::Result(result) => Argument::Result(offset_result(result)),
        Argument::NestedResult(result, index) => {
            Argument::NestedResult(offset_result(result), index)
        }
        Argument::Input(input) => {
            if let Some(coin_result) = rewritten_inputs.get(&input) {
                // replace inputs that refer to a coin reservation with the corresponding ephemeral coin
                *coin_result
            } else {
                // all other inputs are left as is
                Argument::Input(input)
            }
        }
        _ => arg,
    };

    // now take the commands from the original ProgrammableTransaction, fix up all the result
    // arguments by adding num_commands to them, replace any input arguments with the corresponding
    // ephemeral coin
    for command in pt.commands.into_iter() {
        match command {
            Command::MoveCall(mut programmable_move_call) => {
                programmable_move_call.arguments = programmable_move_call
                    .arguments
                    .into_iter()
                    .map(fixup_arg)
                    .collect();
                builder.command(Command::MoveCall(programmable_move_call));
            }
            Command::TransferObjects(arguments, argument) => {
                let arguments = arguments.into_iter().map(fixup_arg).collect();
                let argument = fixup_arg(argument);
                builder.command(Command::TransferObjects(arguments, argument));
            }
            Command::SplitCoins(argument, arguments) => {
                let argument = fixup_arg(argument);
                let arguments = arguments.into_iter().map(fixup_arg).collect();
                builder.command(Command::SplitCoins(argument, arguments));
            }
            Command::MergeCoins(argument, arguments) => {
                let argument = fixup_arg(argument);
                let arguments = arguments.into_iter().map(fixup_arg).collect();
                builder.command(Command::MergeCoins(argument, arguments));
            }
            Command::Publish(items, object_ids) => {
                builder.command(Command::Publish(items, object_ids));
            }
            Command::MakeMoveVec(type_input, arguments) => {
                let arguments = arguments.into_iter().map(fixup_arg).collect();
                builder.command(Command::MakeMoveVec(type_input, arguments));
            }
            Command::Upgrade(items, object_ids, object_id, argument) => {
                let argument = fixup_arg(argument);
                builder.command(Command::Upgrade(items, object_ids, object_id, argument));
            }
        }
    }

    let sender_arg = builder
        .pure(sender)
        .expect("SuiAddress cannot fail to serialize");

    // now add a command to send all ephemeral coins back to the sender
    for (balance_type_input, coin_result) in ephemeral_coins {
        builder.command(Command::MoveCall(Box::new(ProgrammableMoveCall {
            package: SUI_FRAMEWORK_PACKAGE_ID,
            module: COIN_MODULE_NAME.to_string(),
            function: COIN_SEND_FUNDS_FUNCTION_NAME.to_string(),
            type_arguments: vec![balance_type_input],
            arguments: vec![coin_result, sender_arg],
        })));
    }

    builder.finish()
}
