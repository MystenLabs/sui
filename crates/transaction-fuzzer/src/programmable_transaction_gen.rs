// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{cmp, str::FromStr};

use move_core_types::identifier::Identifier;
use once_cell::sync::Lazy;
use proptest::collection::vec;
use proptest::prelude::*;
use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{Argument, CallArg, Command, ProgrammableTransaction};

static PROTOCOL_CONFIG: Lazy<ProtocolConfig> =
    Lazy::new(ProtocolConfig::get_for_max_version_UNSAFE);

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

// these constants have been chosen to deliver a reasonable runtime overhead and can be played with

/// this also reflects the fact that we have coin-generating functions that can generate between 1
/// and MAX_ARG_LEN_INPUT_MATCH coins
pub const MAX_ARG_LEN_INPUT_MATCH: usize = 64;
pub const MAX_COMMANDS_INPUT_MATCH: usize = 24;
pub const MAX_ITERATIONS_INPUT_MATCH: u32 = 10;
pub const MAX_SPLIT_AMOUNT: u64 = 1000;
/// the merge command takes must take no more than MAX_ARG_LEN_INPUT_MATCH total to make sure that
/// we have enough coins to pass as input
pub const MAX_COINS_TO_MERGE: u64 = (MAX_ARG_LEN_INPUT_MATCH - 1) as u64;
/// the max number of coins that the vector can be made out of cannot exceed the number of coins we
/// can generate as input
pub const MAX_VECTOR_COINS: usize = MAX_ARG_LEN_INPUT_MATCH;

/// Stand-ins for programmable transaction Commands used to randomly generate values used when
/// creating the actual command instances
#[derive(Debug)]
pub enum CommandSketch {
    // Command::TransferObjects sketch - argument describes number of objects to transfer
    TransferObjects(u64),
    // Command::SplitCoins sketch - argument describes coin values to split
    SplitCoins(Vec<u64>),
    // Command::MergeCoins sketch - argument describes number of coins to merge
    MergeCoins(u64),
    // Command::MakeMoveVec sketch - argument describes coins to be put into a vector
    MakeMoveVec(Vec<u64>),
}

prop_compose! {
    pub fn gen_transfer_input_match()
        (x in arg_len_strategy_input_match()) -> CommandSketch {
            CommandSketch::TransferObjects(x as u64)
    }
}

prop_compose! {
    pub fn gen_split_coins_input_match()
        (x in arg_len_strategy_input_match())
        (args in vec(1..MAX_SPLIT_AMOUNT, x..=x)) -> CommandSketch {
            CommandSketch::SplitCoins(args)
    }
}

prop_compose! {
    pub fn gen_merge_coins_input_match()
        (coins_to_merge in 1..MAX_COINS_TO_MERGE) -> CommandSketch {
            CommandSketch::MergeCoins(coins_to_merge)
    }
}

prop_compose! {
    pub fn gen_move_vec_input_match()
        (vec_size in 1..MAX_VECTOR_COINS)
        (args in vec(1u64..7u64, vec_size..=vec_size)) -> CommandSketch {
            // at this point we don't care about coin values to be put into the vector but we keep
            // the vector itself to be able to match on a union of MakeMoveVec and SplitCoins when
            // generating the actual commands
            CommandSketch::MakeMoveVec(args)
    }
}

pub fn gen_command_input_match() -> impl Strategy<Value = CommandSketch> {
    prop_oneof![
        gen_transfer_input_match(),
        gen_split_coins_input_match(),
        gen_merge_coins_input_match(),
        gen_move_vec_input_match(),
    ]
}

pub fn arg_len_strategy_input_match() -> impl Strategy<Value = usize> {
    prop_oneof![
        20 => 1usize..10,
        10 => 10usize..MAX_ARG_LEN_INPUT_MATCH
    ]
}

