// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use once_cell::sync::Lazy;
use proptest::collection::vec;
use proptest::prelude::*;
use sui_protocol_config::ProtocolConfig;
use sui_types::messages::{Argument, Command, ProgrammableTransaction};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;

static PROTOCOL_CONFIG: Lazy<ProtocolConfig> = Lazy::new(ProtocolConfig::get_for_max_version);

prop_compose! {
    pub fn gen_transfer()
        (x in arg_len_strategy())
        (args in vec(gen_argument(), x..=x), arg_to in gen_argument()) -> Command {
                Command::TransferObjects(args, arg_to)
    }
}

prop_compose! {
    pub fn gen_split_coins()
        (x in arg_len_strategy())
        (args in vec(gen_argument(), x..=x), arg_to in gen_argument()) -> Command {
                Command::SplitCoins(arg_to, args)
    }
}

prop_compose! {
    pub fn gen_merge_coins()
        (x in arg_len_strategy())
        (args in vec(gen_argument(), x..=x), arg_from in gen_argument()) -> Command {
                Command::MergeCoins(arg_from, args)
    }
}

prop_compose! {
    pub fn gen_move_vec()
        (x in arg_len_strategy())
        (args in vec(gen_argument(), x..=x)) -> Command {
                Command::MakeMoveVec(None, args)
    }
}

prop_compose! {
    pub fn gen_programmable_transaction()
        (len in command_len_strategy())
        (commands in vec(gen_command(), len..=len)) -> ProgrammableTransaction {
            let mut builder = ProgrammableTransactionBuilder::new();
            for command in commands {
                builder.command(command);
            }
            builder.finish()
    }
}

pub fn gen_command() -> impl Strategy<Value = Command> {
    prop_oneof![
        gen_transfer(),
        gen_split_coins(),
        gen_merge_coins(),
        gen_move_vec(),
    ]
}

pub fn gen_argument() -> impl Strategy<Value = Argument> {
    prop_oneof![
        Just(Argument::GasCoin),
        u16_with_boundaries_strategy().prop_map(Argument::Input),
        u16_with_boundaries_strategy().prop_map(Argument::Result),
        (
            u16_with_boundaries_strategy(),
            u16_with_boundaries_strategy()
        )
            .prop_map(|(a, b)| Argument::NestedResult(a, b))
    ]
}

pub fn u16_with_boundaries_strategy() -> impl Strategy<Value = u16> {
    prop_oneof![
        5 => 0u16..u16::MAX - 1,
        1 => Just(u16::MAX - 1),
        1 => Just(u16::MAX),
    ]
}

pub fn arg_len_strategy() -> impl Strategy<Value = usize> {
    let max_args = PROTOCOL_CONFIG.max_arguments() as usize;
    1usize..max_args
}

pub fn command_len_strategy() -> impl Strategy<Value = usize> {
    let max_commands = PROTOCOL_CONFIG.max_programmable_tx_commands() as usize;
    // Favor smaller transactions to make things faster. But generate a big one every once in a while
    prop_oneof![
        10 => 1usize..10,
        1 => 10..=max_commands,
    ]
}
