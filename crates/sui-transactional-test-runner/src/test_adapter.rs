// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This module contains the transactional test runner instantiation for the Sui adapter

use crate::offchain_state::OffchainStateReader;
use crate::simulator_persisted_store::PersistedStore;
use crate::{args::*, programmable_transaction_test_parser::parser::ParsedCommand};
use crate::{TransactionalAdapter, ValidatorWithFullnode};
use anyhow::{anyhow, bail, Context};
use async_trait::async_trait;
use bimap::btree::BiBTreeMap;
use criterion::Criterion;
use fastcrypto::ed25519::Ed25519KeyPair;
use fastcrypto::encoding::{Base64, Encoding};
use fastcrypto::traits::ToFromBytes;
use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_command_line_common::files::verify_and_create_named_address_mapping;
use move_compiler::{
    editions::{Edition, Flavor},
    shared::{NumberFormat, NumericalAddress, PackageConfig, PackagePaths},
    Flags, FullyCompiledProgram,
};
use move_core_types::ident_str;
use move_core_types::parsing::address::ParsedAddress;
use move_core_types::{
    account_address::AccountAddress,
    identifier::IdentStr,
    language_storage::{ModuleId, TypeTag},
};
use move_symbol_pool::Symbol;
use move_transactional_test_runner::framework::MaybeNamedCompiledModule;
use move_transactional_test_runner::tasks::TaskCommand;
use move_transactional_test_runner::{
    framework::{compile_any, store_modules, CompiledState, MoveTestAdapter},
    tasks::{InitCommand, RunCommand, SyntaxChoice, TaskInput},
};
use move_vm_runtime::session::SerializedReturnValues;
use once_cell::sync::Lazy;
use rand::{rngs::StdRng, Rng, SeedableRng};
use serde::Deserialize;
use serde_json::Value;
use std::fmt::{self, Write};
use std::path::PathBuf;
use std::time::Duration;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
};
use sui_core::authority::test_authority_builder::TestAuthorityBuilder;
use sui_core::authority::AuthorityState;
use sui_framework::DEFAULT_FRAMEWORK_PATH;
use sui_graphql_rpc::test_infra::cluster::{RetentionConfig, SnapshotLagConfig};
use sui_json_rpc_api::QUERY_MAX_RESULT_LIMIT;
use sui_json_rpc_types::{
    DevInspectResults, DryRunTransactionBlockResponse, SuiExecutionStatus,
    SuiTransactionBlockEffects, SuiTransactionBlockEffectsAPI, SuiTransactionBlockEvents,
};
use sui_protocol_config::{Chain, ProtocolConfig};
use sui_storage::{
    key_value_store::TransactionKeyValueStore, key_value_store_metrics::KeyValueStoreMetrics,
};
use sui_swarm_config::genesis_config::AccountConfig;
use sui_types::base_types::{SequenceNumber, VersionNumber};
use sui_types::committee::EpochId;
use sui_types::crypto::{get_authority_key_pair, RandomnessRound};
use sui_types::digests::{ConsensusCommitDigest, TransactionDigest, TransactionEventsDigest};
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents};
use sui_types::messages_checkpoint::{
    CheckpointContents, CheckpointContentsDigest, CheckpointSequenceNumber, VerifiedCheckpoint,
};
use sui_types::messages_consensus::ConsensusDeterminedVersionAssignments;
use sui_types::object::bounded_visitor::BoundedVisitor;
use sui_types::storage::ReadStore;
use sui_types::storage::{ObjectStore, RpcStateReader};
use sui_types::transaction::Command;
use sui_types::transaction::ProgrammableTransaction;
use sui_types::utils::to_sender_signed_transaction_with_multi_signers;
use sui_types::SUI_SYSTEM_ADDRESS;
use sui_types::{
    base_types::{ObjectID, ObjectRef, SuiAddress, SUI_ADDRESS_LENGTH},
    crypto::{get_key_pair_from_rng, AccountKeyPair},
    event::Event,
    object::{self, Object},
    transaction::{Transaction, TransactionData, TransactionDataAPI, VerifiedTransaction},
    MOVE_STDLIB_ADDRESS, SUI_CLOCK_OBJECT_ID, SUI_FRAMEWORK_ADDRESS, SUI_SYSTEM_STATE_OBJECT_ID,
};
use sui_types::{execution_status::ExecutionStatus, transaction::TransactionKind};
use sui_types::{gas::GasCostSummary, object::GAS_VALUE_FOR_TESTING};
use sui_types::{
    move_package::MovePackage,
    transaction::{Argument, CallArg},
};
use sui_types::{
    programmable_transaction_builder::ProgrammableTransactionBuilder, SUI_FRAMEWORK_PACKAGE_ID,
};
use sui_types::{utils::to_sender_signed_transaction, SUI_SYSTEM_PACKAGE_ID};
use sui_types::{BRIDGE_ADDRESS, MOVE_STDLIB_PACKAGE_ID};
use sui_types::{DEEPBOOK_ADDRESS, SUI_DENY_LIST_OBJECT_ID};
use sui_types::{DEEPBOOK_PACKAGE_ID, SUI_RANDOMNESS_STATE_OBJECT_ID};
use tempfile::{tempdir, NamedTempFile};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum FakeID {
    Known(ObjectID),
    Enumerated(u64, u64),
}

const DEFAULT_GAS_PRICE: u64 = 1_000;

const WELL_KNOWN_OBJECTS: &[ObjectID] = &[
    MOVE_STDLIB_PACKAGE_ID,
    DEEPBOOK_PACKAGE_ID,
    SUI_FRAMEWORK_PACKAGE_ID,
    SUI_SYSTEM_PACKAGE_ID,
    SUI_SYSTEM_STATE_OBJECT_ID,
    SUI_CLOCK_OBJECT_ID,
    SUI_DENY_LIST_OBJECT_ID,
    SUI_RANDOMNESS_STATE_OBJECT_ID,
];
// TODO use the file name as a seed
const RNG_SEED: [u8; 32] = [
    21, 23, 199, 200, 234, 250, 252, 178, 94, 15, 202, 178, 62, 186, 88, 137, 233, 192, 130, 157,
    179, 179, 65, 9, 31, 249, 221, 123, 225, 112, 199, 247,
];

const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;
const GAS_FOR_TESTING: u64 = GAS_VALUE_FOR_TESTING;

const DEFAULT_CHAIN_START_TIMESTAMP: u64 = 0;

/// Extra args related to configuring the indexer and reader.
// TODO: the configs are still tied to the indexer crate, eventually we'd like a new command that is
// more agnostic
pub struct OffChainConfig {
    pub snapshot_config: SnapshotLagConfig,
    pub retention_config: Option<RetentionConfig>,
    /// Dir for simulacrum to write checkpoint files to. To be passed to the offchain indexer if it
    /// uses file-based ingestion.
    pub data_ingestion_path: PathBuf,
    /// URL for the Sui REST API. To be passed to the offchain indexer if it uses the REST API.
    pub rest_api_url: Option<String>,
}

pub struct SuiTestAdapter {
    pub(crate) compiled_state: CompiledState,
    /// For upgrades: maps an upgraded package name to the original package name.
    package_upgrade_mapping: BTreeMap<Symbol, Symbol>,
    accounts: BTreeMap<String, TestAccount>,
    default_account: TestAccount,
    default_syntax: SyntaxChoice,
    object_enumeration: BiBTreeMap<ObjectID, FakeID>,
    /// Mapping from task ID to a transaction digest, for use in named variable substitution.
    digest_enumeration: BTreeMap<u64, TransactionDigest>,
    next_fake: (u64, u64),
    gas_price: u64,
    pub(crate) staged_modules: BTreeMap<Symbol, StagedPackage>,
    is_simulator: bool,
    pub(crate) executor: Box<dyn TransactionalAdapter>,
    /// If `is_simulator` is true, the executor will be a `Simulacrum`, and this will be a
    /// `RpcStateReader` that can be used to spawn the equivalent of a fullnode rest api. This can
    /// then be used to serve an indexer that reads from said rest api service.
    pub read_replica: Option<Arc<dyn RpcStateReader + Send + Sync>>,
    /// Configuration for offchain state reader read from the file itself, and can be passed to the
    /// specific indexing and reader flavor.
    pub offchain_config: Option<OffChainConfig>,
    /// A trait encapsulating methods to interact with offchain state.
    pub offchain_reader: Option<Box<dyn OffchainStateReader>>,
}

struct AdapterInitConfig {
    additional_mapping: BTreeMap<String, NumericalAddress>,
    account_names: BTreeSet<String>,
    protocol_config: ProtocolConfig,
    is_simulator: bool,
    custom_validator_account: bool,
    reference_gas_price: Option<u64>,
    default_gas_price: Option<u64>,
    flavor: Option<Flavor>,
    /// Configuration for offchain state reader read from the file itself, and can be passed to the
    /// specific indexing and reader flavor.
    offchain_config: Option<OffChainConfig>,
}

pub(crate) struct StagedPackage {
    file: NamedTempFile,
    syntax: SyntaxChoice,
    modules: Vec<MaybeNamedCompiledModule>,
    pub(crate) digest: Vec<u8>,
}

#[derive(Debug)]
struct TestAccount {
    address: SuiAddress,
    key_pair: AccountKeyPair,
    gas: ObjectID,
}

#[derive(Debug)]
struct TxnSummary {
    created: Vec<ObjectID>,
    mutated: Vec<ObjectID>,
    unwrapped: Vec<ObjectID>,
    deleted: Vec<ObjectID>,
    unwrapped_then_deleted: Vec<ObjectID>,
    wrapped: Vec<ObjectID>,
    unchanged_shared: Vec<ObjectID>,
    events: Vec<Event>,
    gas_summary: GasCostSummary,
}