prop_compose! {
    pub fn gen_many_input_match(recipient: SuiAddress, package: ObjectID, cap: ObjectRef)
        (mut command_sketches in vec(gen_command_input_match(), 1..=MAX_COMMANDS_INPUT_MATCH)) -> ProgrammableTransaction {
            let mut builder = ProgrammableTransactionBuilder::new();
            let mut prev_cmd_num = -1;
            // does not matter which is picked as first as they are generated randomly anyway
            let first_cmd_sketch = command_sketches.pop().unwrap();
            let (first_cmd, cmd_inc) = gen_input(&mut builder, None, &first_cmd_sketch, prev_cmd_num, recipient, package, cap);
            builder.command(first_cmd);
            prev_cmd_num += cmd_inc + 1;
            let mut prev_cmd = first_cmd_sketch;
            for cmd_sketch in command_sketches {
                let (cmd, cmd_inc) = gen_input(&mut builder, Some(&prev_cmd), &cmd_sketch, prev_cmd_num, recipient, package, cap);
                builder.command(cmd);
                prev_cmd_num += cmd_inc + 1;
                prev_cmd = cmd_sketch;
            }
            builder.finish()
    }
}

fn gen_input(
    builder: &mut ProgrammableTransactionBuilder,
    prev_command: Option<&CommandSketch>,
    cmd: &CommandSketch,
    prev_cmd_num: i64,
    recipient: SuiAddress,
    package: ObjectID,
    cap: ObjectRef,
) -> (Command, i64) {
    match cmd {
        CommandSketch::TransferObjects(_) => gen_transfer_input(
            builder,
            prev_command,
            cmd,
            prev_cmd_num,
            recipient,
            package,
            cap,
        ),
        CommandSketch::SplitCoins(_) => {
            gen_split_coins_input(builder, cmd, prev_cmd_num, package, cap)
        }
        CommandSketch::MergeCoins(_) => {
            gen_merge_coins_input(builder, prev_command, cmd, prev_cmd_num, package, cap)
        }
        CommandSketch::MakeMoveVec(_) => {
            gen_move_vec_input(builder, prev_command, cmd, prev_cmd_num, package, cap)
        }
    }
}

pub fn gen_transfer_input(
    builder: &mut ProgrammableTransactionBuilder,
    prev_command: Option<&CommandSketch>,
    cmd: &CommandSketch,
    prev_cmd_num: i64,
    recipient: SuiAddress,
    package: ObjectID,
    cap: ObjectRef,
) -> (Command, i64) {
    let CommandSketch::TransferObjects(args_len) = cmd else {
        panic!("Should be TransferObjects command");
    };
    let mut coins = vec![];
    // we need that many coins as input to transfer
    let coins_needed = *args_len as usize;

    let cmd_inc = gen_transfer_or_move_vec_input_internal(
        builder,
        prev_cmd_num,
        package,
        cap,
        prev_command,
        coins_needed,
        &mut coins,
    );
    assert!(coins.len() == *args_len as usize);

    let next_cmd = Command::TransferObjects(coins, builder.pure(recipient).unwrap());
    (next_cmd, cmd_inc)
}

pub fn gen_split_coins_input(
    builder: &mut ProgrammableTransactionBuilder,
    cmd: &CommandSketch,
    prev_cmd_num: i64,
    package: ObjectID,
    cap: ObjectRef,
) -> (Command, i64) {
    let CommandSketch::SplitCoins(split_amounts) = cmd else {
        panic!("Should be SplitCoins command");
    };
    let mut cmd_inc = 0;
    let mut split_args = vec![];

    // the tradeoff here is that we either generate output for each split command that will make it
    // succeed or we will very quickly hit the insufficient coin error only after a few (often just
    // 2) split coin transactions are executed making the whole batch testing into a rather narrow
    // error case
    create_input_calls(
        builder,
        package,
        cap,
        prev_cmd_num,
        MAX_SPLIT_AMOUNT * split_amounts.len() as u64,
        1,
    );
    cmd_inc += 2; // two input calls

    for s in split_amounts {
        split_args.push(builder.pure(*s).unwrap());
    }

    let coin_arg = Argument::Result((prev_cmd_num + cmd_inc) as u16);
    let next_cmd = Command::SplitCoins(coin_arg, split_args);
    (next_cmd, cmd_inc)
}

