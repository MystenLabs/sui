// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::displays::Pretty;
use move_core_types::language_storage::TypeTag;
use serde_json::Value;
use std::fmt::{Display, Formatter};
use sui_sdk::json::SuiJsonValue;
use sui_types::execution_mode::ExecutionResult;
use sui_types::transaction::CallArg::Pure;
use sui_types::transaction::{
    write_sep, Argument, CallArg, Command, ObjectArg, ProgrammableMoveCall, ProgrammableTransaction,
};
use tabled::{
    builder::Builder as TableBuilder,
    settings::{style::HorizontalLine, Panel as TablePanel, Style as TableStyle},
};

pub type FullPTB = (ProgrammableTransaction, Vec<ExecutionResult>);

/// These Display implementations provide alternate displays that are used to format info contained
/// in these Structs when calling the CLI replay command with an additional provided flag.
impl<'a> Display for Pretty<'a, FullPTB> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Pretty(full_ptb) = self;
        let (ptb, results) = full_ptb;

        let mut builder = TableBuilder::default();

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

        let mut table = builder.build();
        table.with(TablePanel::header("Input Objects"));
        table.with(TableStyle::rounded().horizontals([HorizontalLine::new(
            1,
            TableStyle::modern().get_horizontal(),
        )]));
        write!(f, "\n{}\n", table)?;

        if !commands.is_empty() {
            let mut builder = TableBuilder::default();
            for (i, c) in commands.iter().enumerate() {
                if i == commands.len() - 1 {
                    builder.push_record(vec![format!("{i:<2} {} {}", Pretty(c), Pretty(result))]);
                } else {
                    builder.push_record(vec![format!("{i:<2} {} {}\n", Pretty(c), Pretty(result))]);
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
            Argument::NestedResult(j, k) => format!("Result {}: {}", j, k),
        };
        write!(f, "{}", output)
    }
}

impl<'a> Display for Pretty<'a, ExecutionResult> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Pretty((mutable_ref_outputs, return_values)) = self;
        let len_m_ref = mutable_ref_outputs.len();
        let len_ret_vals = return_values.len();
        if len_ret_vals > 0 || len_m_ref > 0 {
            write!(f, "\n ┌ ")?;
        }
        if len_m_ref > 0 {
            write!(f, "\n │ Mutable Reference Outputs:")?;
        }
        for (arg, bytes, type_tag) in mutable_ref_outputs {
            write!(f, "\n │   {} ", arg)?;
            resolve_value(f, bytes, type_tag)?;
        }
        if len_ret_vals > 0 {
            write!(f, "\n │ Return Values:")?;
        }

        for (i, (bytes, type_tag)) in return_values.iter().enumerate() {
            write!(f, "\n │   {i:<2} ")?;
            resolve_value(f, bytes, type_tag)?;
        }
        if len_ret_vals > 0 || len_m_ref > 0 {
            write!(f, "\n └")?;
        }
        Ok(())
    }
}

impl<'a> Display for Pretty<'a, TypeTag> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Pretty(type_tag) = self;
        match type_tag {
            TypeTag::Vector(v) => {
                write!(f, "Vector of {}", Pretty(&**v))
            }
            TypeTag::Struct(s) => {
                write!(f, "{}::{}", s.module, s.name)
            }
            _ => {
                write!(f, "{}", type_tag)
            }
        }
    }
}

fn resolve_value(f: &mut Formatter<'_>, bytes: &[u8], type_tag: &TypeTag) -> std::fmt::Result {
    let value = SuiJsonValue::from_bcs_bytes(None, bytes)
        .unwrap_or_else(|e| panic!("Could not resolve value from bcs bytes: {e}"))
        .to_json_value();
    write!(f, "{}", Pretty(type_tag))?;
    match value {
        Value::Null => {
            write!(f, "   Null")
        }
        Value::Bool(b) => {
            write!(f, "   {b}")
        }
        Value::Number(n) => {
            write!(f, "   {n}")
        }
        Value::String(s) => {
            write!(f, "   {s}")
        }
        Value::Array(_a) => {
            Ok(()) // We haven't printed this value by default for readability
                   // but if you need it, add `write!(f, "{:?}", _a)`
        }
        Value::Object(_o) => {
            Ok(()) // We haven't printed this value by default for readability
                   // but if you need it, add `write!(f, "{:?}", _o)`
        }
    }
}