impl AdapterInitConfig {
    fn from_args(init_cmd: InitCommand, sui_args: SuiInitArgs) -> Self {
        let InitCommand { named_addresses } = init_cmd;
        let SuiInitArgs {
            accounts,
            protocol_version,
            max_gas,
            shared_object_deletion,
            simulator,
            custom_validator_account,
            reference_gas_price,
            default_gas_price,
            snapshot_config,
            flavor,
            epochs_to_keep,
            data_ingestion_path,
            rest_api_url,
        } = sui_args;

        let map = verify_and_create_named_address_mapping(named_addresses).unwrap();
        let accounts = accounts
            .map(|v| v.into_iter().collect::<BTreeSet<_>>())
            .unwrap_or_default();

        let mut protocol_config = if let Some(protocol_version) = protocol_version {
            ProtocolConfig::get_for_version(protocol_version.into(), Chain::Unknown)
        } else {
            ProtocolConfig::get_for_max_version_UNSAFE()
        };
        if let Some(enable) = shared_object_deletion {
            protocol_config.set_shared_object_deletion_for_testing(enable);
        }
        if let Some(mx_tx_gas_override) = max_gas {
            if simulator {
                panic!("Cannot set max gas in simulator mode");
            }
            protocol_config.set_max_tx_gas_for_testing(mx_tx_gas_override)
        }
        if custom_validator_account && !simulator {
            panic!("Can only set custom validator account in simulator mode");
        }
        if reference_gas_price.is_some() && !simulator {
            panic!("Can only set reference gas price in simulator mode");
        }

        let offchain_config = if simulator {
            let retention_config =
                epochs_to_keep.map(RetentionConfig::new_with_default_retention_only_for_testing);

            Some(OffChainConfig {
                snapshot_config,
                retention_config,
                data_ingestion_path: data_ingestion_path.unwrap_or(tempdir().unwrap().into_path()),
                rest_api_url,
            })
        } else {
            None
        };

        Self {
            additional_mapping: map,
            account_names: accounts,
            protocol_config,
            is_simulator: simulator,
            custom_validator_account,
            reference_gas_price,
            default_gas_price,
            flavor,
            offchain_config,
        }
    }
}

#[async_trait]
impl<'a> MoveTestAdapter<'a> for SuiTestAdapter {
    type ExtraPublishArgs = SuiPublishArgs;
    type ExtraRunArgs = SuiRunArgs;
    type ExtraInitArgs = SuiInitArgs;
    type ExtraValueArgs = SuiExtraValueArgs;
    type Subcommand = SuiSubcommand<Self::ExtraValueArgs, Self::ExtraRunArgs>;

    fn render_command_input(
        &self,
        task: &TaskInput<
            TaskCommand<
                Self::ExtraInitArgs,
                Self::ExtraPublishArgs,
                Self::ExtraValueArgs,
                Self::ExtraRunArgs,
                Self::Subcommand,
            >,
        >,
    ) -> Option<String> {
        match &task.command {
            TaskCommand::Subcommand(SuiSubcommand::ProgrammableTransaction(..)) => {
                let data_str = std::fs::read_to_string(task.data.as_ref()?)
                    .ok()?
                    .trim()
                    .to_string();
                Some(format!("{}\n{}", task.task_text, data_str))
            }
            TaskCommand::Init(_, _)
            | TaskCommand::PrintBytecode(_)
            | TaskCommand::Publish(_, _)
            | TaskCommand::Run(_, _)
            | TaskCommand::Subcommand(..) => None,
        }
    }

    fn compiled_state(&mut self) -> &mut CompiledState {
        &mut self.compiled_state
    }

    fn default_syntax(&self) -> SyntaxChoice {
        self.default_syntax
    }

    async fn init(
        default_syntax: SyntaxChoice,
        pre_compiled_deps: Option<Arc<FullyCompiledProgram>>,
        task_opt: Option<
            move_transactional_test_runner::tasks::TaskInput<(
                move_transactional_test_runner::tasks::InitCommand,
                Self::ExtraInitArgs,
            )>,
        >,
        _path: &Path,
    ) -> (Self, Option<String>) {
        let rng = StdRng::from_seed(RNG_SEED);
        assert!(
            pre_compiled_deps.is_some(),
            "Must populate 'pre_compiled_deps' with Sui framework"
        );

        // Unpack the init arguments
        let AdapterInitConfig {
            additional_mapping,
            account_names,
            protocol_config,
            is_simulator,
            custom_validator_account,
            reference_gas_price,
            default_gas_price,
            flavor,
            offchain_config,
        } = match task_opt.map(|t| t.command) {
            Some((init_cmd, sui_args)) => AdapterInitConfig::from_args(init_cmd, sui_args),
            None => AdapterInitConfig::default(),
        };

        let (
            executor,
            AccountSetup {
                default_account,
                accounts,
                named_address_mapping,
                objects,
                account_objects,
            },
            read_replica,
        ) = if is_simulator {
            init_sim_executor(
                rng,
                account_names,
                additional_mapping,
                &protocol_config,
                custom_validator_account,
                reference_gas_price,
                offchain_config
                    .as_ref()
                    .unwrap()
                    .data_ingestion_path
                    .clone(),
            )
            .await
        } else {
            init_val_fullnode_executor(rng, account_names, additional_mapping, &protocol_config)
                .await
        };

        let object_ids = objects.iter().map(|obj| obj.id()).collect::<Vec<_>>();

        let mut test_adapter = Self {
            is_simulator,
            // This is opt-in and instantiated later
            offchain_reader: None,
            executor,
            offchain_config,
            read_replica,
            compiled_state: CompiledState::new(
                named_address_mapping,
                pre_compiled_deps,
                Some(NumericalAddress::new(
                    AccountAddress::ZERO.into_bytes(),
                    NumberFormat::Hex,
                )),
                Some(Edition::DEVELOPMENT),
                flavor.or(Some(Flavor::Sui)),
            ),
            package_upgrade_mapping: BTreeMap::new(),
            accounts,
            default_account,
            default_syntax,
            object_enumeration: BiBTreeMap::new(),
            digest_enumeration: BTreeMap::new(),
            next_fake: (0, 0),
            // TODO: make this configurable
            gas_price: default_gas_price.unwrap_or(DEFAULT_GAS_PRICE),
            staged_modules: BTreeMap::new(),
        };

        for well_known in WELL_KNOWN_OBJECTS.iter().copied() {
            test_adapter
                .object_enumeration
                .insert(well_known, FakeID::Known(well_known));
        }
        let mut output = String::new();
        for (account, obj_id) in account_objects {
            let fake = test_adapter.enumerate_fake(obj_id);
            if !output.is_empty() {
                output.push_str(", ")
            }
            write!(output, "{}: object({})", account, fake).unwrap()
        }
        for object_id in object_ids {
            test_adapter.enumerate_fake(object_id);
        }
        let output = if output.is_empty() {
            None
        } else {
            Some(output)
        };
        (test_adapter, output)
    }

    async fn publish_modules(
        &mut self,
        modules: Vec<MaybeNamedCompiledModule>,
        gas_budget: Option<u64>,
        extra: Self::ExtraPublishArgs,
    ) -> anyhow::Result<(Option<String>, Vec<MaybeNamedCompiledModule>)> {
        self.next_task();
        let SuiPublishArgs {
            sender,
            upgradeable,
            dependencies,
            gas_price,
        } = extra;
        let named_addr_opt = modules.first().unwrap().named_address;
        let first_module_name = modules.first().unwrap().module.self_id().name().to_string();
        let modules_bytes = modules
            .iter()
            .map(|m| {
                let mut module_bytes = vec![];
                m.module
                    .serialize_with_version(m.module.version, &mut module_bytes)
                    .unwrap();
                Ok(module_bytes)
            })
            .collect::<anyhow::Result<_>>()?;
        let gas_budget = gas_budget.unwrap_or(DEFAULT_GAS_BUDGET);
        let mapping = &self.compiled_state.named_address_mapping;
        let mut dependencies: Vec<_> = dependencies
            .into_iter()
            .map(|d| {
                let Some(addr) = mapping.get(&d) else {
                    bail!("There is no published module address corresponding to name address {d}");
                };
                let id: ObjectID = addr.into_inner().into();
                Ok(id)
            })
            .collect::<Result<_, _>>()?;
        let gas_price = gas_price.unwrap_or(self.gas_price);
        // we are assuming that all packages depend on Move Stdlib and Sui Framework, so these
        // don't have to be provided explicitly as parameters
        dependencies.extend([MOVE_STDLIB_PACKAGE_ID, SUI_FRAMEWORK_PACKAGE_ID]);
        let data = |sender, gas| {
            let mut builder = ProgrammableTransactionBuilder::new();
            if upgradeable {
                let cap = builder.publish_upgradeable(modules_bytes, dependencies);
                builder.transfer_arg(sender, cap);
            } else {
                builder.publish_immutable(modules_bytes, dependencies);
            };
            let pt = builder.finish();
            TransactionData::new_programmable(sender, vec![gas], pt, gas_budget, gas_price)
        };
        let transaction = self.sign_txn(sender, data);
        let summary = self.execute_txn(transaction).await?;
        let created_package = summary
            .created
            .iter()
            .find_map(|id| {
                let object = self.get_object(id, None).unwrap();
                let package = object.data.try_as_package()?;
                if package
                    .serialized_module_map()
                    .get(&first_module_name)
                    .is_some()
                {
                    Some(*id)
                } else {
                    None
                }
            })
            .unwrap();
        let package_addr = NumericalAddress::new(created_package.into_bytes(), NumberFormat::Hex);
        if let Some(named_addr) = named_addr_opt {
            let prev_package = self
                .compiled_state
                .named_address_mapping
                .insert(named_addr.to_string(), package_addr);
            match prev_package.map(|a| a.into_inner()) {
                Some(addr) if addr != AccountAddress::ZERO => panic!(
                    "Cannot reuse named address '{}' for multiple packages. \
                It should be set to 0 initially",
                    named_addr
                ),
                _ => (),
            }
        }
        let output = self.object_summary_output(&summary, /* summarize */ false);
        let published_modules = self
            .get_object(&created_package, None)
            .unwrap()
            .data
            .try_as_package()
            .unwrap()
            .serialized_module_map()
            .iter()
            .map(|(_, published_module_bytes)| MaybeNamedCompiledModule {
                named_address: named_addr_opt,
                module: CompiledModule::deserialize_with_defaults(published_module_bytes).unwrap(),
                source_map: None,
            })
            .collect();
        Ok((output, published_modules))
    }

    async fn call_function(
        &mut self,
        module_id: &ModuleId,
        function: &IdentStr,
        type_args: Vec<TypeTag>,
        signers: Vec<ParsedAddress>,
        args: Vec<SuiValue>,
        gas_budget: Option<u64>,
        extra: Self::ExtraRunArgs,
    ) -> anyhow::Result<(Option<String>, SerializedReturnValues)> {
        self.next_task();
        let SuiRunArgs { summarize, .. } = extra;
        let transaction = self.build_function_call_tx(
            module_id, function, type_args, signers, args, gas_budget, extra,
        )?;
        let summary = self.execute_txn(transaction).await?;
        let output = self.object_summary_output(&summary, summarize);
        let empty = SerializedReturnValues {
            mutable_reference_outputs: vec![],
            return_values: vec![],
        };
        Ok((output, empty))
    }

