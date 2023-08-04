// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This module contains the transactional test runner instantiation for the Sui adapter

use crate::{args::*, programmable_transaction_test_parser::parser::ParsedCommand};
use anyhow::{anyhow, bail};
use async_trait::async_trait;
use bimap::btree::BiBTreeMap;
use fastcrypto::traits::KeyPair;
use move_binary_format::{file_format::CompiledScript, CompiledModule};
use move_bytecode_utils::module_cache::GetModule;
use move_command_line_common::{
    address::ParsedAddress, files::verify_and_create_named_address_mapping,
};
use move_compiler::{
    shared::{NumberFormat, NumericalAddress, PackagePaths},
    Flags, FullyCompiledProgram,
};
use move_core_types::ident_str;
use move_core_types::{
    account_address::AccountAddress,
    identifier::IdentStr,
    language_storage::{ModuleId, TypeTag},
    value::MoveStruct,
};
use move_symbol_pool::Symbol;
use move_transactional_test_runner::{
    framework::{compile_any, store_modules, CompiledState, MoveTestAdapter},
    tasks::{InitCommand, SyntaxChoice, TaskInput},
};
use move_vm_runtime::session::SerializedReturnValues;
use once_cell::sync::Lazy;
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::fmt::{self, Write};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
};
use sui_core::authority::{
    authority_test_utils::send_and_confirm_transaction_with_execution_error,
    test_authority_builder::TestAuthorityBuilder, AuthorityState,
};
use sui_framework::BuiltInFramework;
use sui_framework::DEFAULT_FRAMEWORK_PATH;
use sui_json_rpc::api::QUERY_MAX_RESULT_LIMIT;
use sui_json_rpc_types::{
    DevInspectResults, EventFilter, SuiExecutionStatus, SuiTransactionBlockEffectsAPI,
};
use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};
use sui_types::transaction::Command;
use sui_types::transaction::ProgrammableTransaction;
use sui_types::DEEPBOOK_PACKAGE_ID;
use sui_types::MOVE_STDLIB_PACKAGE_ID;
use sui_types::{base_types::SequenceNumber, effects::TransactionEffectsAPI};
use sui_types::{
    base_types::{ObjectID, ObjectRef, SuiAddress, TransactionDigest, SUI_ADDRESS_LENGTH},
    crypto::{get_key_pair_from_rng, AccountKeyPair},
    event::Event,
    object::{self, Object, ObjectFormatOptions},
    object::{MoveObject, Owner},
    transaction::{Transaction, TransactionData, TransactionDataAPI, VerifiedTransaction},
    MOVE_STDLIB_ADDRESS, SUI_CLOCK_OBJECT_ID, SUI_CLOCK_OBJECT_SHARED_VERSION,
    SUI_FRAMEWORK_ADDRESS, SUI_SYSTEM_STATE_OBJECT_ID,
};
use sui_types::{clock::Clock, SUI_SYSTEM_ADDRESS};
use sui_types::{crypto::get_authority_key_pair, storage::ObjectStore};
use sui_types::{execution_status::ExecutionStatus, transaction::TransactionKind};
use sui_types::{gas::GasCostSummary, object::GAS_VALUE_FOR_TESTING};
use sui_types::{id::UID, DEEPBOOK_ADDRESS};
use sui_types::{
    move_package::MovePackage,
    transaction::{Argument, CallArg},
};
use sui_types::{
    programmable_transaction_builder::ProgrammableTransactionBuilder, SUI_FRAMEWORK_PACKAGE_ID,
};
use sui_types::{utils::to_sender_signed_transaction, SUI_SYSTEM_PACKAGE_ID};
use tempfile::NamedTempFile;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum FakeID {
    Known(ObjectID),
    Enumerated(u64, u64),
}

