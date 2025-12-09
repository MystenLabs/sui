// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Utility for printing a `ProgrammableTransaction` in the format for test cases.

use core::panic;
use std::{collections::BTreeMap, fmt};

use move_core_types::account_address::AccountAddress;
use sui_types::{
    BRIDGE_ADDRESS, DEEPBOOK_ADDRESS, MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS,
    SUI_SYSTEM_ADDRESS,
    transaction::{Argument, Command, ProgrammableMoveCall},
    type_input::{StructInput, TypeInput},
};

use crate::programmable_transaction_test_parser::token::{
    GAS_COIN, INPUT, MAKE_MOVE_VEC, MERGE_COINS, NESTED_RESULT, RESULT, SPLIT_COINS,
    TRANSFER_OBJECTS,
};

pub fn commands<W: fmt::Write>(
    named_addresses: &BTreeMap<AccountAddress, String>,
    buf: &mut W,
    cs: &[Command],
) -> fmt::Result {
    for (i, c) in cs.iter().enumerate() {
        write!(buf, "//> {i}: ")?;
        command(named_addresses, buf, c)?;
        writeln!(buf, ";")?;
    }
    Ok(())
}

pub fn command<W: fmt::Write>(
    named_addresses: &BTreeMap<AccountAddress, String>,
    buf: &mut W,
    command: &Command,
) -> fmt::Result {
    match command {
        Command::MoveCall(m) => {
            let ProgrammableMoveCall {
                package,
                module,
                function,
                type_arguments: tys,
                arguments: args,
            } = &**m;
            write!(
                buf,
                "{}::{}::{}",
                address_str(named_addresses, **package),
                module,
                function
            )?;
            type_arguments(named_addresses, buf, tys)?;
            write!(buf, "(")?;
            arguments(buf, args)?;
            write!(buf, ")")
        }
        Command::TransferObjects(objs, r) => {
            write!(buf, "{}([", TRANSFER_OBJECTS)?;
            arguments(buf, objs)?;
            write!(buf, "], ")?;
            argument(buf, r)?;
            write!(buf, ")")
        }
        Command::SplitCoins(c, amts) => {
            write!(buf, "{}(", SPLIT_COINS)?;
            argument(buf, c)?;
            write!(buf, ", [")?;
            arguments(buf, amts)?;
            write!(buf, "])")
        }
        Command::MergeCoins(c, coins) => {
            write!(buf, "{}(", MERGE_COINS)?;
            argument(buf, c)?;
            write!(buf, ", [")?;
            arguments(buf, coins)?;
            write!(buf, "])")
        }
        Command::MakeMoveVec(t_opt, args) => {
            write!(buf, "{}", MAKE_MOVE_VEC)?;
            type_arguments(named_addresses, buf, t_opt.as_slice())?;
            write!(buf, "([")?;
            arguments(buf, args)?;
            write!(buf, "])")
        }
        Command::Publish(_, _) | Command::Upgrade(_, _, _, _) => panic!(
            "Publish and Upgrade are not supported. In transactional adapter tests, you cannot \
            publish or upgrade via bytes"
        ),
    }
}

pub fn arguments<W: fmt::Write>(buf: &mut W, args: &[Argument]) -> fmt::Result {
    for (i, a) in args.iter().enumerate() {
        if i > 0 {
            write!(buf, ", ")?;
        }
        argument(buf, a)?;
    }
    Ok(())
}

pub fn argument<W: fmt::Write>(buf: &mut W, arg: &Argument) -> fmt::Result {
    match arg {
        Argument::GasCoin => write!(buf, "{}", GAS_COIN),
        Argument::Input(i) => write!(buf, "{}({})", INPUT, i),
        Argument::Result(i) => write!(buf, "{}({})", RESULT, i),
        Argument::NestedResult(i, j) => write!(buf, "{}({}, {})", NESTED_RESULT, i, j),
    }
}

pub fn type_<W: fmt::Write>(
    named_addresses: &BTreeMap<AccountAddress, String>,
    buf: &mut W,
    ty: &TypeInput,
) -> fmt::Result {
    match ty {
        TypeInput::U8 => write!(buf, "u8"),
        TypeInput::U16 => write!(buf, "u16"),
        TypeInput::U32 => write!(buf, "u32"),
        TypeInput::U64 => write!(buf, "u64"),
        TypeInput::U128 => write!(buf, "u128"),
        TypeInput::U256 => write!(buf, "u256"),
        TypeInput::Address => write!(buf, "address"),
        TypeInput::Signer => write!(buf, "signer"),
        TypeInput::Bool => write!(buf, "bool"),
        TypeInput::Struct(s) => struct_type_(named_addresses, buf, s),
        TypeInput::Vector(ty) => {
            write!(buf, "vector<")?;
            type_(named_addresses, buf, ty)?;
            write!(buf, ">")
        }
    }
}

pub fn struct_type_<W: fmt::Write>(
    named_addresses: &BTreeMap<AccountAddress, String>,
    buf: &mut W,
    s: &StructInput,
) -> fmt::Result {
    let StructInput {
        address,
        module,
        name,
        type_params,
    } = s;
    write!(
        buf,
        "{}::{}::{}",
        address_str(named_addresses, *address),
        module,
        name
    )?;
    type_arguments(named_addresses, buf, type_params)?;
    Ok(())
}

pub fn type_arguments<W: fmt::Write>(
    named_addresses: &BTreeMap<AccountAddress, String>,
    buf: &mut W,
    tys: &[TypeInput],
) -> fmt::Result {
    if !tys.is_empty() {
        write!(buf, "<")?;
        for (i, ty) in tys.iter().enumerate() {
            if i > 0 {
                write!(buf, ", ")?;
            }
            type_(named_addresses, buf, ty)?;
        }
        write!(buf, ">")?;
    }
    Ok(())
}

pub fn address_str(
    named_addresses: &BTreeMap<AccountAddress, String>,
    address: AccountAddress,
) -> String {
    match address {
        SUI_FRAMEWORK_ADDRESS => "sui".to_owned(),
        MOVE_STDLIB_ADDRESS => "std".to_owned(),
        SUI_SYSTEM_ADDRESS => "sui_system".to_owned(),
        BRIDGE_ADDRESS => "bridge".to_owned(),
        DEEPBOOK_ADDRESS => "deepbook".to_owned(),
        _ => match named_addresses.get(&address) {
            Some(name) => name.clone(),
            _ => panic!("Unknown address: {}", address),
        },
    }
}
