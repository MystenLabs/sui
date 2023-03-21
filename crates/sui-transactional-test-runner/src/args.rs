// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{bail, ensure};
use clap;
use move_command_line_common::values::{ParsableValue, ParsedValue};
use move_command_line_common::{parser::Parser as MoveCLParser, values::ValueToken};
use move_compiler::shared::parse_u128;
use move_core_types::identifier::Identifier;
use move_core_types::value::{MoveStruct, MoveValue};
use sui_types::messages::{Argument, CallArg, ObjectArg};
use sui_types::object::Owner;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;

use crate::test_adapter::SuiTestAdapter;

pub const SUI_ARGS_LONG: &str = "sui-args";

#[derive(Debug, clap::Parser)]
pub struct SuiRunArgs {
    #[clap(long = "sender")]
    pub sender: Option<String>,
    #[clap(long = "view-events")]
    pub view_events: bool,
    #[clap(long = "view-gas-used")]
    pub view_gas_used: bool,
}

#[derive(Debug, clap::Parser)]
pub struct SuiPublishArgs {
    #[clap(long = "sender")]
    pub sender: Option<String>,
    #[clap(long = "upgradeable", action = clap::ArgAction::SetTrue)]
    pub upgradeable: bool,
    #[clap(long = "view-gas-used")]
    pub view_gas_used: bool,
    #[clap(
        long = "dependencies",
        multiple_values(true),
        multiple_occurrences(false)
    )]
    pub dependencies: Vec<String>,
}

#[derive(Debug, clap::Parser)]
pub struct SuiInitArgs {
    #[clap(long = "accounts", multiple_values(true), multiple_occurrences(false))]
    pub accounts: Option<Vec<String>>,
}

#[derive(Debug, clap::Parser)]
pub struct ViewObjectCommand {
    pub id: u64,
}

#[derive(Debug, clap::Parser)]
pub struct TransferObjectCommand {
    pub id: u64,
    #[clap(long = "recipient")]
    pub recipient: String,
    #[clap(long = "sender")]
    pub sender: Option<String>,
    #[clap(long = "gas-budget")]
    pub gas_budget: Option<u64>,
    #[clap(long = "view-gas-used")]
    pub view_gas_used: bool,
}

#[derive(Debug, clap::Parser)]
pub struct ConsensusCommitPrologueCommand {
    #[clap(long = "timestamp-ms")]
    pub timestamp_ms: u64,
}

#[derive(Debug, clap::Parser)]
pub struct ProgrammableTransactionCommand {
    #[clap(long = "sender")]
    pub sender: Option<String>,
    #[clap(long = "gas-budget")]
    pub gas_budget: Option<u64>,
    #[clap(long = "view-events")]
    pub view_events: bool,
    #[clap(long = "view-gas-used")]
    pub view_gas_used: bool,
    #[clap(
        long = "inputs",
        parse(try_from_str = ParsedValue::parse),
        takes_value(true),
        multiple_values(true),
        multiple_occurrences(true)
    )]
    pub inputs: Vec<ParsedValue<SuiExtraValueArgs>>,
}

#[derive(Debug, clap::Parser)]
pub enum SuiSubcommand {
    #[clap(name = "view-object")]
    ViewObject(ViewObjectCommand),
    #[clap(name = "transfer-object")]
    TransferObject(TransferObjectCommand),
    #[clap(name = "consensus-commit-prologue")]
    ConsensusCommitPrologue(ConsensusCommitPrologueCommand),
    #[clap(name = "programmable")]
    ProgrammableTransaction(ProgrammableTransactionCommand),
}

#[derive(Debug)]
pub enum SuiExtraValueArgs {
    Object(u64),
}

pub enum SuiValue {
    MoveValue(MoveValue),
    Object(u64),
    ObjVec(Vec<u64>),
}

impl SuiExtraValueArgs {
    fn parse_value_impl<'a, I: Iterator<Item = (ValueToken, &'a str)>>(
        parser: &mut MoveCLParser<'a, ValueToken, I>,
    ) -> anyhow::Result<Self> {
        let contents = parser.advance(ValueToken::Ident)?;
        ensure!(contents == "object");
        parser.advance(ValueToken::LParen)?;
        let u_str = parser.advance(ValueToken::Number)?;
        let (fake_id, _) = parse_u128(u_str)?;
        if fake_id > (u64::MAX as u128) {
            bail!("Object id too large")
        }
        parser.advance(ValueToken::RParen)?;
        Ok(SuiExtraValueArgs::Object(fake_id as u64))
    }
}

