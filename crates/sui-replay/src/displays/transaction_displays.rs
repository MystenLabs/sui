// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::displays::Pretty;
use crate::replay::LocalExec;
use move_core_types::annotated_value::{MoveTypeLayout, MoveValue};
use move_core_types::language_storage::TypeTag;
use std::fmt::{Display, Formatter};
use std::sync::Arc;
use sui_execution::Executor;
use sui_types::execution::ExecutionResult;
use sui_types::object::bounded_visitor::BoundedVisitor;
use sui_types::transaction::CallArg::Pure;
use sui_types::transaction::{
    write_sep, Argument, CallArg, Command, ObjectArg, ProgrammableMoveCall, ProgrammableTransaction,
};
use tabled::{
    builder::Builder as TableBuilder,
    settings::{style::HorizontalLine, Panel as TablePanel, Style as TableStyle},
};

pub struct FullPTB {
    pub ptb: ProgrammableTransaction,
    pub results: Vec<ResolvedResults>,
}

pub struct ResolvedResults {
    pub mutable_reference_outputs: Vec<(Argument, MoveValue)>,
    pub return_values: Vec<MoveValue>,
}

/// These Display implementations provide alternate displays that are used to format info contained
/// in these Structs when calling the CLI replay command with an additional provided flag.
impl<'a> Display for Pretty<'a, FullPTB> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Pretty(full_ptb) = self;
        let FullPTB { ptb, results } = full_ptb;

        let ProgrammableTransaction { inputs, commands } = ptb;

        // write input objects section
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

        // write command results section
        if !results.is_empty() {
            write!(f, "\n\n")?;
        }
        for (i, result) in results.iter().enumerate() {
            if i == results.len() - 1 {
                write!(
                    f,
                    "╭───────────────────╮\n│ Command {i:<2} Output │\n╰───────────────────╯{}\n\n\n",
                    Pretty(result)
                )?
            } else {
                write!(
                    f,
                    "╭───────────────────╮\n│ Command {i:<2} Output │\n╰───────────────────╯{}\n",
                    Pretty(result)
                )?
            }
        }

        // write ptb functions section
        let mut builder = TableBuilder::default();
        if !commands.is_empty() {
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
            write!(f, "\n{}\n", table)?;
        } else {
            write!(f, "\n  No commands for this transaction")?;
        }

        Ok(())
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
impl<'a> Display for Pretty<'a, ResolvedResults> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Pretty(ResolvedResults {
            mutable_reference_outputs,
            return_values,
        }) = self;

        let len_m_ref = mutable_reference_outputs.len();
        let len_ret_vals = return_values.len();

        if len_ret_vals > 0 {
            write!(f, "\n Return Values:\n ──────────────")?;
        }

        for (i, value) in return_values.iter().enumerate() {
            write!(f, "\n • Result {i:<2} ")?;
            write!(f, "\n{:#}\n", value)?;
        }

        if len_m_ref > 0 {
            write!(
                f,
                "\n Mutable Reference Outputs:\n ──────────────────────────"
            )?;
        }

        for (arg, value) in mutable_reference_outputs {
            write!(f, "\n • {} ", arg)?;
            write!(f, "\n{:#}\n", value)?;
        }

        if len_ret_vals == 0 && len_m_ref == 0 {
            write!(f, "\n No return values")?;
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

fn resolve_to_layout(
    type_tag: &TypeTag,
    executor: &Arc<dyn Executor + Send + Sync>,
    store_factory: &LocalExec,
) -> MoveTypeLayout {
    match type_tag {
        TypeTag::Vector(inner) => {
            MoveTypeLayout::Vector(Box::from(resolve_to_layout(inner, executor, store_factory)))
        }
        TypeTag::Struct(inner) => {
            let mut layout_resolver = executor.type_layout_resolver(Box::new(store_factory));
            layout_resolver
                .get_annotated_layout(inner)
                .unwrap()
                .into_layout()
        }
        TypeTag::Bool => MoveTypeLayout::Bool,
        TypeTag::U8 => MoveTypeLayout::U8,
        TypeTag::U64 => MoveTypeLayout::U64,
        TypeTag::U128 => MoveTypeLayout::U128,
        TypeTag::Address => MoveTypeLayout::Address,
        TypeTag::Signer => MoveTypeLayout::Signer,
        TypeTag::U16 => MoveTypeLayout::U16,
        TypeTag::U32 => MoveTypeLayout::U32,
        TypeTag::U256 => MoveTypeLayout::U256,
    }
}

fn resolve_value(
    bytes: &[u8],
    type_tag: &TypeTag,
    executor: &Arc<dyn Executor + Send + Sync>,
    store_factory: &LocalExec,
) -> anyhow::Result<MoveValue> {
    let layout = resolve_to_layout(type_tag, executor, store_factory);
    BoundedVisitor::deserialize_value(bytes, &layout)
}

pub fn transform_command_results_to_annotated(
    executor: &Arc<dyn Executor + Send + Sync>,
    store_factory: &LocalExec,
    results: Vec<ExecutionResult>,
) -> anyhow::Result<Vec<ResolvedResults>> {
    let mut output = Vec::new();
    for (m_refs, return_vals) in results.iter() {
        let mut m_refs_out = Vec::new();
        let mut return_vals_out = Vec::new();
        for (arg, bytes, tag) in m_refs {
            m_refs_out.push((*arg, resolve_value(bytes, tag, executor, store_factory)?));
        }
        for (bytes, tag) in return_vals {
            return_vals_out.push(resolve_value(bytes, tag, executor, store_factory)?);
        }
        output.push(ResolvedResults {
            mutable_reference_outputs: m_refs_out,
            return_values: return_vals_out,
        });
    }
    Ok(output)
}