    async fn handle_subcommand(
        &mut self,
        task: TaskInput<Self::Subcommand>,
    ) -> anyhow::Result<Option<String>> {
        self.next_task();
        let TaskInput {
            command,
            name,
            number,
            start_line,
            command_lines_stop,
            stop_line,
            data,
            task_text,
        } = task;
        macro_rules! get_obj {
            ($fake_id:ident, $version:expr) => {{
                let id = match self.fake_to_real_object_id($fake_id) {
                    None => bail!(
                        "task {}, lines {}-{}\n{}\n. Unbound fake id {}",
                        number,
                        start_line,
                        command_lines_stop,
                        task_text,
                        $fake_id
                    ),
                    Some(res) => res,
                };
                match self.get_object(&id, $version) {
                    Err(_) => return Ok(Some(format!("No object at id {}", $fake_id))),
                    Ok(obj) => obj,
                }
            }};
            ($fake_id:ident) => {{
                get_obj!($fake_id, None)
            }};
        }
        match command {
            SuiSubcommand::RunGraphql(RunGraphqlCommand {
                show_usage,
                show_headers,
                show_service_version,
                wait_for_checkpoint_pruned,
                cursors,
            }) => {
                let file = data.ok_or_else(|| anyhow::anyhow!("Missing GraphQL query"))?;
                let contents = std::fs::read_to_string(file.path())?;
                let offchain_reader = self
                    .offchain_reader
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Offchain reader not set"))?;
                let highest_checkpoint = self.executor.get_latest_checkpoint_sequence_number()?;
                offchain_reader
                    .wait_for_checkpoint_catchup(highest_checkpoint, Duration::from_secs(60))
                    .await;

                if let Some(checkpoint_to_prune) = wait_for_checkpoint_pruned {
                    offchain_reader
                        .wait_for_pruned_checkpoint(checkpoint_to_prune, Duration::from_secs(60))
                        .await;
                }

                let interpolated =
                    self.interpolate_query(&contents, &cursors, highest_checkpoint)?;
                let resp = offchain_reader
                    .execute_graphql(interpolated.trim().to_owned(), show_usage)
                    .await?;

                let mut output = vec![];
                if show_headers {
                    output.push(format!("Headers: {:#?}", resp.http_headers.unwrap()));
                }
                if show_service_version {
                    output.push(format!(
                        "Service version: {}",
                        resp.service_version.unwrap()
                    ));
                }
                output.push(format!("Response: {}", resp.response_body));

                Ok(Some(output.join("\n")))
            }
            SuiSubcommand::RunJsonRpc(RunJsonRpcCommand {
                show_headers,
                cursors,
            }) => {
                let file = data.ok_or_else(|| anyhow::anyhow!("Missing JSON-RPC query"))?;
                let contents = std::fs::read_to_string(file.path())?;

                let offchain_reader = self
                    .offchain_reader
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Offchain reader not set"))?;

                let highest_checkpoint = self.executor.get_latest_checkpoint_sequence_number()?;
                offchain_reader
                    .wait_for_checkpoint_catchup(highest_checkpoint, Duration::from_secs(60))
                    .await;

                let interpolated =
                    self.interpolate_query(&contents, &cursors, highest_checkpoint)?;

                #[derive(Deserialize)]
                struct Query {
                    method: String,
                    params: Value,
                }

                let query: Query = serde_json::from_str(&interpolated)
                    .context("Failed to parse JSON-RPC query")?;

                let resp = offchain_reader
                    .execute_jsonrpc(query.method, query.params)
                    .await?;

                let mut output = String::new();

                if show_headers {
                    write!(
                        &mut output,
                        "Headers: {:#?}\n\n",
                        resp.http_headers.unwrap()
                    )
                    .unwrap();
                }

                write!(&mut output, "Response: {}", resp.response_body).unwrap();
                Ok(Some(output))
            }
            SuiSubcommand::ViewCheckpoint => {
                let latest_chk = self.executor.get_latest_checkpoint_sequence_number()?;
                let chk = self
                    .executor
                    .get_checkpoint_by_sequence_number(latest_chk)
                    .unwrap();
                Ok(Some(format!("{}", chk.data())))
            }
            SuiSubcommand::CreateCheckpoint(CreateCheckpointCommand { count }) => {
                for _ in 0..count.unwrap_or(1) {
                    self.executor.create_checkpoint().await?;
                }
                let latest_chk = self.executor.get_latest_checkpoint_sequence_number()?;
                Ok(Some(format!("Checkpoint created: {}", latest_chk)))
            }
            SuiSubcommand::AdvanceEpoch(AdvanceEpochCommand {
                count,
                create_random_state,
            }) => {
                for _ in 0..count.unwrap_or(1) {
                    self.executor.advance_epoch(create_random_state).await?;
                }
                let epoch = self.get_latest_epoch_id()?;
                Ok(Some(format!("Epoch advanced: {epoch}")))
            }
            SuiSubcommand::AdvanceClock(AdvanceClockCommand { duration_ns }) => {
                self.executor
                    .advance_clock(Duration::from_nanos(duration_ns))
                    .await?;
                Ok(None)
            }
            SuiSubcommand::SetRandomState(SetRandomStateCommand {
                randomness_round,
                random_bytes,
                randomness_initial_version,
            }) => {
                let random_bytes = Base64::decode(&random_bytes)
                    .map_err(|e| anyhow!("Failed to decode random bytes as Base64: {e}"))?;

                let latest_epoch = self.get_latest_epoch_id()?;
                let tx = VerifiedTransaction::new_randomness_state_update(
                    latest_epoch,
                    RandomnessRound(randomness_round),
                    random_bytes,
                    SequenceNumber::from_u64(randomness_initial_version),
                );

                self.execute_txn(tx.into()).await?;
                Ok(None)
            }
            SuiSubcommand::ViewObject(ViewObjectCommand { id: fake_id }) => {
                let obj = get_obj!(fake_id);
                Ok(Some(match &obj.data {
                    object::Data::Move(move_obj) => {
                        let layout = move_obj.get_layout(&&*self).unwrap();
                        let move_struct =
                            BoundedVisitor::deserialize_struct(move_obj.contents(), &layout)
                                .unwrap();

                        self.stabilize_str(format!(
                            "Owner: {}\nVersion: {}\nContents: {:#}",
                            &obj.owner,
                            obj.version().value(),
                            move_struct
                        ))
                    }
                    object::Data::Package(package) => {
                        let num_modules = package.serialized_module_map().len();
                        let modules = package
                            .serialized_module_map()
                            .keys()
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ");
                        assert!(!modules.is_empty());
                        if num_modules > 1 {
                            format!("{}::{{{}}}", fake_id, modules)
                        } else {
                            format!("{}::{}", fake_id, modules)
                        }
                    }
                }))
            }
            SuiSubcommand::TransferObject(TransferObjectCommand {
                id: fake_id,
                recipient,
                sender,
                gas_budget,
                gas_price,
            }) => {
                let mut builder = ProgrammableTransactionBuilder::new();
                let obj_arg = SuiValue::Object(fake_id, None).into_argument(&mut builder, self)?;
                let recipient = match self.accounts.get(&recipient) {
                    Some(test_account) => test_account.address,
                    None => panic!("Unbound account {}", recipient),
                };
                let gas_budget = gas_budget.unwrap_or(DEFAULT_GAS_BUDGET);
                let gas_price: u64 = gas_price.unwrap_or(self.gas_price);
                let transaction = self.sign_txn(sender, |sender, gas| {
                    let rec_arg = builder.pure(recipient).unwrap();
                    builder.command(sui_types::transaction::Command::TransferObjects(
                        vec![obj_arg],
                        rec_arg,
                    ));
                    let pt = builder.finish();
                    TransactionData::new_programmable(sender, vec![gas], pt, gas_budget, gas_price)
                });
                let summary = self.execute_txn(transaction).await?;
                let output = self.object_summary_output(&summary, /* summarize */ false);
                Ok(output)
            }
            SuiSubcommand::ConsensusCommitPrologue(ConsensusCommitPrologueCommand {
                timestamp_ms,
            }) => {
                let transaction = VerifiedTransaction::new_consensus_commit_prologue_v3(
                    0,
                    0,
                    timestamp_ms,
                    ConsensusCommitDigest::default(),
                    ConsensusDeterminedVersionAssignments::empty_for_testing(),
                );
                let summary = self.execute_txn(transaction.into()).await?;
                let output = self.object_summary_output(&summary, /* summarize */ false);
                Ok(output)
            }
            SuiSubcommand::ProgrammableTransaction(ProgrammableTransactionCommand {
                sender,
                sponsor,
                gas_budget,
                gas_price,
                gas_payment,
                dev_inspect,
                dry_run,
                inputs,
            }) => {
                if dev_inspect && self.is_simulator() {
                    bail!("Dev inspect is not supported on simulator mode");
                }

                if dry_run && dev_inspect {
                    bail!("Cannot set both dev-inspect and dry-run");
                }

                let inputs = self.compiled_state().resolve_args(inputs)?;
                let inputs: Vec<CallArg> = inputs
                    .into_iter()
                    .map(|arg| arg.into_call_arg(self))
                    .collect::<anyhow::Result<_>>()?;
                let file = data.ok_or_else(|| {
                    anyhow::anyhow!("Missing commands for programmable transaction")
                })?;
                let contents = std::fs::read_to_string(file.path())?;
                let commands = ParsedCommand::parse_vec(&contents)?;
                let staged = &self.staged_modules;
                let state = &self.compiled_state;
                let commands = commands
                    .into_iter()
                    .map(|c| {
                        c.into_command(
                            &|p| {
                                let modules = staged
                                    .get(&Symbol::from(p))?
                                    .modules
                                    .iter()
                                    .map(|m| {
                                        let mut buf = vec![];
                                        m.module
                                            .serialize_with_version(m.module.version, &mut buf)
                                            .unwrap();
                                        buf
                                    })
                                    .collect();
                                Some(modules)
                            },
                            &|s| Some(state.resolve_named_address(s)),
                        )
                    })
                    .collect::<anyhow::Result<Vec<Command>>>()?;

                let summary = if !dev_inspect && !dry_run {
                    let gas_budget = gas_budget.unwrap_or(DEFAULT_GAS_BUDGET);
                    let gas_price = gas_price.unwrap_or(self.gas_price);
                    let transaction = self.sign_sponsor_txn(
                        sender,
                        sponsor,
                        gas_payment,
                        |sender, sponsor, gas| {
                            TransactionData::new_programmable_allow_sponsor(
                                sender,
                                vec![gas],
                                ProgrammableTransaction { inputs, commands },
                                gas_budget,
                                gas_price,
                                sponsor,
                            )
                        },
                    );
                    self.execute_txn(transaction).await?
                } else if dry_run {
                    let gas_budget = gas_budget.unwrap_or(DEFAULT_GAS_BUDGET);
                    let gas_price = gas_price.unwrap_or(self.gas_price);
                    let sender = self.get_sender(sender);
                    let sponsor = sponsor.map_or(sender, |a| self.get_sender(Some(a)));

                    let payment = self.get_payment(sponsor, gas_payment);

                    let transaction = TransactionData::new_programmable(
                        sender.address,
                        vec![payment],
                        ProgrammableTransaction { inputs, commands },
                        gas_budget,
                        gas_price,
                    );
                    self.dry_run(transaction).await?
                } else {
                    assert!(
                        gas_budget.is_none(),
                        "Meaningless to set gas budget with dev-inspect"
                    );
                    let sender_address = self.get_sender(sender).address;
                    let transaction =
                        TransactionKind::ProgrammableTransaction(ProgrammableTransaction {
                            inputs,
                            commands,
                        });
                    self.dev_inspect(sender_address, transaction, gas_price)
                        .await?
                };
                let output = self.object_summary_output(&summary, /* summarize */ false);
                Ok(output)
            }
            SuiSubcommand::UpgradePackage(UpgradePackageCommand {
                package,
                upgrade_capability,
                dependencies,
                sender,
                gas_budget,
                syntax,
                policy,
                gas_price,
            }) => {
                let syntax = syntax.unwrap_or_else(|| self.default_syntax());
                // zero out the package name
                let zero =
                    NumericalAddress::new(AccountAddress::ZERO.into_bytes(), NumberFormat::Hex);
                let before_upgrade = {
                    // not binding `m` separately results in some strange async capture error
                    let m = &mut self.compiled_state.named_address_mapping;
                    let Some(before) = m.insert(package.clone(), zero) else {
                        panic!("Unbound package '{package}' for upgrade");
                    };
                    before
                };

                // Override address mappings for compilation when upgrading. Each dependency is set to its
                // original address (if that dependency had been upgraded previously). This ensures upgraded
                // dependencies are resolved during compilation--without this workaround, the compiler will
                // find multiple definitions of the same module). We persist the original package name and
                // addresses, which we restore before performing an upgrade transaction below.
                let mut original_package_addrs = vec![];
                for dep in dependencies.iter() {
                    let named_address_mapping = &mut self.compiled_state.named_address_mapping;
                    let dep = &Symbol::from(dep.as_str());
                    let Some(orig_package) = self.package_upgrade_mapping.get(dep) else {
                        continue;
                    };
                    let Some(orig_package_address) =
                        named_address_mapping.insert(orig_package.to_string(), zero)
                    else {
                        continue;
                    };
                    original_package_addrs.push((*orig_package, orig_package_address));
                    let dep_address = named_address_mapping
                        .insert(dep.to_string(), orig_package_address)
                        .unwrap_or_else(||
                            panic!("Internal error: expected dependency {dep} in map when overriding address.")
                        );
                    original_package_addrs.push((*dep, dep_address));
                }
                let gas_price = gas_price.unwrap_or(self.gas_price);

                let result = compile_any(
                    self,
                    "upgrade",
                    syntax,
                    name,
                    number,
                    start_line,
                    command_lines_stop,
                    stop_line,
                    data,
                    |adapter, modules| async {
                        // Restore the original package addresses for dependencies before performing the upgrade.
                        // This ensures package upgrades are properly linked at their correct addresses
                        // (previously, addresses referred to the dependency's original package for compilation).
                        for (name, addr) in original_package_addrs {
                            adapter
                                .compiled_state()
                                .named_address_mapping
                                .insert(name.to_string(), addr)
                                .unwrap_or_else(|| panic!("Internal error: expected dependency {name} in map when restoring address."));
                        }

                        let upgraded_name = modules.first().unwrap().named_address.unwrap();
                        let package = &Symbol::from(package.as_str());
                        let original_name = adapter
                            .package_upgrade_mapping
                            .get(package)
                            .unwrap_or(package);
                        // Persist the upgraded package name with its original package name, so that we can
                        // refer to the original package name when compiling (see above on overridden addresses).
                        adapter
                            .package_upgrade_mapping
                            .insert(upgraded_name, *original_name);

                        let output = adapter.upgrade_package(
                            before_upgrade,
                            &modules,
                            upgrade_capability,
                            dependencies,
                            sender,
                            gas_budget,
                            policy,
                            gas_price,
                        ).await?;
                        Ok((output, modules))
                    },
                )
                .await;
                // if the package name was not updated, reset it to the value before the upgrade
                let package_addr = self
                    .compiled_state
                    .named_address_mapping
                    .get(&package)
                    .unwrap();
                if package_addr == &zero {
                    self.compiled_state
                        .named_address_mapping
                        .insert(package, before_upgrade);
                }
                let (warnings_opt, output, data, modules) = result?;
                store_modules(self, syntax, data, modules);
                Ok(merge_output(warnings_opt, output))
            }
            SuiSubcommand::StagePackage(StagePackageCommand {
                syntax,
                dependencies,
            }) => {
                let syntax = syntax.unwrap_or_else(|| self.default_syntax());
                let (warnings_opt, output, data, modules) = compile_any(
                    self,
                    "upgrade",
                    syntax,
                    name,
                    number,
                    start_line,
                    command_lines_stop,
                    stop_line,
                    data,
                    |_adapter, modules| async { Ok((None, modules)) },
                )
                .await?;
                assert!(!modules.is_empty());
                let Some(package_name) = modules.first().unwrap().named_address else {
                    bail!("Staged modules must have a named address")
                };
                for m in &modules {
                    let Some(named_addr) = &m.named_address else {
                        bail!("Staged modules must have a named address")
                    };
                    if named_addr != &package_name {
                        bail!(
                            "Staged modules must have the same named address, \
                            {package_name} != {named_addr}"
                        );
                    }
                }
                let dependencies =
                    self.get_dependency_ids(dependencies, /* include_std */ true)?;
                let module_bytes = modules
                    .iter()
                    .map(|m| {
                        let mut buf = vec![];
                        m.module
                            .serialize_with_version(m.module.version, &mut buf)
                            .unwrap();
                        buf
                    })
                    .collect::<Vec<_>>();
                let digest = MovePackage::compute_digest_for_modules_and_deps(
                    module_bytes.iter(),
                    &dependencies,
                    /* hash_modules */ true,
                )
                .to_vec();
                let staged = StagedPackage {
                    file: data,
                    syntax,
                    modules,
                    digest,
                };
                let prev = self.staged_modules.insert(package_name, staged);
                if prev.is_some() {
                    panic!("Package '{package_name}' already staged")
                }
                Ok(merge_output(warnings_opt, output))
            }
            SuiSubcommand::SetAddress(SetAddressCommand { address, input }) => {
                let address_sym = &Symbol::from(address.as_str());
                let state = self.compiled_state();
                let input = input.into_concrete_value(&|s| Some(state.resolve_named_address(s)))?;
                let (value, package) = match input {
                    SuiValue::Object(fake_id, version) => {
                        let id = match self.fake_to_real_object_id(fake_id) {
                            Some(id) => id,
                            None => bail!("INVALID TEST. Unknown object, object({})", fake_id),
                        };
                        let obj = self.get_object(&id, version)?;
                        let package = obj.data.try_as_package().map(|package| {
                            package
                                .serialized_module_map()
                                .iter()
                                .map(|(_, published_module_bytes)| {
                                    let module = CompiledModule::deserialize_with_defaults(
                                        published_module_bytes,
                                    )
                                    .unwrap();
                                    MaybeNamedCompiledModule {
                                        named_address: Some(*address_sym),
                                        module,
                                        source_map: None,
                                    }
                                })
                                .collect()
                        });
                        let value: AccountAddress = id.into();
                        (value, package)
                    }
                    SuiValue::MoveValue(v) => {
                        let bytes = v.simple_serialize().unwrap();
                        let value: AccountAddress = bcs::from_bytes(&bytes)?;
                        (value, None)
                    }
                    SuiValue::Digest(_) => bail!("digest is not supported as an input"),
                    SuiValue::ObjVec(_) => bail!("obj vec is not supported as an input"),
                    SuiValue::Receiving(_, _) => bail!("receiving is not supported as an input"),
                    SuiValue::ImmShared(_, _) => {
                        bail!("read-only shared object is not supported as an input")
                    }
                };
                let value = NumericalAddress::new(value.into_bytes(), NumberFormat::Hex);
                self.compiled_state
                    .named_address_mapping
                    .insert(address, value);

                let res = package.and_then(|p| Some((p, self.staged_modules.remove(address_sym)?)));
                if let Some((package, staged)) = res {
                    let StagedPackage {
                        file,
                        syntax,
                        modules: _,
                        digest: _,
                    } = staged;
                    store_modules(self, syntax, file, package)
                }

                Ok(None)
            }
            SuiSubcommand::Bench(
                RunCommand {
                    signers,
                    args,
                    type_args,
                    gas_budget,
                    syntax,
                    name,
                },
                extra_args,
            ) => {
                let (raw_addr, module_name, name) = name.unwrap();

                assert!(
                    syntax.is_none(),
                    "syntax flag meaningless with function execution"
                );

                let addr = self.compiled_state().resolve_address(&raw_addr);
                let module_id = ModuleId::new(addr, module_name);
                let type_args = self.compiled_state().resolve_type_args(type_args)?;
                let args = self.compiled_state().resolve_args(args)?;

                let tx = self
                    .build_function_call_tx(
                        &module_id,
                        name.as_ident_str(),
                        type_args.clone(),
                        signers.clone(),
                        args.clone(),
                        gas_budget,
                        extra_args.clone(),
                    )
                    .unwrap();

                let objects = self.executor.read_input_objects(tx.clone()).await?;

                // only run benchmarks in release mode
                if !cfg!(debug_assertions) {
                    let mut c = Criterion::default();

                    c.bench_function("benchmark_tx", |b| {
                        let tx = tx.clone();
                        let objects = objects.clone();
                        b.iter(|| {
                            self.executor
                                .prepare_txn(tx.clone(), objects.clone())
                                .unwrap();
                        })
                    });
                }

                // Run the tx for real after the benchmark, so that its effects are persisted and
                // available to subsequent commands
                self.call_function(
                    &module_id,
                    name.as_ident_str(),
                    type_args,
                    signers,
                    args,
                    gas_budget,
                    extra_args,
                )
                .await?;
                Ok(merge_output(None, None))
            }
        }
    }