const WELL_KNOWN_OBJECTS: &[ObjectID] = &[
    MOVE_STDLIB_PACKAGE_ID,
    DEEPBOOK_PACKAGE_ID,
    SUI_FRAMEWORK_PACKAGE_ID,
    SUI_SYSTEM_PACKAGE_ID,
    SUI_SYSTEM_STATE_OBJECT_ID,
    SUI_CLOCK_OBJECT_ID,
];
// TODO use the file name as a seed
const RNG_SEED: [u8; 32] = [
    21, 23, 199, 200, 234, 250, 252, 178, 94, 15, 202, 178, 62, 186, 88, 137, 233, 192, 130, 157,
    179, 179, 65, 9, 31, 249, 221, 123, 225, 112, 199, 247,
];

const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;
const GAS_FOR_TESTING: u64 = GAS_VALUE_FOR_TESTING;

pub struct SuiTestAdapter<'a> {
    pub(crate) validator: Arc<AuthorityState>,
    pub(crate) fullnode: Arc<AuthorityState>,
    pub(crate) compiled_state: CompiledState<'a>,
    /// For upgrades: maps an upgraded package name to the original package name.
    package_upgrade_mapping: BTreeMap<Symbol, Symbol>,
    accounts: BTreeMap<String, TestAccount>,
    default_account: TestAccount,
    default_syntax: SyntaxChoice,
    object_enumeration: BiBTreeMap<ObjectID, FakeID>,
    next_fake: (u64, u64),
    gas_price: u64,
    pub(crate) staged_modules: BTreeMap<Symbol, StagedPackage>,
}

pub(crate) struct StagedPackage {
    file: NamedTempFile,
    syntax: SyntaxChoice,
    modules: Vec<(Option<Symbol>, CompiledModule)>,
    pub(crate) digest: Vec<u8>,
}

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
    events: Vec<Event>,
    gas_summary: GasCostSummary,
}

static GENESIS: Lazy<Genesis> = Lazy::new(create_genesis_module_objects);
static GENESIS_PROTOCOL_CONSTANTS: Lazy<ProtocolConfig> =
    Lazy::new(ProtocolConfig::get_for_min_version);

struct Genesis {
    pub objects: Vec<Object>,
    pub packages: Vec<Object>,
    pub modules: Vec<Vec<CompiledModule>>,
}

pub fn clone_genesis_compiled_modules() -> Vec<Vec<CompiledModule>> {
    GENESIS.modules.clone()
}

pub fn clone_genesis_objects() -> Vec<Object> {
    GENESIS.objects.clone()
}

fn genesis_module_objects_for_protocol_version(protocol_version: ProtocolVersion) -> Vec<Object> {
    if protocol_version == ProtocolVersion::max() {
        return GENESIS.packages.clone();
    }
    sui_framework_snapshot::load_bytecode_snapshot(protocol_version.as_u64())
        .expect("Unable to load bytecode snapshot")
        .into_iter()
        .map(|pkg| pkg.genesis_object())
        .collect()
}

/// Create and return objects wrapping the genesis modules for sui
fn create_genesis_module_objects() -> Genesis {
    Genesis {
        objects: vec![create_clock()],
        packages: BuiltInFramework::genesis_objects().collect(),
        modules: BuiltInFramework::iter_system_packages()
            .map(|p| p.modules())
            .collect(),
    }
}

fn create_clock() -> Object {
    // SAFETY: unwrap safe because genesis objects should be serializable
    let contents = bcs::to_bytes(&Clock {
        id: UID::new(SUI_CLOCK_OBJECT_ID),
        timestamp_ms: 0,
    })
    .unwrap();

    // SAFETY: Whether `Clock` has public transfer or not is statically known, and unwrap safe
    // because genesis objects should never exceed max size
    let move_object = unsafe {
        let has_public_transfer = false;
        MoveObject::new_from_execution(
            Clock::type_().into(),
            has_public_transfer,
            SUI_CLOCK_OBJECT_SHARED_VERSION,
            contents,
            &GENESIS_PROTOCOL_CONSTANTS,
        )
        .unwrap()
    };

    Object::new_move(
        move_object,
        Owner::Shared {
            initial_shared_version: SUI_CLOCK_OBJECT_SHARED_VERSION,
        },
        TransactionDigest::genesis(),
    )
}

