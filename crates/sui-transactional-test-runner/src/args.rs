// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::test_adapter::{FakeID, SuiTestAdapter};
use anyhow::{bail, ensure};
use clap;
use clap::{Args, Parser};
use move_compiler::editions::Flavor;
use move_core_types::parsing::{
    parser::Parser as MoveCLParser,
    parser::{parse_u256, parse_u64},
    values::ValueToken,
    values::{ParsableValue, ParsedValue},
};
use move_core_types::runtime_value::{MoveStruct, MoveValue};
use move_core_types::u256::U256;
use move_symbol_pool::Symbol;
use move_transactional_test_runner::tasks::{RunCommand, SyntaxChoice};
use sui_graphql_rpc::test_infra::cluster::SnapshotLagConfig;
use sui_types::base_types::{SequenceNumber, SuiAddress};
use sui_types::move_package::UpgradePolicy;
use sui_types::object::{Object, Owner};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{Argument, CallArg, ObjectArg};

pub const SUI_ARGS_LONG: &str = "sui-args";

#[derive(Clone, Debug, clap::Parser)]
pub struct SuiRunArgs {
    #[clap(long = "sender")]
    pub sender: Option<String>,
    #[clap(long = "gas-price")]
    pub gas_price: Option<u64>,
    #[clap(long = "summarize")]
    pub summarize: bool,
}

#[derive(Debug, clap::Parser, Default)]
pub struct SuiPublishArgs {
    #[clap(long = "sender")]
    pub sender: Option<String>,
    #[clap(long = "upgradeable", action = clap::ArgAction::SetTrue)]
    pub upgradeable: bool,
    #[clap(long = "dependencies", num_args(1..))]
    pub dependencies: Vec<String>,
    #[clap(long = "gas-price")]
    pub gas_price: Option<u64>,
}

#[derive(Debug, clap::Parser)]
pub struct SuiInitArgs {
    #[clap(long = "accounts", num_args(1..))]
    pub accounts: Option<Vec<String>>,
    #[clap(long = "protocol-version")]
    pub protocol_version: Option<u64>,
    #[clap(long = "max-gas")]
    pub max_gas: Option<u64>,
    #[clap(long = "shared-object-deletion")]
    pub shared_object_deletion: Option<bool>,
    #[clap(long = "simulator")]
    pub simulator: bool,
    #[clap(long = "custom-validator-account")]
    pub custom_validator_account: bool,
    #[clap(long = "reference-gas-price")]
    pub reference_gas_price: Option<u64>,
    #[clap(long = "default-gas-price")]
    pub default_gas_price: Option<u64>,
    #[clap(flatten)]
    pub snapshot_config: SnapshotLagConfig,
    #[clap(long = "flavor")]
    pub flavor: Option<Flavor>,
    /// The number of epochs to keep in the database. Epochs outside of this range will be pruned by
    /// the indexer.
    #[clap(long = "epochs-to-keep")]
    pub epochs_to_keep: Option<u64>,
    /// Dir for simulacrum to write checkpoint files to. To be passed to the offchain indexer and
    /// reader.
    #[clap(long)]
    pub data_ingestion_path: Option<PathBuf>,
    /// URL for the Sui REST API. To be passed to the offchain indexer and reader.
    #[clap(long)]
    pub rest_api_url: Option<String>,
}

#[derive(Debug, clap::Parser)]
pub struct ViewObjectCommand {
    #[clap(value_parser = parse_fake_id)]
    pub id: FakeID,
}