    /// Process the error string such that it's less dependent on specific addresses or object IDs. Instead, they are
    /// replaced by the account names or fake IDs as much as possible. This reduces the effort of updating tests
    /// when something changed.
    async fn process_error(&self, error: anyhow::Error) -> anyhow::Error {
        let mut err = error.to_string();
        for (name, account) in &self.accounts {
            let addr = account.address.to_string();
            let replace = format!("@{}", name);
            err = err.replace(&addr, &replace);
            // Also match without 0x since different error messages may use different format.
            err = err.replace(&addr[2..], &replace);
        }
        for (id, fake_id) in &self.object_enumeration {
            let id = id.to_string();
            let replace = format!("object({})", fake_id);
            err = err.replace(&id, &replace);
            // Also match without 0x since different error messages may use different format.
            err = err.replace(&id[2..], &replace);
        }
        anyhow!(err)
    }
}

fn merge_output(left: Option<String>, right: Option<String>) -> Option<String> {
    match (left, right) {
        (None, right) => right,
        (left, None) => left,
        (Some(mut left), Some(right)) => {
            left.push_str(&right);
            Some(left)
        }
    }
}

impl SuiTestAdapter {
    pub fn with_offchain_reader(&mut self, offchain_reader: Box<dyn OffchainStateReader>) {
        self.offchain_reader = Some(offchain_reader);
    }

