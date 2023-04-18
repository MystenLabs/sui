// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{bail, ensure};
use clap;
use move_command_line_common::parser::{parse_u256, parse_u64};
use move_command_line_common::values::{ParsableValue, ParsedValue};
use move_command_line_common::{parser::Parser as MoveCLParser, values::ValueToken};
use move_core_types::identifier::Identifier;
use move_core_types::u256::U256;
use move_core_types::value::{MoveStruct, MoveValue};
use move_transactional_test_runner::tasks::SyntaxChoice;
use sui_types::base_types::SuiAddress;
use sui_types::messages::{Argument, CallArg, ObjectArg};
use sui_types::move_package::UpgradePolicy;
use sui_types::object::Owner;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;

use crate::test_adapter::{FakeID, SuiTestAdapter};

pub const SUI_ARGS_LONG: &str = "sui-args";

#[derive(Debug, clap::Parser)]
pub struct SuiRunArgs {
    #[clap(long = "sender")]
    pub sender: Option<String>,
    #[clap(long = "gas-price")]
    pub gas_price: Option<u64>,
    /// If set, this will override the protocol version
    /// specified elsewhere (e.g., in init). Use with
    /// caution!
    #[clap(long = "protocol-version")]
    pub protocol_version: Option<u64>,
    #[clap(long = "uncharged")]
    pub uncharged: bool,
}

#[derive(Debug, clap::Parser)]
pub struct SuiPublishArgs {
    #[clap(long = "sender")]
    pub sender: Option<String>,
    #[clap(long = "upgradeable", action = clap::ArgAction::SetTrue)]
    pub upgradeable: bool,
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
    #[clap(long = "protocol_version")]
    pub protocol_version: Option<u64>,
}

#[derive(Debug, clap::Parser)]
pub struct ViewObjectCommand {
    #[clap(parse(try_from_str = parse_fake_id))]
    pub id: FakeID,
}

#[derive(Debug, clap::Parser)]
pub struct TransferObjectCommand {
    #[clap(parse(try_from_str = parse_fake_id))]
    pub id: FakeID,
    #[clap(long = "recipient")]
    pub recipient: String,
    #[clap(long = "sender")]
    pub sender: Option<String>,
    #[clap(long = "gas-budget")]
    pub gas_budget: Option<u64>,
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
    #[clap(long = "gas-price")]
    pub gas_price: Option<u64>,
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
pub struct UpgradePackageCommand {
    #[clap(long = "package")]
    pub package: String,
    #[clap(long = "upgrade-capability", parse(try_from_str = parse_fake_id))]
    pub upgrade_capability: FakeID,
    #[clap(
        long = "dependencies",
        multiple_values(true),
        multiple_occurrences(false)
    )]
    pub dependencies: Vec<String>,
    #[clap(long = "sender")]
    pub sender: String,
    #[clap(long = "gas-budget")]
    pub gas_budget: Option<u64>,
    #[clap(long = "syntax")]
    pub syntax: Option<SyntaxChoice>,
    #[clap(long = "policy", default_value="compatible", parse(try_from_str = parse_policy))]
    pub policy: u8,
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
    #[clap(name = "upgrade")]
    UpgradePackage(UpgradePackageCommand),
}

#[derive(Debug)]
pub enum SuiExtraValueArgs {
    Object(FakeID),
}

pub enum SuiValue {
    MoveValue(MoveValue),
    Object(FakeID),
    ObjVec(Vec<FakeID>),
}

impl SuiExtraValueArgs {
    fn parse_value_impl<'a, I: Iterator<Item = (ValueToken, &'a str)>>(
        parser: &mut MoveCLParser<'a, ValueToken, I>,
    ) -> anyhow::Result<Self> {
        let contents = parser.advance(ValueToken::Ident)?;
        ensure!(contents == "object");
        parser.advance(ValueToken::LParen)?;
        let i_str = parser.advance(ValueToken::Number)?;
        let (i, _) = parse_u256(i_str)?;
        let fake_id = if let Some(ValueToken::Comma) = parser.peek_tok() {
            parser.advance(ValueToken::Comma)?;
            let j_str = parser.advance(ValueToken::Number)?;
            let (j, _) = parse_u64(j_str)?;
            if i > U256::from(u64::MAX) {
                bail!("Object ID too large")
            }
            FakeID::Enumerated(i.unchecked_as_u64(), j)
        } else {
            let mut u256_bytes = i.to_le_bytes().to_vec();
            u256_bytes.reverse();
            let address: SuiAddress = SuiAddress::from_bytes(&u256_bytes).unwrap();
            FakeID::Known(address.into())
        };
        parser.advance(ValueToken::RParen)?;
        Ok(SuiExtraValueArgs::Object(fake_id))
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

    fn assert_object(self) -> FakeID {
        match self {
            SuiValue::MoveValue(_) => panic!("unexpected nested non-object value in args"),
            SuiValue::Object(v) => v,
            SuiValue::ObjVec(_) => panic!("unexpected nested Sui object vector in args"),
        }
    }

    fn object_arg(fake_id: FakeID, test_adapter: &SuiTestAdapter) -> anyhow::Result<ObjectArg> {
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

fn parse_fake_id(s: &str) -> anyhow::Result<FakeID> {
    Ok(if let Some((s1, s2)) = s.split_once(',') {
        let (i, _) = parse_u64(s1)?;
        let (j, _) = parse_u64(s2)?;
        FakeID::Enumerated(i, j)
    } else {
        let (i, _) = parse_u256(s)?;
        let mut u256_bytes = i.to_le_bytes().to_vec();
        u256_bytes.reverse();
        let address: SuiAddress = SuiAddress::from_bytes(&u256_bytes).unwrap();
        FakeID::Known(address.into())
    })
}

fn parse_policy(x: &str) -> anyhow::Result<u8> {
    Ok(match x {
            "compatible" => UpgradePolicy::COMPATIBLE,
            "additive" => UpgradePolicy::ADDITIVE,
            "dep_only" => UpgradePolicy::DEP_ONLY,
        _ => bail!("Invalid upgrade policy {x}. Policy must be one of 'compatible', 'additive', or 'dep_only'")
    })
}