#[derive(Debug, clap::Parser)]
pub struct TransferObjectCommand {
    #[clap(value_parser = parse_fake_id)]
    pub id: FakeID,
    #[clap(long = "recipient")]
    pub recipient: String,
    #[clap(long = "sender")]
    pub sender: Option<String>,
    #[clap(long = "gas-budget")]
    pub gas_budget: Option<u64>,
    #[clap(long = "gas-price")]
    pub gas_price: Option<u64>,
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
    #[clap(long = "sponsor")]
    pub sponsor: Option<String>,
    #[clap(long = "gas-budget")]
    pub gas_budget: Option<u64>,
    #[clap(long = "gas-price")]
    pub gas_price: Option<u64>,
    #[clap(long = "gas-payment", value_parser = parse_fake_id)]
    pub gas_payment: Option<FakeID>,
    #[clap(long = "dev-inspect")]
    pub dev_inspect: bool,
    #[clap(long = "dry-run")]
    pub dry_run: bool,
    #[clap(
        long = "inputs",
        value_parser = ParsedValue::<SuiExtraValueArgs>::parse,
        num_args(1..),
        action = clap::ArgAction::Append,
    )]
    pub inputs: Vec<ParsedValue<SuiExtraValueArgs>>,
}

#[derive(Debug, clap::Parser)]
pub struct UpgradePackageCommand {
    #[clap(long = "package")]
    pub package: String,
    #[clap(long = "upgrade-capability", value_parser = parse_fake_id)]
    pub upgrade_capability: FakeID,
    #[clap(long = "dependencies", num_args(1..))]
    pub dependencies: Vec<String>,
    #[clap(long = "sender")]
    pub sender: String,
    #[clap(long = "gas-budget")]
    pub gas_budget: Option<u64>,
    #[clap(long = "syntax")]
    pub syntax: Option<SyntaxChoice>,
    #[clap(long = "policy", default_value="compatible", value_parser = parse_policy)]
    pub policy: u8,
    #[clap(long = "gas-price")]
    pub gas_price: Option<u64>,
}

#[derive(Debug, clap::Parser)]
pub struct StagePackageCommand {
    #[clap(long = "syntax")]
    pub syntax: Option<SyntaxChoice>,
    #[clap(long = "dependencies", num_args(1..))]
    pub dependencies: Vec<String>,
}

#[derive(Debug, clap::Parser)]
pub struct SetAddressCommand {
    pub address: String,
    #[clap(value_parser = ParsedValue::<SuiExtraValueArgs>::parse)]
    pub input: ParsedValue<SuiExtraValueArgs>,
}

#[derive(Debug, clap::Parser)]
pub struct AdvanceClockCommand {
    #[clap(long = "duration-ns")]
    pub duration_ns: u64,
}

#[derive(Debug, clap::Parser)]
pub struct RunGraphqlCommand {
    #[clap(long = "show-usage")]
    pub show_usage: bool,
    #[clap(long = "show-headers")]
    pub show_headers: bool,
    #[clap(long = "show-service-version")]
    pub show_service_version: bool,
    #[clap(long, num_args(1..))]
    pub cursors: Vec<String>,
    #[clap(long)]
    pub wait_for_checkpoint_pruned: Option<u64>,
}

#[derive(Debug, clap::Parser)]
pub struct RunJsonRpcCommand {
    #[clap(long = "show-headers")]
    pub show_headers: bool,
    #[clap(long, num_args(1..))]
    pub cursors: Vec<String>,
}

#[derive(Debug, clap::Parser)]
pub struct CreateCheckpointCommand {
    pub count: Option<u64>,
}

#[derive(Debug, clap::Parser)]
pub struct AdvanceEpochCommand {
    pub count: Option<u64>,
    #[clap(long = "create-random-state")]
    pub create_random_state: bool,
}

#[derive(Debug, clap::Parser)]
pub struct SetRandomStateCommand {
    #[clap(long = "randomness-round")]
    pub randomness_round: u64,
    #[clap(long = "random-bytes")]
    pub random_bytes: String,
    #[clap(long = "randomness-initial-version")]
    pub randomness_initial_version: u64,
}