    pub fn is_simulator(&self) -> bool {
        self.is_simulator
    }

    pub fn executor(&self) -> &dyn TransactionalAdapter {
        &*self.executor
    }

    pub fn into_executor(self) -> Box<dyn TransactionalAdapter> {
        self.executor
    }

    fn named_variables(&self) -> BTreeMap<String, String> {
        let mut variables = BTreeMap::new();

        let named_addrs = self
            .compiled_state
            .named_address_mapping
            .iter()
            .map(|(name, addr)| (name.clone(), format!("{:#02x}", addr)));

        for (name, addr) in named_addrs {
            let addr = addr.to_string();

            // Required variant
            variables.insert(name.to_owned(), addr.clone());
            // Optional variant
            let name = name.to_string() + "_opt";
            variables.insert(name.clone(), addr.clone());
        }

        for (oid, fid) in &self.object_enumeration {
            if let FakeID::Enumerated(x, y) = fid {
                variables.insert(format!("obj_{x}_{y}"), oid.to_string());
                variables.insert(format!("obj_{x}_{y}_opt"), oid.to_string());
            }
        }

        for (tid, digest) in &self.digest_enumeration {
            variables.insert(format!("digest_{tid}"), digest.to_string());
        }

        variables
    }

    fn interpolate_contents(
        &self,
        contents: &str,
        variables: &BTreeMap<String, String>,
    ) -> anyhow::Result<String> {
        let mut interpolated_contents = contents.to_string();

        let re = regex::Regex::new(r"@\{([^\}]+)\}").unwrap();

        let unique_vars = re
            .captures_iter(contents)
            .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
            .collect::<std::collections::HashSet<_>>();

        for var_name in unique_vars {
            let Some(value) = variables.get(&var_name) else {
                return Err(anyhow!(
                    "Unknown variable: {}\nAllowed variable mappings are {:#?}",
                    var_name,
                    variables
                ));
            };

            let pattern = format!("@{{{}}}", var_name);
            interpolated_contents = interpolated_contents.replace(&pattern, value);
        }

        Ok(interpolated_contents)
    }

    fn encode_cursor(&self, cursor: &str) -> anyhow::Result<String> {
        // Cursor format is either bcs(object_id,n1,n2,...) or a json value,
        // in which case we just return its base64 encoding.
        let Some(args) = cursor
            .strip_prefix("bcs(")
            .and_then(|c| c.strip_suffix(")"))
        else {
            return Ok(Base64::encode(cursor));
        };

        let mut parts = args.split(",");

        let id: ObjectID = parts
            .next()
            .context("bcs(...) cursors must have at least one argument")?
            .trim()
            .parse()?;

        let mut bytes = bcs::to_bytes(&id.to_vec())?;
        for part in parts {
            let n: u64 = part.trim().parse()?;
            bytes.extend(bcs::to_bytes(&n)?);
        }

        Ok(Base64::encode(bytes))
    }

    fn interpolate_query(
        &self,
        contents: &str,
        cursors: &[String],
        highest_checkpoint: u64,
    ) -> anyhow::Result<String> {
        // First collect all the variable mappings
        let mut variables = self.named_variables();
        variables.insert(
            "highest_checkpoint".to_string(),
            highest_checkpoint.to_string(),
        );

        // Then interpolate the cursors which may reference objects
        for (idx, s) in cursors.iter().enumerate() {
            let interpolated_cursor = self.interpolate_contents(s, &variables)?;
            let encoded_cursor = self.encode_cursor(&interpolated_cursor)?;

            // Add the encoded cursor to the variables map because they may get used in the query.
            variables.insert(format!("cursor_{idx}"), encoded_cursor);
        }
        self.interpolate_contents(contents, &variables)
    }

    async fn upgrade_package(
        &mut self,
        before_upgrade: NumericalAddress,
        modules: &[MaybeNamedCompiledModule],
        upgrade_capability: FakeID,
        dependencies: Vec<String>,
        sender: String,
        gas_budget: Option<u64>,
        policy: u8,
        gas_price: u64,
        // dry_run: bool,
    ) -> anyhow::Result<Option<String>> {
        let modules_bytes = modules
            .iter()
            .map(|m| {
                let mut module_bytes = vec![];
                m.module
                    .serialize_with_version(m.module.version, &mut module_bytes)?;
                Ok(module_bytes)
            })
            .collect::<anyhow::Result<Vec<Vec<u8>>>>()?;
        let gas_budget = gas_budget.unwrap_or(DEFAULT_GAS_BUDGET);

        let dependencies = self.get_dependency_ids(dependencies, /* include_std */ true)?;

        let mut builder = ProgrammableTransactionBuilder::new();

        // Argument::Input(0)
        SuiValue::Object(upgrade_capability, None).into_argument(&mut builder, self)?;
        let upgrade_arg = builder.pure(policy).unwrap();
        let digest: Vec<u8> = MovePackage::compute_digest_for_modules_and_deps(
            &modules_bytes,
            &dependencies,
            /* hash_modules */ true,
        )
        .into();
        let digest_arg = builder.pure(digest).unwrap();

        let upgrade_ticket = builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            ident_str!("package").to_owned(),
            ident_str!("authorize_upgrade").to_owned(),
            vec![],
            vec![Argument::Input(0), upgrade_arg, digest_arg],
        );

        let package_id = before_upgrade.into_inner().into();
        let upgrade_receipt =
            builder.upgrade(package_id, upgrade_ticket, dependencies, modules_bytes);

        builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            ident_str!("package").to_owned(),
            ident_str!("commit_upgrade").to_owned(),
            vec![],
            vec![Argument::Input(0), upgrade_receipt],
        );

        let pt = builder.finish();

        let data = |sender, gas| {
            TransactionData::new_programmable(sender, vec![gas], pt, gas_budget, gas_price)
        };

        let transaction = self.sign_txn(Some(sender), data);
        let summary = self.execute_txn(transaction).await?;
        let created_package = summary
            .created
            .iter()
            .find_map(|id| {
                let object = self.get_object(id, None).unwrap();
                let package = object.data.try_as_package()?;
                Some(package.id())
            })
            .unwrap();
        let package_addr = NumericalAddress::new(created_package.into_bytes(), NumberFormat::Hex);
        if let Some(new_package_name) = modules[0].named_address {
            let prev_package = self
                .compiled_state
                .named_address_mapping
                .insert(new_package_name.to_string(), package_addr);
            match prev_package.map(|a| a.into_inner()) {
                Some(addr) if addr != AccountAddress::ZERO => panic!(
                    "Cannot reuse named address '{}' for multiple packages. \
                It should be set to 0 initially",
                    new_package_name
                ),
                _ => (),
            }
        }
        let output = self.object_summary_output(&summary, /* summarize */ false);
        Ok(output)
    }

    fn sign_txn(
        &self,
        sender: Option<String>,
        txn_data: impl FnOnce(/* sender */ SuiAddress, /* gas */ ObjectRef) -> TransactionData,
    ) -> Transaction {
        self.sign_sponsor_txn(sender, None, None, move |sender, _, gas| {
            txn_data(sender, gas)
        })
    }

    fn get_payment(&self, sponsor: &TestAccount, payment: Option<FakeID>) -> ObjectRef {
        let payment = if let Some(payment) = payment {
            self.fake_to_real_object_id(payment)
                .expect("Could not find specified payment object")
        } else {
            sponsor.gas
        };

        self.get_object(&payment, None)
            .unwrap()
            .compute_object_reference()
    }

    fn sign_sponsor_txn(
        &self,
        sender: Option<String>,
        sponsor: Option<String>,
        payment: Option<FakeID>,
        txn_data: impl FnOnce(
            /* sender */ SuiAddress,
            /* sponsor */ SuiAddress,
            /* gas */ ObjectRef,
        ) -> TransactionData,
    ) -> Transaction {
        let sender = self.get_sender(sender);
        let sponsor = sponsor.map_or(sender, |a| self.get_sender(Some(a)));

        let payment_ref = self.get_payment(sponsor, payment);

        let data = txn_data(sender.address, sponsor.address, payment_ref);
        if sender.address == sponsor.address {
            to_sender_signed_transaction(data, &sender.key_pair)
        } else {
            to_sender_signed_transaction_with_multi_signers(
                data,
                vec![&sender.key_pair, &sponsor.key_pair],
            )
        }
    }

    fn get_sender(&self, sender: Option<String>) -> &TestAccount {
        match sender {
            Some(n) => match self.accounts.get(&n) {
                Some(test_account) => test_account,
                None => panic!("Unbound account {}", n),
            },
            None => &self.default_account,
        }
    }

    fn build_function_call_tx(
        &mut self,
        module_id: &ModuleId,
        function: &IdentStr,
        type_args: Vec<TypeTag>,
        signers: Vec<ParsedAddress>,
        args: Vec<SuiValue>,
        gas_budget: Option<u64>,
        extra: SuiRunArgs,
    ) -> anyhow::Result<Transaction> {
        assert!(signers.is_empty(), "signers are not used");
        let SuiRunArgs {
            sender, gas_price, ..
        } = extra;
        let mut builder = ProgrammableTransactionBuilder::new();
        let arguments = args
            .into_iter()
            .map(|arg| arg.into_argument(&mut builder, self))
            .collect::<anyhow::Result<_>>()?;
        let package_id = ObjectID::from(*module_id.address());

        let gas_budget = gas_budget.unwrap_or(DEFAULT_GAS_BUDGET);
        let gas_price = gas_price.unwrap_or(self.gas_price);
        let data = |sender, gas| {
            builder.command(Command::move_call(
                package_id,
                module_id.name().to_owned(),
                function.to_owned(),
                type_args,
                arguments,
            ));
            let pt = builder.finish();
            TransactionData::new_programmable(sender, vec![gas], pt, gas_budget, gas_price)
        };
        Ok(self.sign_txn(sender, data))
    }

    async fn execute_txn(&mut self, transaction: Transaction) -> anyhow::Result<TxnSummary> {
        let with_shared = transaction
            .data()
            .intent_message()
            .value
            .contains_shared_object();
        let (effects, error_opt) = self.executor.execute_txn(transaction).await?;
        let digest = effects.transaction_digest();

        // Try to assign `digest_$task` to this transaction's digest -- panic if a transaction has
        // already been set. Currently each task executes at most one transaction, and everything
        // is fine. This panic triggering will be an early warning that we need to do something
        // more sophisticated.
        let task = self.next_fake.0;
        if let Some(prev) = self.digest_enumeration.insert(task, *digest) {
            panic!(
                "Task {task} executed two transactions (expected at most one): {prev}, {digest}"
            );
        }

        let mut created_ids: Vec<_> = effects
            .created()
            .iter()
            .map(|((id, _, _), _)| *id)
            .collect();
        let mut mutated_ids: Vec<_> = effects
            .mutated()
            .iter()
            .map(|((id, _, _), _)| *id)
            .collect();
        let mut unwrapped_ids: Vec<_> = effects
            .unwrapped()
            .iter()
            .map(|((id, _, _), _)| *id)
            .collect();
        let mut deleted_ids: Vec<_> = effects.deleted().iter().map(|(id, _, _)| *id).collect();
        let mut unwrapped_then_deleted_ids: Vec<_> = effects
            .unwrapped_then_deleted()
            .iter()
            .map(|(id, _, _)| *id)
            .collect();
        let mut wrapped_ids: Vec<_> = effects.wrapped().iter().map(|(id, _, _)| *id).collect();
        let gas_summary = effects.gas_cost_summary();

        // make sure objects that have previously not been in storage get assigned a fake id.
        let mut might_need_fake_id: Vec<_> = created_ids
            .iter()
            .chain(unwrapped_ids.iter())
            .copied()
            .collect();

        // Use a stable sort before assigning fake ids, so test output remains stable.
        might_need_fake_id.sort_by_key(|id| self.get_object_sorting_key(id));
        for id in might_need_fake_id {
            self.enumerate_fake(id);
        }

        let mut unchanged_shared_ids = effects
            .unchanged_shared_objects()
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();

        // Treat unwrapped objects as writes (even though sometimes this is the first time we can
        // refer to them at their id in storage).

        // sort by fake id
        created_ids.sort_by_key(|id| self.real_to_fake_object_id(id));
        mutated_ids.sort_by_key(|id| self.real_to_fake_object_id(id));
        unwrapped_ids.sort_by_key(|id| self.real_to_fake_object_id(id));
        deleted_ids.sort_by_key(|id| self.real_to_fake_object_id(id));
        unwrapped_then_deleted_ids.sort_by_key(|id| self.real_to_fake_object_id(id));
        wrapped_ids.sort_by_key(|id| self.real_to_fake_object_id(id));
        unchanged_shared_ids.sort_by_key(|id| self.real_to_fake_object_id(id));

        match effects.status() {
            ExecutionStatus::Success { .. } => {
                let events = self
                    .executor
                    .query_tx_events_asc(digest, *QUERY_MAX_RESULT_LIMIT)
                    .await?;
                Ok(TxnSummary {
                    events,
                    gas_summary: gas_summary.clone(),
                    created: created_ids,
                    mutated: mutated_ids,
                    unwrapped: unwrapped_ids,
                    deleted: deleted_ids,
                    unwrapped_then_deleted: unwrapped_then_deleted_ids,
                    wrapped: wrapped_ids,
                    unchanged_shared: unchanged_shared_ids,
                })
            }
            ExecutionStatus::Failure { error, command } => {
                let execution_msg = if with_shared {
                    format!("Debug of error: {error:?} at command {command:?}")
                } else {
                    format!("Execution Error: {}", error_opt.unwrap())
                };
                Err(anyhow::anyhow!(self.stabilize_str(format!(
                    "Transaction Effects Status: {error}\n{execution_msg}",
                ))))
            }
        }
    }

    async fn dry_run(&mut self, transaction: TransactionData) -> anyhow::Result<TxnSummary> {
        let digest = transaction.digest();
        let results = self
            .executor
            .dry_run_transaction_block(transaction, digest)
            .await?;
        let DryRunTransactionBlockResponse {
            effects, events, ..
        } = results;

        self.tx_summary_from_effects(effects, events)
    }

    async fn dev_inspect(
        &mut self,
        sender: SuiAddress,
        transaction_kind: TransactionKind,
        gas_price: Option<u64>,
    ) -> anyhow::Result<TxnSummary> {
        let results = self
            .executor
            .dev_inspect_transaction_block(sender, transaction_kind, gas_price)
            .await?;
        let DevInspectResults {
            effects, events, ..
        } = results;

        self.tx_summary_from_effects(effects, events)
    }

    fn tx_summary_from_effects(
        &mut self,
        effects: SuiTransactionBlockEffects,
        events: SuiTransactionBlockEvents,
    ) -> anyhow::Result<TxnSummary> {
        if let SuiExecutionStatus::Failure { error } = effects.status() {
            return Err(anyhow::anyhow!(self.stabilize_str(format!(
                "Transaction Effects Status: {error}\nExecution Error: {error}",
            ))));
        }

        let mut created_ids: Vec<_> = effects.created().iter().map(|o| o.object_id()).collect();
        let mut mutated_ids: Vec<_> = effects.mutated().iter().map(|o| o.object_id()).collect();
        let mut unwrapped_ids: Vec<_> = effects.unwrapped().iter().map(|o| o.object_id()).collect();
        let mut deleted_ids: Vec<_> = effects.deleted().iter().map(|o| o.object_id).collect();
        let mut unwrapped_then_deleted_ids: Vec<_> = effects
            .unwrapped_then_deleted()
            .iter()
            .map(|o| o.object_id)
            .collect();
        let mut wrapped_ids: Vec<_> = effects.wrapped().iter().map(|o| o.object_id).collect();
        let gas_summary = effects.gas_cost_summary();

        // make sure objects that have previously not been in storage get assigned a fake id.
        let mut might_need_fake_id: Vec<_> = created_ids
            .iter()
            .chain(unwrapped_ids.iter())
            .copied()
            .collect();

        // Use a stable sort before assigning fake ids, so test output remains stable.
        might_need_fake_id.sort_by_key(|id| self.get_object_sorting_key(id));
        for id in might_need_fake_id {
            self.enumerate_fake(id);
        }

        // Treat unwrapped objects as writes (even though sometimes this is the first time we can
        // refer to them at their id in storage).

        // sort by fake id
        created_ids.sort_by_key(|id| self.real_to_fake_object_id(id));
        mutated_ids.sort_by_key(|id| self.real_to_fake_object_id(id));
        unwrapped_ids.sort_by_key(|id| self.real_to_fake_object_id(id));
        deleted_ids.sort_by_key(|id| self.real_to_fake_object_id(id));
        unwrapped_then_deleted_ids.sort_by_key(|id| self.real_to_fake_object_id(id));
        wrapped_ids.sort_by_key(|id| self.real_to_fake_object_id(id));

        let events = events
            .data
            .into_iter()
            .map(|sui_event| sui_event.into())
            .collect();

        Ok(TxnSummary {
            events,
            gas_summary: gas_summary.clone(),
            created: created_ids,
            mutated: mutated_ids,
            unwrapped: unwrapped_ids,
            deleted: deleted_ids,
            unwrapped_then_deleted: unwrapped_then_deleted_ids,
            wrapped: wrapped_ids,
            // TODO: Properly propagate unchanged shared objects in dev_inspect.
            unchanged_shared: vec![],
        })
    }

    fn get_object(&self, id: &ObjectID, version: Option<SequenceNumber>) -> anyhow::Result<Object> {
        let obj_res = if let Some(v) = version {
            ObjectStore::get_object_by_key(&*self.executor, id, v)
        } else {
            ObjectStore::get_object(&*self.executor, id)
        };
        match obj_res {
            Some(obj) => Ok(obj),
            None => Err(anyhow!("INVALID TEST! Unable to find object {id}")),
        }
    }

    // stable way of sorting objects by type. Does not however, produce a stable sorting
    // between objects of the same type
    fn get_object_sorting_key(&self, id: &ObjectID) -> String {
        match &self.get_object(id, None).unwrap().data {
            object::Data::Move(obj) => self.stabilize_str(format!("{}", obj.type_())),
            object::Data::Package(pkg) => pkg
                .serialized_module_map()
                .keys()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(","),
        }
    }

    pub(crate) fn fake_to_real_object_id(&self, fake_id: FakeID) -> Option<ObjectID> {
        self.object_enumeration.get_by_right(&fake_id).copied()
    }

    pub(crate) fn real_to_fake_object_id(&self, id: &ObjectID) -> Option<FakeID> {
        self.object_enumeration.get_by_left(id).copied()
    }

    fn enumerate_fake(&mut self, id: ObjectID) -> FakeID {
        if let Some(fake) = self.object_enumeration.get_by_left(&id) {
            return *fake;
        }
        let (task, i) = self.next_fake;
        let fake_id = FakeID::Enumerated(task, i);
        self.object_enumeration.insert(id, fake_id);

        self.next_fake = (task, i + 1);
        fake_id
    }

    fn object_summary_output(
        &self,
        TxnSummary {
            events,
            gas_summary,
            created,
            mutated,
            unwrapped,
            deleted,
            unwrapped_then_deleted,
            wrapped,
            unchanged_shared,
        }: &TxnSummary,
        summarize: bool,
    ) -> Option<String> {
        let mut out = String::new();
        if !events.is_empty() {
            write!(out, "events: {}", self.list_events(events, summarize)).unwrap();
        }
        if !created.is_empty() {
            if !out.is_empty() {
                out.push('\n')
            }
            write!(out, "created: {}", self.list_objs(created, summarize)).unwrap();
        }
        if !mutated.is_empty() {
            if !out.is_empty() {
                out.push('\n')
            }
            write!(out, "mutated: {}", self.list_objs(mutated, summarize)).unwrap();
        }
        if !unwrapped.is_empty() {
            if !out.is_empty() {
                out.push('\n')
            }
            write!(out, "unwrapped: {}", self.list_objs(unwrapped, summarize)).unwrap();
        }
        if !deleted.is_empty() {
            if !out.is_empty() {
                out.push('\n')
            }
            write!(out, "deleted: {}", self.list_objs(deleted, summarize)).unwrap();
        }
        if !unwrapped_then_deleted.is_empty() {
            if !out.is_empty() {
                out.push('\n')
            }
            write!(
                out,
                "unwrapped_then_deleted: {}",
                self.list_objs(unwrapped_then_deleted, summarize)
            )
            .unwrap();
        }
        if !wrapped.is_empty() {
            if !out.is_empty() {
                out.push('\n')
            }
            write!(out, "wrapped: {}", self.list_objs(wrapped, summarize)).unwrap();
        }
        if !unchanged_shared.is_empty() {
            if !out.is_empty() {
                out.push('\n')
            }
            write!(
                out,
                "unchanged_shared: {}",
                self.list_objs(unchanged_shared, summarize)
            )
            .unwrap();
        }
        out.push('\n');
        write!(out, "gas summary: {}", gas_summary).unwrap();

        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    }

    fn list_events(&self, events: &[Event], summarize: bool) -> String {
        if summarize {
            return format!("{}", events.len());
        }
        events
            .iter()
            .map(|event| self.stabilize_str(format!("{:?}", event)))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn list_objs(&self, objs: &[ObjectID], summarize: bool) -> String {
        if summarize {
            return format!("{}", objs.len());
        }
        objs.iter()
            .map(
                |id| /*id.to_string(), */match self.real_to_fake_object_id(id) {
                                         None => "object(_)".to_string(),
                                         Some(FakeID::Known(id)) => {
                                             let id: AccountAddress = id.into();
                                             format!("0x{id:x}")
                                         }
                                         Some(fake) => format!("object({})", fake),
                                     },
            )
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn stabilize_str(&self, input: impl AsRef<str>) -> String {
        fn candidate_is_hex(s: &str) -> bool {
            const HEX_STR_LENGTH: usize = SUI_ADDRESS_LENGTH * 2;
            let n = s.len();
            (s.starts_with("0x") && n >= 3) || n == HEX_STR_LENGTH
        }
        let mut hex_candidate = String::new();
        let mut result = String::new();
        let mut chars = input.as_ref().chars().peekable();
        let mut cur = chars.next();
        while let Some(c) = cur {
            match c {
                '0' if hex_candidate.is_empty() && matches!(chars.peek(), Some('x')) => {
                    let c = chars.next().unwrap();
                    assert!(c == 'x');
                    hex_candidate.push_str("0x");
                }
                '0'..='9' | 'a'..='f' | 'A'..='F' => hex_candidate.push(c),
                _ => {
                    if candidate_is_hex(&hex_candidate) {
                        result.push_str(&self.remap_hex_str(hex_candidate));
                        hex_candidate = String::new();
                    } else {
                        result.push_str(&hex_candidate);
                        if !hex_candidate.is_empty() {
                            hex_candidate = String::new();
                        }
                    }
                    result.push(c);
                }
            }
            cur = chars.next();
        }
        if candidate_is_hex(&hex_candidate) {
            result.push_str(&self.remap_hex_str(hex_candidate));
        } else {
            result.push_str(&hex_candidate);
        }
        result
    }

    fn remap_hex_str(&self, hex_str: String) -> String {
        let hex_str = if hex_str.starts_with("0x") {
            hex_str
        } else {
            format!("0x{}", hex_str)
        };
        let parsed = AccountAddress::from_hex_literal(&hex_str).unwrap();
        if let Some((known, _)) = self
            .compiled_state
            .named_address_mapping
            .iter()
            .find(|(_name, addr)| addr.into_inner() == parsed)
        {
            return known.clone();
        }
        match self.real_to_fake_object_id(&parsed.into()) {
            None => "_".to_string(),
            Some(FakeID::Known(id)) => {
                let id: AccountAddress = id.into();
                format!("0x{id:x}")
            }
            Some(fake) => format!("fake({})", fake),
        }
    }

    fn next_task(&mut self) {
        self.next_fake = (self.next_fake.0 + 1, 0)
    }

    fn get_dependency_ids(
        &self,
        dependencies: Vec<String>,
        include_std: bool,
    ) -> anyhow::Result<Vec<ObjectID>> {
        let mut dependencies: Vec<_> = dependencies
            .into_iter()
            .map(|d| {
                let Some(addr) = self.compiled_state.named_address_mapping.get(&d) else {
                    bail!("There is no published module address corresponding to name address {d}");
                };
                let id: ObjectID = addr.into_inner().into();
                Ok(id)
            })
            .collect::<Result<_, _>>()?;
        // we are assuming that all packages depend on Move Stdlib and Sui Framework, so these
        // don't have to be provided explicitly as parameters
        if include_std {
            dependencies.extend([MOVE_STDLIB_PACKAGE_ID, SUI_FRAMEWORK_PACKAGE_ID]);
        }
        Ok(dependencies)
    }
}

impl<'a> GetModule for &'a SuiTestAdapter {
    type Error = anyhow::Error;

    type Item = &'a CompiledModule;

    fn get_module_by_id(&self, id: &ModuleId) -> anyhow::Result<Option<Self::Item>, Self::Error> {
        Ok(Some(
            self.compiled_state
                .dep_modules()
                .find(|m| &m.self_id() == id)
                .unwrap_or_else(|| panic!("Internal error: Unbound module {}", id)),
        ))
    }
}

impl fmt::Display for FakeID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FakeID::Known(id) => {
                let addr: AccountAddress = (*id).into();
                write!(f, "0x{:x}", addr)
            }
            FakeID::Enumerated(task, i) => write!(f, "{},{}", task, i),
        }
    }
}