impl SuiValue {
    fn assert_move_value(self) -> MoveValue {
        match self {
            SuiValue::MoveValue(v) => v,
            SuiValue::Object(_) => panic!("unexpected nested Sui object in args"),
            SuiValue::ObjVec(_) => panic!("unexpected nested Sui object vector in args"),
        }
    }

    fn assert_object(self) -> u64 {
        match self {
            SuiValue::MoveValue(_) => panic!("unexpected nested non-object value in args"),
            SuiValue::Object(v) => v,
            SuiValue::ObjVec(_) => panic!("unexpected nested Sui object vector in args"),
        }
    }

    fn object_arg(fake_id: u64, test_adapter: &SuiTestAdapter) -> anyhow::Result<ObjectArg> {
        let id = match test_adapter.fake_to_real_object_id(fake_id) {
            Some(id) => id,
            None => bail!("INVALID TEST. Unknown object, object({})", fake_id),
        };
        let obj = match test_adapter.storage.get_object(&id) {
            Some(obj) => obj,
            None => bail!("INVALID TEST. Could not load object argument {}", id),
        };
        match obj.owner {
            Owner::Shared {
                initial_shared_version,
            } => Ok(ObjectArg::SharedObject {
                id,
                initial_shared_version,
                mutable: true,
            }),
            Owner::AddressOwner(_) | Owner::ObjectOwner(_) | Owner::Immutable => {
                let obj_ref = obj.compute_object_reference();
                Ok(ObjectArg::ImmOrOwnedObject(obj_ref))
            }
        }
    }

    pub(crate) fn into_call_arg(self, test_adapter: &SuiTestAdapter) -> anyhow::Result<CallArg> {
        Ok(match self {
            SuiValue::Object(fake_id) => CallArg::Object(Self::object_arg(fake_id, test_adapter)?),
            SuiValue::MoveValue(v) => CallArg::Pure(v.simple_serialize().unwrap()),
            SuiValue::ObjVec(_) => bail!("obj vec is not supported as an input"),
        })
    }

    pub(crate) fn into_argument(
        self,
        builder: &mut ProgrammableTransactionBuilder,
        test_adapter: &SuiTestAdapter,
    ) -> anyhow::Result<Argument> {
        Ok(match self {
            SuiValue::Object(fake_id) => builder.obj(Self::object_arg(fake_id, test_adapter)?)?,
            SuiValue::ObjVec(vec) => builder.make_obj_vec(
                vec.iter()
                    .map(|fake_id| Self::object_arg(*fake_id, test_adapter))
                    .collect::<Result<Vec<ObjectArg>, _>>()?,
            )?,
            SuiValue::MoveValue(v) => {
                builder.input(CallArg::Pure(v.simple_serialize().unwrap()))?
            }
        })
    }
}

impl ParsableValue for SuiExtraValueArgs {
    type ConcreteValue = SuiValue;

    fn parse_value<'a, I: Iterator<Item = (ValueToken, &'a str)>>(
        parser: &mut MoveCLParser<'a, ValueToken, I>,
    ) -> Option<anyhow::Result<Self>> {
        match parser.peek()? {
            (ValueToken::Ident, "object") => Some(Self::parse_value_impl(parser)),
            _ => None,
        }
    }

    fn move_value_into_concrete(v: MoveValue) -> anyhow::Result<Self::ConcreteValue> {
        Ok(SuiValue::MoveValue(v))
    }

    fn concrete_vector(elems: Vec<Self::ConcreteValue>) -> anyhow::Result<Self::ConcreteValue> {
        if !elems.is_empty() && matches!(elems[0], SuiValue::Object(_)) {
            Ok(SuiValue::ObjVec(
                elems.into_iter().map(SuiValue::assert_object).collect(),
            ))
        } else {
            Ok(SuiValue::MoveValue(MoveValue::Vector(
                elems.into_iter().map(SuiValue::assert_move_value).collect(),
            )))
        }
    }

    fn concrete_struct(
        _addr: move_core_types::account_address::AccountAddress,
        _module: String,
        _name: String,
        values: std::collections::BTreeMap<String, Self::ConcreteValue>,
    ) -> anyhow::Result<Self::ConcreteValue> {
        Ok(SuiValue::MoveValue(MoveValue::Struct(
            MoveStruct::WithFields(
                values
                    .into_iter()
                    .map(|(f, v)| Ok((Identifier::new(f)?, v.assert_move_value())))
                    .collect::<anyhow::Result<_>>()?,
            ),
        )))
    }

    fn into_concrete_value(
        self,
        _mapping: &impl Fn(&str) -> Option<move_core_types::account_address::AccountAddress>,
    ) -> anyhow::Result<Self::ConcreteValue> {
        match self {
            SuiExtraValueArgs::Object(id) => Ok(SuiValue::Object(id)),
        }
    }
}