#[derive(Debug)]
pub enum SuiSubcommand<ExtraValueArgs: ParsableValue, ExtraRunArgs: Parser> {
    ViewObject(ViewObjectCommand),
    TransferObject(TransferObjectCommand),
    ConsensusCommitPrologue(ConsensusCommitPrologueCommand),
    ProgrammableTransaction(ProgrammableTransactionCommand),
    UpgradePackage(UpgradePackageCommand),
    StagePackage(StagePackageCommand),
    SetAddress(SetAddressCommand),
    CreateCheckpoint(CreateCheckpointCommand),
    AdvanceEpoch(AdvanceEpochCommand),
    AdvanceClock(AdvanceClockCommand),
    SetRandomState(SetRandomStateCommand),
    ViewCheckpoint,
    RunGraphql(RunGraphqlCommand),
    RunJsonRpc(RunJsonRpcCommand),
    Bench(RunCommand<ExtraValueArgs>, ExtraRunArgs),
}

impl<ExtraValueArgs: ParsableValue, ExtraRunArgs: Parser> clap::FromArgMatches
    for SuiSubcommand<ExtraValueArgs, ExtraRunArgs>
{
    fn from_arg_matches(matches: &clap::ArgMatches) -> Result<Self, clap::Error> {
        Ok(match matches.subcommand() {
            Some(("view-object", matches)) => {
                SuiSubcommand::ViewObject(ViewObjectCommand::from_arg_matches(matches)?)
            }
            Some(("transfer-object", matches)) => {
                SuiSubcommand::TransferObject(TransferObjectCommand::from_arg_matches(matches)?)
            }
            Some(("consensus-commit-prologue", matches)) => SuiSubcommand::ConsensusCommitPrologue(
                ConsensusCommitPrologueCommand::from_arg_matches(matches)?,
            ),
            Some(("programmable", matches)) => SuiSubcommand::ProgrammableTransaction(
                ProgrammableTransactionCommand::from_arg_matches(matches)?,
            ),
            Some(("upgrade", matches)) => {
                SuiSubcommand::UpgradePackage(UpgradePackageCommand::from_arg_matches(matches)?)
            }
            Some(("stage-package", matches)) => {
                SuiSubcommand::StagePackage(StagePackageCommand::from_arg_matches(matches)?)
            }
            Some(("set-address", matches)) => {
                SuiSubcommand::SetAddress(SetAddressCommand::from_arg_matches(matches)?)
            }
            Some(("create-checkpoint", matches)) => {
                SuiSubcommand::CreateCheckpoint(CreateCheckpointCommand::from_arg_matches(matches)?)
            }
            Some(("advance-epoch", matches)) => {
                SuiSubcommand::AdvanceEpoch(AdvanceEpochCommand::from_arg_matches(matches)?)
            }
            Some(("advance-clock", matches)) => {
                SuiSubcommand::AdvanceClock(AdvanceClockCommand::from_arg_matches(matches)?)
            }
            Some(("set-random-state", matches)) => {
                SuiSubcommand::SetRandomState(SetRandomStateCommand::from_arg_matches(matches)?)
            }
            Some(("view-checkpoint", _)) => SuiSubcommand::ViewCheckpoint,
            Some(("run-graphql", matches)) => {
                SuiSubcommand::RunGraphql(RunGraphqlCommand::from_arg_matches(matches)?)
            }
            Some(("run-jsonrpc", matches)) => {
                SuiSubcommand::RunJsonRpc(RunJsonRpcCommand::from_arg_matches(matches)?)
            }
            Some(("bench", matches)) => SuiSubcommand::Bench(
                RunCommand::from_arg_matches(matches)?,
                ExtraRunArgs::from_arg_matches(matches)?,
            ),
            _ => {
                return Err(clap::Error::raw(
                    clap::error::ErrorKind::InvalidSubcommand,
                    "Invalid submcommand",
                ));
            }
        })
    }

    fn update_from_arg_matches(&mut self, matches: &clap::ArgMatches) -> Result<(), clap::Error> {
        *self = Self::from_arg_matches(matches)?;
        Ok(())
    }
}