#[async_trait]
impl<'a> MoveTestAdapter<'a> for SuiTestAdapter<'a> {
    type ExtraPublishArgs = SuiPublishArgs;
    type ExtraRunArgs = SuiRunArgs;
    type Subcommand = SuiSubcommand;
    type ExtraInitArgs = SuiInitArgs;
    type ExtraValueArgs = SuiExtraValueArgs;

    fn compiled_state(&mut self) -> &mut CompiledState<'a> {
        &mut self.compiled_state
    }

    fn default_syntax(&self) -> SyntaxChoice {
        self.default_syntax
    }

    async fn init(
        default_syntax: SyntaxChoice,
        pre_compiled_deps: Option<&'a FullyCompiledProgram>,
        task_opt: Option<
            move_transactional_test_runner::tasks::TaskInput<(
                move_transactional_test_runner::tasks::InitCommand,
                Self::ExtraInitArgs,
            )>,
        >,
    ) -> (Self, Option<String>) {
        let mut rng = StdRng::from_seed(RNG_SEED);
        assert!(
            pre_compiled_deps.is_some(),
            "Must populate 'pre_compiled_deps' with Sui framework"
        );
        let (additional_mapping, account_names, protocol_config) = match task_opt.map(|t| t.command)
        {
            Some((
                InitCommand { named_addresses },
                SuiInitArgs {
                    accounts,
                    protocol_version,
                    max_gas,
                },
            )) => {
                let map = verify_and_create_named_address_mapping(named_addresses).unwrap();
                let accounts = accounts
                    .map(|v| v.into_iter().collect::<BTreeSet<_>>())
                    .unwrap_or_default();
                let mut protocol_config = if let Some(protocol_version) = protocol_version {
                    ProtocolConfig::get_for_version(protocol_version.into(), Chain::Unknown)
                } else {
                    ProtocolConfig::get_for_max_version_UNSAFE()
                };
                if let Some(mx_tx_gas_override) = max_gas {
                    protocol_config.set_max_tx_gas_for_testing(mx_tx_gas_override)
                }
                (map, accounts, protocol_config)
            }
            None => (
                BTreeMap::new(),
                BTreeSet::new(),
                ProtocolConfig::get_for_max_version_UNSAFE(),
            ),
        };

        let mut named_address_mapping = NAMED_ADDRESSES.clone();

        let mut objects = genesis_module_objects_for_protocol_version(protocol_config.version);
        objects.extend(clone_genesis_objects());
        let mut account_objects = BTreeMap::new();
        let mut accounts = BTreeMap::new();
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
        for n in account_names {
            let test_account = mk_account();
            account_objects.insert(n.clone(), test_account.gas);
            accounts.insert(n, test_account);
        }
        let default_account = mk_account();
        let additional_mapping =
            additional_mapping
                .into_iter()
                .chain(accounts.iter().map(|(n, test_account)| {
                    let addr =
                        NumericalAddress::new(test_account.address.to_inner(), NumberFormat::Hex);
                    (n.clone(), addr)
                }));
        for (name, addr) in additional_mapping {
            if named_address_mapping.contains_key(&name) || name == "sui" {
                panic!("Invalid init. The named address '{}' is reserved", name)
            }
            named_address_mapping.insert(name, addr);
        }

        let object_ids = objects.iter().map(|obj| obj.id()).collect::<Vec<_>>();
        let (validator, fullnode) = {
            let dir = tempfile::TempDir::new().unwrap();
            let network_config =
                sui_swarm_config::network_config_builder::ConfigBuilder::new(&dir).build();
            let genesis = network_config.genesis;
            let keypair = network_config.validator_configs[0]
                .protocol_key_pair()
                .copy();
            let genesis = &genesis;
            let authority_key = &keypair;
            let state = TestAuthorityBuilder::new()
                .with_genesis_and_keypair(genesis, authority_key)
                .with_protocol_config(protocol_config.clone())
                .build()
                .await;
            let fullnode_key_pair = get_authority_key_pair().1;
            let fullnode = TestAuthorityBuilder::new()
                .with_keypair(&fullnode_key_pair)
                .with_protocol_config(protocol_config)
                .build()
                .await;
            state.insert_genesis_objects(&objects).await;
            fullnode.insert_genesis_objects(&objects).await;
            (state, fullnode)
        };
        let mut test_adapter = Self {
            validator,
            fullnode,
            compiled_state: CompiledState::new(
                named_address_mapping,
                pre_compiled_deps,
                Some(NumericalAddress::new(
                    AccountAddress::ZERO.into_bytes(),
                    NumberFormat::Hex,
                )),
            ),
            package_upgrade_mapping: BTreeMap::new(),
            accounts,
            default_account,
            default_syntax,
            object_enumeration: BiBTreeMap::new(),
            next_fake: (0, 0),
            // TODO: make this configurable
            gas_price: 1000,
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
        modules: Vec<(/* package name */ Option<Symbol>, CompiledModule)>,
        gas_budget: Option<u64>,
        extra: Self::ExtraPublishArgs,
    ) -> anyhow::Result<(Option<String>, Vec<(Option<Symbol>, CompiledModule)>)> {
        self.next_task();
        let SuiPublishArgs {
            sender,
            upgradeable,
            dependencies,
        } = extra;
        let named_addr_opt = modules.first().unwrap().0;
        let first_module_name = modules.first().unwrap().1.self_id().name().to_string();
        let modules_bytes = modules
            .iter()
            .map(|(_, module)| {
                let mut module_bytes = vec![];
                module.serialize(&mut module_bytes).unwrap();
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
        let gas_price = self.gas_price;
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
            .map(|(_, published_module_bytes)| {
                (
                    named_addr_opt,
                    CompiledModule::deserialize_with_defaults(published_module_bytes).unwrap(),
                )
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
        assert!(signers.is_empty(), "signers are not used");
        let SuiRunArgs {
            sender,
            gas_price,
            summarize,
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
        let transaction = self.sign_txn(sender, data);
        let summary = self.execute_txn(transaction).await?;
        let output = self.object_summary_output(&summary, summarize);
        let empty = SerializedReturnValues {
            mutable_reference_outputs: vec![],
            return_values: vec![],
        };
        Ok((output, empty))
    }

    async fn execute_script(
        &mut self,
        _script: CompiledScript,
        _type_args: Vec<TypeTag>,
        _signers: Vec<ParsedAddress>,
        _args: Vec<SuiValue>,
        _gas_budget: Option<u64>,
        _extra: Self::ExtraRunArgs,
    ) -> anyhow::Result<(Option<String>, SerializedReturnValues)> {
        bail!("Scripts are not supported")
    }

    async fn view_data(
        &mut self,
        _address: AccountAddress,
        _module: &ModuleId,
        _resource: &IdentStr,
        _type_args: Vec<TypeTag>,
    ) -> anyhow::Result<String> {
        bail!("Resource viewing is not supported")
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
        } = task;
        macro_rules! get_obj {
            ($fake_id:ident, $version:expr) => {{
                let id = match self.fake_to_real_object_id($fake_id) {
                    None => bail!(
                        "task {}, lines {}-{}. Unbound fake id {}",
                        number,
                        start_line,
                        command_lines_stop,
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
            SuiSubcommand::ViewObject(ViewObjectCommand { id: fake_id }) => {
                let obj = get_obj!(fake_id);
                Ok(Some(match &obj.data {
                    object::Data::Move(move_obj) => {
                        let layout = move_obj
                            .get_layout(ObjectFormatOptions::default(), &&*self)
                            .unwrap();
                        let move_struct =
                            MoveStruct::simple_deserialize(move_obj.contents(), &layout).unwrap();
                        self.stabilize_str(format!(
                            "Owner: {}\nVersion: {}\nContents: {}",
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
            }) => {
                let mut builder = ProgrammableTransactionBuilder::new();
                let obj_arg = SuiValue::Object(fake_id, None).into_argument(&mut builder, self)?;
                let recipient = match self.accounts.get(&recipient) {
                    Some(test_account) => test_account.address,
                    None => panic!("Unbound account {}", recipient),
                };
                let gas_budget = gas_budget.unwrap_or(DEFAULT_GAS_BUDGET);
                let gas_price = self.gas_price;
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
                let transaction =
                    VerifiedTransaction::new_consensus_commit_prologue(0, 0, timestamp_ms);
                let summary = self.execute_txn(transaction.into()).await?;
                let output = self.object_summary_output(&summary, /* summarize */ false);
                Ok(output)
            }
            SuiSubcommand::ProgrammableTransaction(ProgrammableTransactionCommand {
                sender,
                gas_budget,
                gas_price,
                dev_inspect,
                inputs,
            }) => {
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
                                    .map(|(_n, m)| {
                                        let mut buf = vec![];
                                        m.serialize(&mut buf).unwrap();
                                        buf
                                    })
                                    .collect();
                                Some(modules)
                            },
                            &|s| Some(state.resolve_named_address(s)),
                        )
                    })
                    .collect::<anyhow::Result<Vec<Command>>>()?;
                let summary = if !dev_inspect {
                    let gas_budget = gas_budget.unwrap_or(DEFAULT_GAS_BUDGET);
                    let gas_price = gas_price.unwrap_or(self.gas_price);
                    let transaction = self.sign_txn(sender, |sender, gas| {
                        TransactionData::new_programmable(
                            sender,
                            vec![gas],
                            ProgrammableTransaction { inputs, commands },
                            gas_budget,
                            gas_price,
                        )
                    });
                    self.execute_txn(transaction).await?
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
            }) => {
                let syntax = syntax.unwrap_or_else(|| self.default_syntax());
                // zero out the package name
                let zero =
                    NumericalAddress::new(AccountAddress::ZERO.into_bytes(), NumberFormat::Hex);
                let before_upgrade = {
                    // not binding `m` separately results in some strange async capture error
                    let m = &mut self.compiled_state.named_address_mapping;
                    let Some(before) = m.insert(package.clone(), zero)
                    else {
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
                    let Some(orig_package) = self.package_upgrade_mapping.get(dep) else { continue };
                    let Some(orig_package_address) = named_address_mapping.insert(orig_package.to_string(), zero) else { continue };
                    original_package_addrs.push((*orig_package, orig_package_address));
                    let dep_address = named_address_mapping
                        .insert(dep.to_string(), orig_package_address)
                        .unwrap_or_else(||
                            panic!("Internal error: expected dependency {dep} in map when overriding address.")
                        );
                    original_package_addrs.push((*dep, dep_address));
                }

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

                        let upgraded_name = modules.first().unwrap().0.unwrap();
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
                let Some(package_name) = modules.first().unwrap().0 else {
                    bail!("Staged modules must have a named address")
                };
                for (named_addr, _) in &modules {
                    let Some(named_addr) = named_addr else {
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
                    .map(|(_, m)| {
                        let mut buf = vec![];
                        m.serialize(&mut buf).unwrap();
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
                                    (Some(*address_sym), module)
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
        }
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

impl<'a> SuiTestAdapter<'a> {
    async fn upgrade_package(
        &mut self,
        before_upgrade: NumericalAddress,
        modules: &[(Option<Symbol>, CompiledModule)],
        upgrade_capability: FakeID,
        dependencies: Vec<String>,
        sender: String,
        gas_budget: Option<u64>,
        policy: u8,
    ) -> anyhow::Result<Option<String>> {
        let modules_bytes = modules
            .iter()
            .map(|(_, module)| {
                let mut module_bytes = vec![];
                module.serialize(&mut module_bytes)?;
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

        let gas_price = self.gas_price;
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
        if let Some(new_package_name) = modules[0].0 {
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
        let test_account = self.get_sender(sender);
        let gas_payment = self
            .get_object(&test_account.gas, None)
            .unwrap()
            .compute_object_reference();
        let data = txn_data(test_account.address, gas_payment);
        to_sender_signed_transaction(data, &test_account.key_pair)
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

    async fn execute_txn(&mut self, transaction: Transaction) -> anyhow::Result<TxnSummary> {
        let with_shared = transaction
            .data()
            .intent_message()
            .value
            .contains_shared_object();
        let (txn, effects, error_opt) = send_and_confirm_transaction_with_execution_error(
            &self.validator,
            Some(&self.fullnode),
            transaction,
            with_shared,
        )
        .await?;
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

        // Treat unwrapped objects as writes (even though sometimes this is the first time we can
        // refer to them at their id in storage).

        // sort by fake id
        created_ids.sort_by_key(|id| self.real_to_fake_object_id(id));
        mutated_ids.sort_by_key(|id| self.real_to_fake_object_id(id));
        unwrapped_ids.sort_by_key(|id| self.real_to_fake_object_id(id));
        deleted_ids.sort_by_key(|id| self.real_to_fake_object_id(id));
        unwrapped_then_deleted_ids.sort_by_key(|id| self.real_to_fake_object_id(id));
        wrapped_ids.sort_by_key(|id| self.real_to_fake_object_id(id));

        match effects.status() {
            ExecutionStatus::Success { .. } => {
                let events = self
                    .validator
                    .query_events(
                        EventFilter::Transaction(*txn.digest()),
                        None,
                        *QUERY_MAX_RESULT_LIMIT,
                        /* descending */ false,
                    )
                    .unwrap_or_default()
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
                })
            }
            ExecutionStatus::Failure { error, command } => {
                let execution_msg = if with_shared {
                    format!(
                        "Cannot return execution error with shared objects. \
                        Debug of error: {error:?} at command {command:?}"
                    )
                } else {
                    format!("Execution Error: {}", error_opt.unwrap())
                };
                Err(anyhow::anyhow!(self.stabilize_str(format!(
                    "Transaction Effects Status: {error}\n{execution_msg}",
                ))))
            }
        }
    }

    async fn dev_inspect(
        &mut self,
        sender: SuiAddress,
        transaction_kind: TransactionKind,
        gas_price: Option<u64>,
    ) -> anyhow::Result<TxnSummary> {
        let results = self
            .fullnode
            .dev_inspect_transaction_block(sender, transaction_kind, gas_price)
            .await?;
        let DevInspectResults {
            effects, events, ..
        } = results;
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

        match effects.status() {
            SuiExecutionStatus::Success { .. } => {
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
                })
            }
            SuiExecutionStatus::Failure { error } => Err(anyhow::anyhow!(self.stabilize_str(
                format!("Transaction Effects Status: {error}\nExecution Error: {error}",)
            ))),
        }
    }

    fn get_object(&self, id: &ObjectID, version: Option<SequenceNumber>) -> anyhow::Result<Object> {
        let obj_res = if let Some(v) = version {
            self.validator.database.get_object_by_key(id, v)
        } else {
            self.validator.database.get_object(id)
        };
        match obj_res {
            Ok(Some(obj)) => Ok(obj),
            Ok(None) | Err(_) => Err(anyhow!("INVALID TEST! Unable to find object {id}")),
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
            .map(|id| match self.real_to_fake_object_id(id) {
                None => "object(_)".to_string(),
                Some(FakeID::Known(id)) => {
                    let id: AccountAddress = id.into();
                    format!("0x{id:x}")
                }
                Some(fake) => format!("object({})", fake),
            })
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

impl<'a> GetModule for &'a SuiTestAdapter<'_> {
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
    map
});

pub(crate) static PRE_COMPILED: Lazy<FullyCompiledProgram> = Lazy::new(|| {
    // TODO invoke package system?
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
    let fully_compiled_res = move_compiler::construct_pre_compiled_lib(
        vec![PackagePaths {
            name: None,
            paths: vec![sui_system_sources, sui_sources, sui_deps, deepbook_sources],
            named_address_map: NAMED_ADDRESSES.clone(),
        }],
        None,
        Flags::empty(),
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