impl Default for AdapterInitConfig {
    fn default() -> Self {
        Self {
            additional_mapping: BTreeMap::new(),
            account_names: BTreeSet::new(),
            protocol_config: ProtocolConfig::get_for_max_version_UNSAFE(),
            is_simulator: false,
            custom_validator_account: false,
            reference_gas_price: None,
            default_gas_price: None,
            flavor: None,
            offchain_config: None,
        }
    }
}

static NAMED_ADDRESSES: Lazy<BTreeMap<String, NumericalAddress>> = Lazy::new(|| {
    let mut map = move_stdlib::move_stdlib_named_addresses();
    assert!(map.get("std").unwrap().into_inner() == MOVE_STDLIB_ADDRESS);
    // TODO fix Sui framework constants
    map.insert(
        "sui".to_string(),
        NumericalAddress::new(
            SUI_FRAMEWORK_ADDRESS.into_bytes(),
            move_compiler::shared::NumberFormat::Hex,
        ),
    );
    map.insert(
        "sui_system".to_string(),
        NumericalAddress::new(
            SUI_SYSTEM_ADDRESS.into_bytes(),
            move_compiler::shared::NumberFormat::Hex,
        ),
    );
    map.insert(
        "deepbook".to_string(),
        NumericalAddress::new(
            DEEPBOOK_ADDRESS.into_bytes(),
            move_compiler::shared::NumberFormat::Hex,
        ),
    );
    map.insert(
        "bridge".to_string(),
        NumericalAddress::new(
            BRIDGE_ADDRESS.into_bytes(),
            move_compiler::shared::NumberFormat::Hex,
        ),
    );
    map
});

pub static PRE_COMPILED: Lazy<FullyCompiledProgram> = Lazy::new(|| {
    // TODO invoke package system? Or otherwise pull the versions for these packages as per their
    // actual Move.toml files. They way they are treated here is odd, too, though.
    let sui_files: &Path = Path::new(DEFAULT_FRAMEWORK_PATH);
    let sui_system_sources = {
        let mut buf = sui_files.to_path_buf();
        buf.extend(["packages", "sui-system", "sources"]);
        buf.to_string_lossy().to_string()
    };
    let sui_sources = {
        let mut buf = sui_files.to_path_buf();
        buf.extend(["packages", "sui-framework", "sources"]);
        buf.to_string_lossy().to_string()
    };
    let sui_deps = {
        let mut buf = sui_files.to_path_buf();
        buf.extend(["packages", "move-stdlib", "sources"]);
        buf.to_string_lossy().to_string()
    };
    let deepbook_sources = {
        let mut buf = sui_files.to_path_buf();
        buf.extend(["packages", "deepbook", "sources"]);
        buf.to_string_lossy().to_string()
    };
    let config = PackageConfig {
        edition: Edition::E2024_BETA,
        flavor: Flavor::Sui,
        ..Default::default()
    };
    let bridge_sources = {
        let mut buf = sui_files.to_path_buf();
        buf.extend(["packages", "bridge", "sources"]);
        buf.to_string_lossy().to_string()
    };
    let fully_compiled_res = move_compiler::construct_pre_compiled_lib(
        vec![PackagePaths {
            name: Some(("sui-framework".into(), config)),
            paths: vec![
                sui_system_sources,
                sui_sources,
                sui_deps,
                deepbook_sources,
                bridge_sources,
            ],
            named_address_map: NAMED_ADDRESSES.clone(),
        }],
        None,
        Flags::empty(),
        None,
    )
    .unwrap();
    match fully_compiled_res {
        Err((files, diags)) => {
            eprintln!("!!!Sui framework failed to compile!!!");
            move_compiler::diagnostics::report_diagnostics(&files, diags)
        }
        Ok(res) => res,
    }
});

async fn create_validator_fullnode(
    protocol_config: &ProtocolConfig,
    objects: &[Object],
) -> (Arc<AuthorityState>, Arc<AuthorityState>) {
    let builder = TestAuthorityBuilder::new()
        .with_protocol_config(protocol_config.clone())
        .with_starting_objects(objects);
    let state = builder.clone().build().await;
    let fullnode_key_pair = get_authority_key_pair().1;
    let fullnode = builder.with_keypair(&fullnode_key_pair).build().await;
    (state, fullnode)
}

async fn create_val_fullnode_executor(
    protocol_config: &ProtocolConfig,
    objects: &[Object],
) -> ValidatorWithFullnode {
    let (validator, fullnode) = create_validator_fullnode(protocol_config, objects).await;

    let metrics = KeyValueStoreMetrics::new_for_tests();
    let kv_store = Arc::new(TransactionKeyValueStore::new(
        "rocksdb",
        metrics,
        validator.clone(),
    ));
    ValidatorWithFullnode {
        validator,
        fullnode,
        kv_store,
    }
}