pub fn gen_merge_coins_input(
    builder: &mut ProgrammableTransactionBuilder,
    prev_command: Option<&CommandSketch>,
    cmd: &CommandSketch,
    prev_cmd_num: i64,
    package: ObjectID,
    cap: ObjectRef,
) -> (Command, i64) {
    let CommandSketch::MergeCoins(coins_to_merge) = cmd else {
        panic!("Should be MergeCoins command");
    };
    let mut cmd_inc = 0;
    let mut coins = vec![];
    // we need all coins that are going to be merged plus on that they are going to be merged into
    let coins_needed = *coins_to_merge as usize + 1;

    let output_coin = if let Some(prev_cmd) = prev_command {
        match prev_cmd {
            CommandSketch::TransferObjects(_) | CommandSketch::MergeCoins(_) => {
                // no useful input
                create_input_calls(builder, package, cap, prev_cmd_num, 7, coins_needed as u64);
                cmd_inc += 2; // two input calls
                for i in 0..coins_needed - 1 {
                    coins.push(Argument::NestedResult(
                        (prev_cmd_num + cmd_inc) as u16,
                        i as u16,
                    ));
                }
                Argument::NestedResult((prev_cmd_num + cmd_inc) as u16, *coins_to_merge as u16)
            }
            CommandSketch::SplitCoins(output) | CommandSketch::MakeMoveVec(output) => {
                // how many coins we have a available as output from previous command that we can
                // immediately use as input to the next command
                let usable_coins = cmp::min(output.len(), coins_needed);
                if let CommandSketch::MakeMoveVec(_) = prev_cmd {
                    create_unpack_call(builder, package, prev_cmd_num, output.len() as u64);
                    cmd_inc += 1; // unpack call
                };
                // there is at least one coin in the output - use it as the coin others are merged into
                let res_coin = Argument::NestedResult((prev_cmd_num + cmd_inc) as u16, 0);

                cmd_inc = gen_enough_arguments(
                    builder,
                    prev_cmd_num,
                    package,
                    cap,
                    coins_needed,
                    usable_coins,
                    1, /* one available coin already used */
                    output.len(),
                    &mut coins,
                    cmd_inc,
                );
                res_coin
            }
        }
    } else {
        // first command - no input
        create_input_calls(builder, package, cap, prev_cmd_num, 7, coins_needed as u64);
        cmd_inc += 2; // two input calls
        for i in 0..coins_needed - 1 {
            coins.push(Argument::NestedResult(
                (prev_cmd_num + cmd_inc) as u16,
                i as u16,
            ));
        }
        Argument::NestedResult((prev_cmd_num + cmd_inc) as u16, *coins_to_merge as u16)
    };

    let next_cmd = Command::MergeCoins(output_coin, coins);
    (next_cmd, cmd_inc)
}

pub fn gen_move_vec_input(
    builder: &mut ProgrammableTransactionBuilder,
    prev_command: Option<&CommandSketch>,
    cmd: &CommandSketch,
    prev_cmd_num: i64,
    package: ObjectID,
    cap: ObjectRef,
) -> (Command, i64) {
    let CommandSketch::MakeMoveVec(vector_coins) = cmd else {
        panic!("Should be MakeMoveVec command");
    };
    let mut coins = vec![];
    // we need that many coins as input to transfer
    let coins_needed = vector_coins.len();

    let cmd_inc = gen_transfer_or_move_vec_input_internal(
        builder,
        prev_cmd_num,
        package,
        cap,
        prev_command,
        coins_needed,
        &mut coins,
    );

    let next_cmd = Command::MakeMoveVec(None, coins);
    (next_cmd, cmd_inc)
}