impl<ExtraValueArgs: ParsableValue, ExtraRunArgs: Parser> clap::CommandFactory
    for SuiSubcommand<ExtraValueArgs, ExtraRunArgs>
{
    fn command() -> clap::Command {
        clap::Command::new("sui_sub_command")
            .subcommand(ViewObjectCommand::command().name("view-object"))
            .subcommand(TransferObjectCommand::command().name("transfer-object"))
            .subcommand(ConsensusCommitPrologueCommand::command().name("consensus-commit-prologue"))
            .subcommand(ProgrammableTransactionCommand::command().name("programmable"))
            .subcommand(UpgradePackageCommand::command().name("upgrade"))
            .subcommand(StagePackageCommand::command().name("stage-package"))
            .subcommand(SetAddressCommand::command().name("set-address"))
            .subcommand(CreateCheckpointCommand::command().name("create-checkpoint"))
            .subcommand(AdvanceEpochCommand::command().name("advance-epoch"))
            .subcommand(AdvanceClockCommand::command().name("advance-clock"))
            .subcommand(SetRandomStateCommand::command().name("set-random-state"))
            .subcommand(clap::Command::new("view-checkpoint"))
            .subcommand(RunGraphqlCommand::command().name("run-graphql"))
            .subcommand(RunJsonRpcCommand::command().name("run-jsonrpc"))
            .subcommand(
                RunCommand::<ExtraValueArgs>::augment_args(ExtraRunArgs::command()).name("bench"),
            )
    }

    fn command_for_update() -> clap::Command {
        todo!()
    }
}

impl<ExtraValueArgs: ParsableValue, ExtraRunArgs: Parser> clap::Parser
    for SuiSubcommand<ExtraValueArgs, ExtraRunArgs>
{
}

#[derive(Clone, Debug)]
pub enum SuiExtraValueArgs {
    Object(FakeID, Option<SequenceNumber>),
    Digest(String),
    Receiving(FakeID, Option<SequenceNumber>),
    ImmShared(FakeID, Option<SequenceNumber>),
}

#[derive(Clone)]
pub enum SuiValue {
    MoveValue(MoveValue),
    Object(FakeID, Option<SequenceNumber>),
    ObjVec(Vec<(FakeID, Option<SequenceNumber>)>),
    Digest(String),
    Receiving(FakeID, Option<SequenceNumber>),
    ImmShared(FakeID, Option<SequenceNumber>),
}

impl SuiExtraValueArgs {
    fn parse_object_value<'a, I: Iterator<Item = (ValueToken, &'a str)>>(
        parser: &mut MoveCLParser<'a, ValueToken, I>,
    ) -> anyhow::Result<Self> {
        let (fake_id, version) = Self::parse_receiving_or_object_value(parser, "object")?;
        Ok(SuiExtraValueArgs::Object(fake_id, version))
    }

    fn parse_receiving_value<'a, I: Iterator<Item = (ValueToken, &'a str)>>(
        parser: &mut MoveCLParser<'a, ValueToken, I>,
    ) -> anyhow::Result<Self> {
        let (fake_id, version) = Self::parse_receiving_or_object_value(parser, "receiving")?;
        Ok(SuiExtraValueArgs::Receiving(fake_id, version))
    }

    fn parse_read_shared_value<'a, I: Iterator<Item = (ValueToken, &'a str)>>(
        parser: &mut MoveCLParser<'a, ValueToken, I>,
    ) -> anyhow::Result<Self> {
        let (fake_id, version) = Self::parse_receiving_or_object_value(parser, "immshared")?;
        Ok(SuiExtraValueArgs::ImmShared(fake_id, version))
    }

    fn parse_digest_value<'a, I: Iterator<Item = (ValueToken, &'a str)>>(
        parser: &mut MoveCLParser<'a, ValueToken, I>,
    ) -> anyhow::Result<Self> {
        let contents = parser.advance(ValueToken::Ident)?;
        ensure!(contents == "digest");
        parser.advance(ValueToken::LParen)?;
        let package = parser.advance(ValueToken::Ident)?;
        parser.advance(ValueToken::RParen)?;
        Ok(SuiExtraValueArgs::Digest(package.to_owned()))
    }

    fn parse_receiving_or_object_value<'a, I: Iterator<Item = (ValueToken, &'a str)>>(
        parser: &mut MoveCLParser<'a, ValueToken, I>,
        ident_name: &str,
    ) -> anyhow::Result<(FakeID, Option<SequenceNumber>)> {
        let contents = parser.advance(ValueToken::Ident)?;
        ensure!(contents == ident_name);
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
        let version = if let Some(ValueToken::AtSign) = parser.peek_tok() {
            parser.advance(ValueToken::AtSign)?;
            let v_str = parser.advance(ValueToken::Number)?;
            let (v, _) = parse_u64(v_str)?;
            Some(SequenceNumber::from_u64(v))
        } else {
            None
        };
        Ok((fake_id, version))
    }
}

