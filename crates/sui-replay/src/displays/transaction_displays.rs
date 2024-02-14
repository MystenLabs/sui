// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::displays::Pretty;
use std::fmt::{Display, Formatter};
use sui_types::transaction::CallArg::Pure;
use sui_types::transaction::{
    write_sep, Argument, CallArg, Command, ObjectArg, ProgrammableMoveCall, ProgrammableTransaction,
};
use tabled::{
    builder::Builder as TableBuilder,
    settings::{style::HorizontalLine, Panel as TablePanel, Style as TableStyle},
};

/// These Display implementations provide alternate displays that are used to format info contained
/// in these Structs when calling the CLI replay command with an additional provided flag.
impl<'a> Display for Pretty<'a, ProgrammableTransaction> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Pretty(ptb) = self;
        let ProgrammableTransaction { inputs, commands } = ptb;
        if !inputs.is_empty() {
            let mut builder = TableBuilder::default();
            for (i, input) in inputs.iter().enumerate() {
                match input {
                    Pure(v) => {
                        if v.len() <= 16 {
                            builder.push_record(vec![format!("{i:<3} Pure Arg          {:?}", v)]);
                        } else {
                            builder.push_record(vec![format!(
                            "{i:<3} Pure Arg          [{}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, ...]",
                            v[0],
                            v[1],
                            v[2],
                            v[3],
                            v[4],
                            v[5],
                            v[6],
                            v[7],
                            v[8],
                            v[9],
                            v[10],
                            v[11],
                            v[12],
                            v[13],
                            v[14],
                        )]);
                        }
                    }
                    CallArg::Object(ObjectArg::ImmOrOwnedObject(o)) => {
                        builder.push_record(vec![format!("{i:<3} Imm/Owned Object  ID: {}", o.0)]);
                    }
                    CallArg::Object(ObjectArg::SharedObject { id, .. }) => {
                        builder.push_record(vec![format!("{i:<3} Shared Object     ID: {}", id)]);
                    }
                    CallArg::Object(ObjectArg::Receiving(o)) => {
                        builder.push_record(vec![format!("{i:<3} Receiving Object  ID: {}", o.0)]);
                    }
                };
            }

            let mut table = builder.build();
            table.with(TablePanel::header("Input Objects"));
            table.with(TableStyle::rounded().horizontals([HorizontalLine::new(
                1,
                TableStyle::modern().get_horizontal(),
            )]));
            write!(f, "\n{}\n", table)?;
        } else {
            write!(f, "\n  No input objects for this transaction")?;
        }

        if !commands.is_empty() {
            let mut builder = TableBuilder::default();
            for (i, c) in commands.iter().enumerate() {
                if i == commands.len() - 1 {
                    builder.push_record(vec![format!("{i:<2} {}", Pretty(c))]);
                } else {
                    builder.push_record(vec![format!("{i:<2} {}\n", Pretty(c))]);
                }
            }
            let mut table = builder.build();
            table.with(TablePanel::header("Commands"));
            table.with(TableStyle::rounded().horizontals([HorizontalLine::new(
                1,
                TableStyle::modern().get_horizontal(),
            )]));
            write!(f, "\n{}\n", table)
        } else {
            write!(f, "\n  No commands for this transaction")
        }
    }
}

impl<'a> Display for Pretty<'a, Command> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Pretty(command) = self;
        match command {
            Command::MoveCall(p) => {
                write!(f, "{}", Pretty(&**p))
            }
            Command::MakeMoveVec(ty_opt, elems) => {
                write!(f, "MakeMoveVec:\n ┌")?;
                if let Some(ty) = ty_opt {
                    write!(f, "\n │ Type Tag: {ty}")?;
                }
                write!(f, "\n │ Arguments:\n │   ")?;
                write_sep(f, elems.iter().map(Pretty), "\n │   ")?;
                write!(f, "\n └")
            }
            Command::TransferObjects(objs, addr) => {
                write!(f, "TransferObjects:\n ┌\n │ Arguments: \n │   ")?;
                write_sep(f, objs.iter().map(Pretty), "\n │   ")?;
                write!(f, "\n │ Address: {}\n └", Pretty(addr))
            }
            Command::SplitCoins(coin, amounts) => {
                write!(
                    f,
                    "SplitCoins:\n ┌\n │ Coin: {}\n │ Amounts: \n │   ",
                    Pretty(coin)
                )?;
                write_sep(f, amounts.iter().map(Pretty), "\n │   ")?;
                write!(f, "\n └")
            }
            Command::MergeCoins(target, coins) => {
                write!(
                    f,
                    "MergeCoins:\n ┌\n │ Target: {}\n │ Coins: \n │   ",
                    Pretty(target)
                )?;
                write_sep(f, coins.iter().map(Pretty), "\n │   ")?;
                write!(f, "\n └")
            }
            Command::Publish(_bytes, deps) => {
                write!(f, "Publish:\n ┌\n │ Dependencies: \n │   ")?;
                write_sep(f, deps, "\n │   ")?;
                write!(f, "\n └")
            }
            Command::Upgrade(_bytes, deps, current_package_id, ticket) => {
                write!(f, "Upgrade:\n ┌\n │ Dependencies: \n │   ")?;
                write_sep(f, deps, "\n │   ")?;
                write!(f, "\n │ Current Package ID: {current_package_id}")?;
                write!(f, "\n │ Ticket: {}", Pretty(ticket))?;
                write!(f, "\n └")
            }
        }
    }
}

impl<'a> Display for Pretty<'a, ProgrammableMoveCall> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Pretty(move_call) = self;
        let ProgrammableMoveCall {
            package,
            module,
            function,
            type_arguments,
            arguments,
        } = move_call;

        write!(
            f,
            "MoveCall:\n ┌\n │ Function:  {} \n │ Module:    {}\n │ Package:   {}",
            function, module, package
        )?;

        if !type_arguments.is_empty() {
            write!(f, "\n │ Type Arguments: \n │   ")?;
            write_sep(f, type_arguments, "\n │   ")?;
        }
        if !arguments.is_empty() {
            write!(f, "\n │ Arguments: \n │   ")?;
            write_sep(f, arguments.iter().map(Pretty), "\n │   ")?;
        }

        write!(f, "\n └")
    }
}

impl<'a> Display for Pretty<'a, Argument> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Pretty(argument) = self;

        let output = match argument {
            Argument::GasCoin => "GasCoin".to_string(),
            Argument::Input(i) => format!("Input  {}", i),
            Argument::Result(i) => format!("Result {}", i),
            Argument::NestedResult(j, k) => format!("Nested Result {}: {}", j, k),
        };
        write!(f, "{}", output)
    }
}