/// A helper function to generate enough input coins for a command (transfer, merge, or create vector)
/// - either collect them all from previous command or generate additional ones if the previous
///     command does not deliver enough.
fn gen_enough_arguments(
    builder: &mut ProgrammableTransactionBuilder,
    prev_cmd_num: i64,
    package: ObjectID,
    cap: ObjectRef,
    coins_needed: usize,
    coins_available: usize,
    available_coins_used: usize,
    prev_cmd_out_len: usize,
    coins: &mut Vec<Argument>,
    mut cmd_inc: i64,
) -> i64 {
    for i in available_coins_used..coins_available {
        coins.push(Argument::NestedResult(
            (prev_cmd_num + cmd_inc) as u16,
            i as u16,
        ));
    }
    if prev_cmd_out_len < coins_needed {
        // we have some arguments from previous command's output but not all
        let remaining_args_num = (coins_needed - prev_cmd_out_len) as u64;
        create_input_calls(
            builder,
            package,
            cap,
            prev_cmd_num + cmd_inc,
            7,
            remaining_args_num,
        );
        cmd_inc += 2; // two input calls
        for i in 0..remaining_args_num {
            coins.push(Argument::NestedResult(
                (prev_cmd_num + cmd_inc) as u16,
                i as u16,
            ));
        }
    }
    cmd_inc
}

/// A helper function to generate arguments fro transfer or create vector commands as they are
/// exactly the same.
fn gen_transfer_or_move_vec_input_internal(
    builder: &mut ProgrammableTransactionBuilder,
    prev_cmd_num: i64,
    package: ObjectID,
    cap: ObjectRef,
    prev_command: Option<&CommandSketch>,
    coins_needed: usize,
    coins: &mut Vec<Argument>,
) -> i64 {
    let mut cmd_inc = 0;
    if let Some(prev_cmd) = prev_command {
        match prev_cmd {
            CommandSketch::TransferObjects(_) | CommandSketch::MergeCoins(_) => {
                // no useful input
                create_input_calls(builder, package, cap, prev_cmd_num, 7, coins_needed as u64);
                cmd_inc += 2; // two input calls
                for i in 0..coins_needed {
                    coins.push(Argument::NestedResult(
                        (prev_cmd_num + cmd_inc) as u16,
                        i as u16,
                    ));
                }
            }
            CommandSketch::SplitCoins(output) | CommandSketch::MakeMoveVec(output) => {
                // how many coins we have a available as output from previous command that we can
                // immediately use as input to the next command
                let usable_coins = cmp::min(output.len(), coins_needed);
                if let CommandSketch::MakeMoveVec(_) = prev_cmd {
                    create_unpack_call(builder, package, prev_cmd_num, output.len() as u64);
                    cmd_inc += 1; // unpack call
                };

                cmd_inc = gen_enough_arguments(
                    builder,
                    prev_cmd_num,
                    package,
                    cap,
                    coins_needed,
                    usable_coins,
                    0, /* no available coins used */
                    output.len(),
                    coins,
                    cmd_inc,
                )
            }
        }
    } else {
        // first command - no input
        create_input_calls(builder, package, cap, prev_cmd_num, 7, coins_needed as u64);
        cmd_inc += 2; // two input calls
        for i in 0..coins_needed {
            coins.push(Argument::NestedResult(
                (prev_cmd_num + cmd_inc) as u16,
                i as u16,
            ));
        }
    }
    cmd_inc
}

fn create_input_calls(
    builder: &mut ProgrammableTransactionBuilder,
    package: ObjectID,
    cap: ObjectRef,
    prev_cmd_num: i64,
    coin_value: u64,
    input_size: u64,
) {
    builder
        .move_call(
            package,
            Identifier::from_str("coin_factory").unwrap(),
            Identifier::from_str("mint_vec").unwrap(),
            vec![],
            vec![
                CallArg::from(cap),
                CallArg::from(coin_value),
                CallArg::from(input_size),
            ],
        )
        .unwrap();
    create_unpack_call(builder, package, prev_cmd_num + 1, input_size);
}

fn create_unpack_call(
    builder: &mut ProgrammableTransactionBuilder,
    package: ObjectID,
    prev_cmd_num: i64,
    input_size: u64,
) {
    builder.programmable_move_call(
        package,
        Identifier::from_str("coin_factory").unwrap(),
        Identifier::from_str(format!("unpack_{input_size}").as_str()).unwrap(),
        vec![],
        vec![Argument::Result(prev_cmd_num as u16)],
    );
}