impl SuiValue {
    fn assert_move_value(self) -> MoveValue {
        match self {
            SuiValue::MoveValue(v) => v,
            SuiValue::Object(_, _) => panic!("unexpected nested Sui object in args"),
            SuiValue::ObjVec(_) => panic!("unexpected nested Sui object vector in args"),
            SuiValue::Digest(_) => panic!("unexpected nested Sui package digest in args"),
            SuiValue::Receiving(_, _) => panic!("unexpected nested Sui receiving object in args"),
            SuiValue::ImmShared(_, _) => panic!("unexpected nested Sui shared object in args"),
        }
    }

    fn assert_object(self) -> (FakeID, Option<SequenceNumber>) {
        match self {
            SuiValue::MoveValue(_) => panic!("unexpected nested non-object value in args"),
            SuiValue::Object(id, version) => (id, version),
            SuiValue::ObjVec(_) => panic!("unexpected nested Sui object vector in args"),
            SuiValue::Digest(_) => panic!("unexpected nested Sui package digest in args"),
            SuiValue::Receiving(_, _) => panic!("unexpected nested Sui receiving object in args"),
            SuiValue::ImmShared(_, _) => panic!("unexpected nested Sui shared object in args"),
        }
    }

    fn resolve_object(
        fake_id: FakeID,
        version: Option<SequenceNumber>,
        test_adapter: &SuiTestAdapter,
    ) -> anyhow::Result<Object> {
        let id = match test_adapter.fake_to_real_object_id(fake_id) {
            Some(id) => id,
            None => bail!("INVALID TEST. Unknown object, object({})", fake_id),
        };
        let obj_res = if let Some(v) = version {
            sui_types::storage::ObjectStore::get_object_by_key(&*test_adapter.executor, &id, v)
        } else {
            sui_types::storage::ObjectStore::get_object(&*test_adapter.executor, &id)
        };
        let obj = match obj_res {
            Some(obj) => obj,
            None => bail!("INVALID TEST. Could not load object argument {}", id),
        };
        Ok(obj)
    }

    fn receiving_arg(
        fake_id: FakeID,
        version: Option<SequenceNumber>,
        test_adapter: &SuiTestAdapter,
    ) -> anyhow::Result<ObjectArg> {
        let obj = Self::resolve_object(fake_id, version, test_adapter)?;
        Ok(ObjectArg::Receiving(obj.compute_object_reference()))
    }

    fn read_shared_arg(
        fake_id: FakeID,
        version: Option<SequenceNumber>,
        test_adapter: &SuiTestAdapter,
    ) -> anyhow::Result<ObjectArg> {
        let obj = Self::resolve_object(fake_id, version, test_adapter)?;
        let id = obj.id();
        if let Owner::Shared {
            initial_shared_version,
        } = obj.owner
        {
            Ok(ObjectArg::SharedObject {
                id,
                initial_shared_version,
                mutable: false,
            })
        } else {
            bail!("{fake_id} is not a shared object.")
        }
    }