struct AccountSetup {
    pub default_account: TestAccount,
    pub named_address_mapping: BTreeMap<String, NumericalAddress>,
    pub objects: Vec<Object>,
    pub account_objects: BTreeMap<String, ObjectID>,
    pub accounts: BTreeMap<String, TestAccount>,
}

/// Create the executor for a validator with a fullnode
/// The issue with this executor is we cannot control the checkpoint
/// and epoch creation process
async fn init_val_fullnode_executor(
    mut rng: StdRng,
    account_names: BTreeSet<String>,
    additional_mapping: BTreeMap<String, NumericalAddress>,
    protocol_config: &ProtocolConfig,
) -> (
    Box<dyn TransactionalAdapter>,
    AccountSetup,
    Option<Arc<dyn RpcStateReader + Send + Sync>>,
) {
    // Initial list of named addresses with specified values
    let mut named_address_mapping = NAMED_ADDRESSES.clone();
    let mut account_objects = BTreeMap::new();
    let mut accounts = BTreeMap::new();
    let mut objects = vec![];

    // Closure to create accounts with gas objects of value `GAS_FOR_TESTING`
    let mut mk_account = || {
        let (address, key_pair) = get_key_pair_from_rng(&mut rng);
        let obj = Object::with_id_owner_gas_for_testing(
            ObjectID::new(rng.gen()),
            address,
            GAS_FOR_TESTING,
        );
        let test_account = TestAccount {
            address,
            key_pair,
            gas: obj.id(),
        };
        objects.push(obj);
        test_account
    };

    // For each named Sui account without an address value, create an account with an address
    // and a gas object
    for n in account_names {
        let test_account = mk_account();
        account_objects.insert(n.clone(), test_account.gas);
        accounts.insert(n, test_account);
    }

    // Make a default account with a gas object
    let default_account = mk_account();

    let executor = Box::new(create_val_fullnode_executor(protocol_config, &objects).await);

    update_named_address_mapping(
        &mut named_address_mapping,
        &accounts,
        additional_mapping,
        &*executor,
    )
    .await;

    let acc_setup = AccountSetup {
        default_account,
        named_address_mapping,
        objects,
        account_objects,
        accounts,
    };
    (executor, acc_setup, None)
}

/// Create an executor using a simulator
/// This means we can control the checkpoint, epoch creation process and
/// manually advance clock as needed
async fn init_sim_executor(
    mut rng: StdRng,
    account_names: BTreeSet<String>,
    additional_mapping: BTreeMap<String, NumericalAddress>,
    protocol_config: &ProtocolConfig,
    custom_validator_account: bool,
    reference_gas_price: Option<u64>,
    data_ingestion_path: PathBuf,
) -> (
    Box<dyn TransactionalAdapter>,
    AccountSetup,
    Option<Arc<dyn RpcStateReader + Send + Sync>>,
) {
    // Initial list of named addresses with specified values
    let mut named_address_mapping = NAMED_ADDRESSES.clone();
    let mut account_objects = BTreeMap::new();
    let mut account_kps = BTreeMap::new();
    let mut accounts = BTreeMap::new();
    let mut objects = vec![];

    // For each named Sui account without an address value, create a key pair
    for n in account_names {
        let test_account = get_key_pair_from_rng(&mut rng);
        account_kps.insert(n, test_account);
    }

    // Make a default account keypair
    let default_account_kp = get_key_pair_from_rng(&mut rng);

    let (mut validator_addr, mut validator_key, mut key_copy) = (None, None, None);
    if custom_validator_account {
        // Make a validator account with a gas object
        let (a, b): (SuiAddress, Ed25519KeyPair) = get_key_pair_from_rng(&mut rng);

        key_copy = Some(
            Ed25519KeyPair::from_bytes(b.as_bytes())
                .expect("FATAL: recovering key from bytes failed"),
        );
        validator_addr = Some(a);
        validator_key = Some(b);
    }

    let mut acc_cfgs = account_kps
        .values()
        .map(|acc| AccountConfig {
            address: Some(acc.0),
            gas_amounts: vec![GAS_FOR_TESTING],
        })
        .collect::<Vec<_>>();
    acc_cfgs.push(AccountConfig {
        address: Some(default_account_kp.0),
        gas_amounts: vec![GAS_FOR_TESTING],
    });

    if let Some(v_addr) = validator_addr {
        acc_cfgs.push(AccountConfig {
            address: Some(v_addr),
            gas_amounts: vec![GAS_FOR_TESTING],
        });
    }

    // Create the simulator with the specific account configs, which also crates objects

    let (mut sim, read_replica) =
        PersistedStore::new_sim_replica_with_protocol_version_and_accounts(
            rng,
            DEFAULT_CHAIN_START_TIMESTAMP,
            protocol_config.version,
            acc_cfgs,
            key_copy.map(|q| vec![q]),
            reference_gas_price,
            None,
        );

    sim.set_data_ingestion_path(data_ingestion_path.clone());

    // Get the actual object values from the simulator
    for (name, (addr, kp)) in account_kps {
        let o = sim.store().owned_objects(addr).next().unwrap();
        objects.push(o.clone());
        account_objects.insert(name.clone(), o.id());

        accounts.insert(
            name.to_owned(),
            TestAccount {
                address: addr,
                key_pair: kp,
                gas: o.id(),
            },
        );
    }
    let o = sim
        .store()
        .owned_objects(default_account_kp.0)
        .next()
        .unwrap();
    let default_account = TestAccount {
        address: default_account_kp.0,
        key_pair: default_account_kp.1,
        gas: o.id(),
    };
    objects.push(o.clone());

    if let (Some(v_addr), Some(v_key)) = (validator_addr, validator_key) {
        let o = sim.store().owned_objects(v_addr).next().unwrap();
        let validator_account = TestAccount {
            address: v_addr,
            key_pair: v_key,
            gas: o.id(),
        };
        objects.push(o.clone());
        account_objects.insert("validator_0".to_string(), o.id());
        accounts.insert("validator_0".to_string(), validator_account);
    }

    let sim = Box::new(sim);
    update_named_address_mapping(
        &mut named_address_mapping,
        &accounts,
        additional_mapping,
        &*sim,
    )
    .await;

    (
        sim,
        AccountSetup {
            default_account,
            named_address_mapping,
            objects,
            account_objects,
            accounts,
        },
        Some(Arc::new(read_replica)),
    )
}

async fn update_named_address_mapping(
    named_address_mapping: &mut BTreeMap<String, NumericalAddress>,
    accounts: &BTreeMap<String, TestAccount>,
    additional_mapping: BTreeMap<String, NumericalAddress>,
    trans_adapter: &dyn TransactionalAdapter,
) {
    let active_val_addrs: BTreeMap<_, _> = trans_adapter
        .get_active_validator_addresses()
        .await
        .expect("Failed to get validator addresses")
        .iter()
        .enumerate()
        .map(|(idx, addr)| (format!("validator_{idx}"), *addr))
        .collect();

    // For mappings where the address is specified, populate the named address mapping
    let additional_mapping = additional_mapping
        .into_iter()
        .chain(accounts.iter().map(|(n, test_account)| {
            let addr = NumericalAddress::new(test_account.address.to_inner(), NumberFormat::Hex);
            (n.clone(), addr)
        }))
        .chain(active_val_addrs.iter().map(|(n, addr)| {
            let addr = NumericalAddress::new(addr.to_inner(), NumberFormat::Hex);
            (n.clone(), addr)
        }));
    // Extend the mappings of all named addresses with values
    for (name, addr) in additional_mapping {
        if (named_address_mapping.contains_key(&name)
            && (named_address_mapping.get(&name) != Some(&addr)))
            || name == "sui"
        {
            panic!(
                "Invalid init. The named address '{}' is reserved or duplicated",
                name
            )
        }
        named_address_mapping.insert(name, addr);
    }
}

impl ObjectStore for SuiTestAdapter {
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        ObjectStore::get_object(&*self.executor, object_id)
    }

    fn get_object_by_key(&self, object_id: &ObjectID, version: VersionNumber) -> Option<Object> {
        ObjectStore::get_object_by_key(&*self.executor, object_id, version)
    }
}

impl ReadStore for SuiTestAdapter {
    fn get_latest_epoch_id(&self) -> sui_types::storage::error::Result<EpochId> {
        self.executor.get_latest_epoch_id()
    }

    fn get_committee(
        &self,
        epoch: sui_types::committee::EpochId,
    ) -> Option<Arc<sui_types::committee::Committee>> {
        self.executor.get_committee(epoch)
    }

    fn get_latest_checkpoint(&self) -> sui_types::storage::error::Result<VerifiedCheckpoint> {
        ReadStore::get_latest_checkpoint(&self.executor)
    }

    fn get_highest_verified_checkpoint(
        &self,
    ) -> sui_types::storage::error::Result<VerifiedCheckpoint> {
        self.executor.get_highest_verified_checkpoint()
    }

    fn get_highest_synced_checkpoint(
        &self,
    ) -> sui_types::storage::error::Result<VerifiedCheckpoint> {
        self.executor.get_highest_synced_checkpoint()
    }

    fn get_lowest_available_checkpoint(
        &self,
    ) -> sui_types::storage::error::Result<CheckpointSequenceNumber> {
        self.executor.get_lowest_available_checkpoint()
    }

    fn get_checkpoint_by_digest(
        &self,
        digest: &sui_types::messages_checkpoint::CheckpointDigest,
    ) -> Option<VerifiedCheckpoint> {
        self.executor.get_checkpoint_by_digest(digest)
    }

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        self.executor
            .get_checkpoint_by_sequence_number(sequence_number)
    }

    fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        self.executor.get_checkpoint_contents_by_digest(digest)
    }

    fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<CheckpointContents> {
        self.executor
            .get_checkpoint_contents_by_sequence_number(sequence_number)
    }

    fn get_transaction(&self, tx_digest: &TransactionDigest) -> Option<Arc<VerifiedTransaction>> {
        self.executor.get_transaction(tx_digest)
    }

    fn get_transaction_effects(&self, tx_digest: &TransactionDigest) -> Option<TransactionEffects> {
        self.executor.get_transaction_effects(tx_digest)
    }

    fn get_events(&self, event_digest: &TransactionEventsDigest) -> Option<TransactionEvents> {
        self.executor.get_events(event_digest)
    }

    fn get_full_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<sui_types::messages_checkpoint::FullCheckpointContents> {
        self.executor
            .get_full_checkpoint_contents_by_sequence_number(sequence_number)
    }

    fn get_full_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<sui_types::messages_checkpoint::FullCheckpointContents> {
        self.executor.get_full_checkpoint_contents(digest)
    }
}