    fn object_arg(
        fake_id: FakeID,
        version: Option<SequenceNumber>,
        test_adapter: &SuiTestAdapter,
    ) -> anyhow::Result<ObjectArg> {
        let obj = Self::resolve_object(fake_id, version, test_adapter)?;
        let id = obj.id();
        match obj.owner {
            Owner::Shared {
                initial_shared_version,
            }
            | Owner::ConsensusV2 {
                start_version: initial_shared_version,
                ..
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
            SuiValue::Object(fake_id, version) => {
                CallArg::Object(Self::object_arg(fake_id, version, test_adapter)?)
            }
            SuiValue::MoveValue(v) => CallArg::Pure(v.simple_serialize().unwrap()),
            SuiValue::Receiving(fake_id, version) => {
                CallArg::Object(Self::receiving_arg(fake_id, version, test_adapter)?)
            }
            SuiValue::ImmShared(fake_id, version) => {
                CallArg::Object(Self::read_shared_arg(fake_id, version, test_adapter)?)
            }
            SuiValue::ObjVec(_) => bail!("obj vec is not supported as an input"),
            SuiValue::Digest(pkg) => {
                let pkg = Symbol::from(pkg);
                let Some(staged) = test_adapter.staged_modules.get(&pkg) else {
                    bail!("Unbound staged package '{pkg}'")
                };
                CallArg::Pure(bcs::to_bytes(&staged.digest).unwrap())
            }
        })
    }

    pub(crate) fn into_argument(
        self,
        builder: &mut ProgrammableTransactionBuilder,
        test_adapter: &SuiTestAdapter,
    ) -> anyhow::Result<Argument> {
        match self {
            SuiValue::ObjVec(vec) => builder.make_obj_vec(
                vec.iter()
                    .map(|(fake_id, version)| Self::object_arg(*fake_id, *version, test_adapter))
                    .collect::<Result<Vec<ObjectArg>, _>>()?,
            ),
            value => {
                let call_arg = value.into_call_arg(test_adapter)?;
                builder.input(call_arg)
            }
        }
    }
}

impl ParsableValue for SuiExtraValueArgs {
    type ConcreteValue = SuiValue;

    fn parse_value<'a, I: Iterator<Item = (ValueToken, &'a str)>>(
        parser: &mut MoveCLParser<'a, ValueToken, I>,
    ) -> Option<anyhow::Result<Self>> {
        match parser.peek()? {
            (ValueToken::Ident, "object") => Some(Self::parse_object_value(parser)),
            (ValueToken::Ident, "digest") => Some(Self::parse_digest_value(parser)),
            (ValueToken::Ident, "receiving") => Some(Self::parse_receiving_value(parser)),
            (ValueToken::Ident, "immshared") => Some(Self::parse_read_shared_value(parser)),
            _ => None,
        }
    }

    fn move_value_into_concrete(v: MoveValue) -> anyhow::Result<Self::ConcreteValue> {
        Ok(SuiValue::MoveValue(v))
    }

    fn concrete_vector(elems: Vec<Self::ConcreteValue>) -> anyhow::Result<Self::ConcreteValue> {
        if !elems.is_empty() && matches!(elems[0], SuiValue::Object(_, _)) {
            Ok(SuiValue::ObjVec(
                elems.into_iter().map(SuiValue::assert_object).collect(),
            ))
        } else {
            Ok(SuiValue::MoveValue(MoveValue::Vector(
                elems.into_iter().map(SuiValue::assert_move_value).collect(),
            )))
        }
    }

    fn concrete_struct(values: Vec<Self::ConcreteValue>) -> anyhow::Result<Self::ConcreteValue> {
        Ok(SuiValue::MoveValue(MoveValue::Struct(MoveStruct(
            values.into_iter().map(|v| v.assert_move_value()).collect(),
        ))))
    }

    fn into_concrete_value(
        self,
        _mapping: &impl Fn(&str) -> Option<move_core_types::account_address::AccountAddress>,
    ) -> anyhow::Result<Self::ConcreteValue> {
        match self {
            SuiExtraValueArgs::Object(id, version) => Ok(SuiValue::Object(id, version)),
            SuiExtraValueArgs::Digest(pkg) => Ok(SuiValue::Digest(pkg)),
            SuiExtraValueArgs::Receiving(id, version) => Ok(SuiValue::Receiving(id, version)),
            SuiExtraValueArgs::ImmShared(id, version) => Ok(SuiValue::ImmShared(id, version)),
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
