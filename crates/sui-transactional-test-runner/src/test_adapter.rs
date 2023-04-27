// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This module contains the transactional test runner instantiation for the Sui adapter

use crate::{args::*, programmable_transaction_test_parser::parser::ParsedCommand};
use anyhow::bail;
use bimap::btree::BiBTreeMap;
use fastcrypto::hash::MultisetHash;
use move_binary_format::{file_format::CompiledScript, CompiledModule};
use move_bytecode_utils::module_cache::GetModule;
use move_command_line_common::{
    address::ParsedAddress, files::verify_and_create_named_address_mapping,
};
use move_compiler::{
    compiled_unit::AnnotatedCompiledUnit,
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
    framework::{compile_ir_module, compile_source_units, CompiledState, MoveTestAdapter},
    tasks::{InitCommand, SyntaxChoice, TaskInput},
};
use move_vm_runtime::{move_vm::MoveVM, session::SerializedReturnValues};
use once_cell::sync::Lazy;
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::fmt::{self, Write};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
};
use sui_adapter::execution_engine;
use sui_adapter::{adapter::new_move_vm, execution_mode};
use sui_core::{
    state_accumulator::{accumulate_effects, WrappedObject},
    transaction_input_checker::check_objects,
};
use sui_framework::BuiltInFramework;
use sui_framework::DEFAULT_FRAMEWORK_PATH;
use sui_protocol_config::ProtocolConfig;
use sui_types::accumulator::Accumulator;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::execution_status::ExecutionStatus;
use sui_types::MOVE_STDLIB_OBJECT_ID;
use sui_types::{
    base_types::{ObjectID, ObjectRef, SuiAddress, TransactionDigest, SUI_ADDRESS_LENGTH},
    crypto::{get_key_pair_from_rng, AccountKeyPair},
    event::Event,
    messages::{TransactionData, TransactionDataAPI, VerifiedTransaction},
    object::{self, Object, ObjectFormatOptions},
    object::{MoveObject, Owner},
    MOVE_STDLIB_ADDRESS, SUI_CLOCK_OBJECT_ID, SUI_CLOCK_OBJECT_SHARED_VERSION,
    SUI_FRAMEWORK_ADDRESS, SUI_SYSTEM_STATE_OBJECT_ID,
};
use sui_types::{clock::Clock, SUI_SYSTEM_ADDRESS};
use sui_types::{epoch_data::EpochData, messages::Command};
use sui_types::{gas::SuiGasStatus, temporary_store::TemporaryStore};
use sui_types::{
    gas::{GasCostSummary, SuiCostTable},
    object::GAS_VALUE_FOR_TESTING,
};
use sui_types::{id::UID, DEEPBOOK_ADDRESS};
use sui_types::{in_memory_storage::InMemoryStorage, messages::ProgrammableTransaction};
use sui_types::{
    messages::{Argument, CallArg},
    move_package::MovePackage,
};
use sui_types::{metrics::LimitsMetrics, DEEPBOOK_OBJECT_ID};
use sui_types::{
    programmable_transaction_builder::ProgrammableTransactionBuilder, SUI_FRAMEWORK_OBJECT_ID,
};
use sui_types::{utils::to_sender_signed_transaction, SUI_SYSTEM_OBJECT_ID};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum FakeID {
    Known(ObjectID),
    Enumerated(u64, u64),
}

const WELL_KNOWN_OBJECTS: &[ObjectID] = &[
    MOVE_STDLIB_OBJECT_ID,
    DEEPBOOK_OBJECT_ID,
    SUI_FRAMEWORK_OBJECT_ID,
    SUI_SYSTEM_OBJECT_ID,
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
    vm: Arc<MoveVM>,
    pub(crate) storage: Arc<InMemoryStorage>,
    pub(crate) compiled_state: CompiledState<'a>,
    accounts: BTreeMap<String, TestAccount>,
    default_account: TestAccount,
    default_syntax: SyntaxChoice,
    object_enumeration: BiBTreeMap<ObjectID, FakeID>,
    next_fake: (u64, u64),
    rng: StdRng,
    gas_price: u64,
    protocol_config: ProtocolConfig,
    metrics: Arc<LimitsMetrics>,
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

pub fn clone_genesis_packages() -> Vec<Object> {
    GENESIS.packages.clone()
}

pub fn clone_genesis_objects() -> Vec<Object> {
    GENESIS.objects.clone()
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

    fn init(
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
                },
            )) => {
                let map = verify_and_create_named_address_mapping(named_addresses).unwrap();
                let accounts = accounts
                    .map(|v| v.into_iter().collect::<BTreeSet<_>>())
                    .unwrap_or_default();
                let protocol_config = if let Some(protocol_version) = protocol_version {
                    ProtocolConfig::get_for_version(protocol_version.into())
                } else {
                    ProtocolConfig::get_for_max_version()
                };
                (map, accounts, protocol_config)
            }
            None => (
                BTreeMap::new(),
                BTreeSet::new(),
                ProtocolConfig::get_for_max_version(),
            ),
        };

        let mut named_address_mapping = NAMED_ADDRESSES.clone();

        let native_functions = sui_move_natives::all_natives(/* silent */ false);
        let mut objects = clone_genesis_packages();
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

        // Use a throwaway metrics registry for testing.
        let registry = prometheus::Registry::new();
        let metrics = Arc::new(LimitsMetrics::new(&registry));

        let enable_move_vm_paranoid_checks = false;
        let mut test_adapter = Self {
            vm: Arc::new(
                new_move_vm(
                    native_functions,
                    &protocol_config,
                    enable_move_vm_paranoid_checks,
                )
                .unwrap(),
            ),
            storage: Arc::new(InMemoryStorage::new(objects)),
            compiled_state: CompiledState::new(
                named_address_mapping,
                pre_compiled_deps,
                Some(NumericalAddress::new(
                    AccountAddress::ZERO.into_bytes(),
                    NumberFormat::Hex,
                )),
            ),
            accounts,
            default_account,
            default_syntax,
            object_enumeration: BiBTreeMap::new(),
            next_fake: (0, 0),
            rng,
            // TODO: make this configurable
            gas_price: 1000,
            protocol_config,
            metrics,
        };
        for well_known in WELL_KNOWN_OBJECTS.iter().copied() {
            test_adapter
                .object_enumeration
                .insert(well_known, FakeID::Known(well_known));
        }
        let object_ids = test_adapter
            .storage
            .objects()
            .keys()
            .copied()
            .collect::<Vec<_>>();
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

    fn publish_modules(
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
        let gas_price = self.gas_price;
        // we are assuming that all packages depend on Move Stdlib and Sui Framework, so these
        // don't have to be provided explicitly as parameters
        dependencies.extend([MOVE_STDLIB_OBJECT_ID, SUI_FRAMEWORK_OBJECT_ID]);
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
        let summary = self.execute_txn(transaction, gas_budget, false)?;
        let created_package = summary
            .created
            .iter()
            .find_map(|id| {
                let package = self.storage.get_object(id).unwrap().data.try_as_package()?;
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
        let output = self.object_summary_output(&summary);
        let published_modules = self
            .storage
            .get_object(&created_package)
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

    fn call_function(
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
            protocol_version,
            uncharged,
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
        let default_protocol_version = self.protocol_config.version;
        if let Some(protocol_version) = protocol_version {
            // override protocol version, just for this call
            self.protocol_config = ProtocolConfig::get_for_version(protocol_version.into())
        }
        let summary = self.execute_txn(transaction, gas_budget, uncharged)?;
        let output = self.object_summary_output(&summary);
        // restore old protocol version (if needed)
        if protocol_version.is_some() {
            self.protocol_config = ProtocolConfig::get_for_version(default_protocol_version)
        }
        let empty = SerializedReturnValues {
            mutable_reference_outputs: vec![],
            return_values: vec![],
        };
        Ok((output, empty))
    }

    fn execute_script(
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

    fn view_data(
        &mut self,
        _address: AccountAddress,
        _module: &ModuleId,
        _resource: &IdentStr,
        _type_args: Vec<TypeTag>,
    ) -> anyhow::Result<String> {
        bail!("Resource viewing is not supported")
    }

    fn handle_subcommand(
        &mut self,
        task: TaskInput<Self::Subcommand>,
    ) -> anyhow::Result<Option<String>> {
        self.next_task();
        let TaskInput {
            command,
            name: _,
            number,
            start_line,
            command_lines_stop,
            stop_line: _,
            data,
        } = task;
        macro_rules! get_obj {
            ($fake_id:ident) => {{
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
                match self.storage.get_object(&id) {
                    None => return Ok(Some(format!("No object at id {}", $fake_id))),
                    Some(obj) => obj,
                }
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
                let obj_arg = SuiValue::Object(fake_id).into_argument(&mut builder, self)?;
                let recipient = match self.accounts.get(&recipient) {
                    Some(test_account) => test_account.address,
                    None => panic!("Unbound account {}", recipient),
                };
                let gas_budget = gas_budget.unwrap_or(DEFAULT_GAS_BUDGET);
                let transaction = self.sign_txn(sender, |sender, gas| {
                    let rec_arg = builder.pure(recipient).unwrap();
                    builder.command(sui_types::messages::Command::TransferObjects(
                        vec![obj_arg],
                        rec_arg,
                    ));
                    let pt = builder.finish();
                    TransactionData::new_programmable(sender, vec![gas], pt, gas_budget, 1)
                });
                let summary = self.execute_txn(transaction, gas_budget, false)?;
                let output = self.object_summary_output(&summary);
                Ok(output)
            }
            SuiSubcommand::ConsensusCommitPrologue(ConsensusCommitPrologueCommand {
                timestamp_ms,
            }) => {
                let transaction =
                    VerifiedTransaction::new_consensus_commit_prologue(0, 0, timestamp_ms);
                let summary = self.execute_txn(transaction, DEFAULT_GAS_BUDGET, false)?;
                let output = self.object_summary_output(&summary);
                Ok(output)
            }
            SuiSubcommand::ProgrammableTransaction(ProgrammableTransactionCommand {
                sender,
                gas_budget,
                gas_price,
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
                let state = &self.compiled_state;
                let commands = commands
                    .into_iter()
                    .map(|c| c.into_command(&|s| Some(state.resolve_named_address(s))))
                    .collect::<anyhow::Result<Vec<Command>>>()?;
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
                let summary = self.execute_txn(transaction, gas_budget, false)?;
                let output = self.object_summary_output(&summary);
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
                let data = data.ok_or_else(|| {
                    anyhow::anyhow!(
                        "Expected a module text block following 'upgrade' starting on lines {}-{}",
                        start_line,
                        command_lines_stop
                    )
                })?;

                let state = self.compiled_state();
                let (mut modules, warnings_opt) = match syntax {
                    SyntaxChoice::Source => {
                        let (units, warnings_opt) =
                            compile_source_units(state, data.path(), Some(package.clone()))?;
                        let modules = units
                            .into_iter()
                            .map(|unit| match unit {
                                AnnotatedCompiledUnit::Module(annot_module) => {
                                    let (named_addr_opt, _id) = annot_module.module_id();
                                    let named_addr_opt = named_addr_opt.map(|n| n.value);
                                    let module = annot_module.named_module.module;
                                    (named_addr_opt, module)
                                }
                                AnnotatedCompiledUnit::Script(_) => panic!(
                                    "Expected a module text block, not a script, \
                                    following 'upgrade' starting on lines {}-{}",
                                    start_line, command_lines_stop
                                ),
                            })
                            .collect();
                        (modules, warnings_opt)
                    }
                    SyntaxChoice::IR => {
                        let module = compile_ir_module(state, data.path())?;
                        (vec![(None, module)], None)
                    }
                };
                let output = self.upgrade_package(
                    package,
                    &modules,
                    upgrade_capability,
                    dependencies,
                    sender,
                    gas_budget,
                    policy,
                )?;
                match syntax {
                    SyntaxChoice::Source => {
                        let path = data.path().to_str().unwrap().to_owned();
                        self.compiled_state()
                            .add_with_source_file(modules, (path, data))
                    }
                    SyntaxChoice::IR => {
                        let module = modules.pop().unwrap().1;
                        self.compiled_state()
                            .add_and_generate_interface_file(module);
                    }
                };
                Ok(merge_output(warnings_opt, output))
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

fn accumulate_in_memory_store(store: &InMemoryStorage) -> Accumulator {
    let mut acc = Accumulator::default();
    for (_, obj) in store.objects().iter() {
        acc.insert(obj.compute_object_reference().2);
    }

    for (id, version) in store.wrapped().iter() {
        acc.insert(
            bcs::to_bytes(&WrappedObject::new(*id, *version))
                .expect("Failed to serialize WrappedObject"),
        );
    }
    acc
}

impl<'a> SuiTestAdapter<'a> {
    fn upgrade_package(
        &mut self,
        package: String,
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
        dependencies.extend([MOVE_STDLIB_OBJECT_ID, SUI_FRAMEWORK_OBJECT_ID]);

        let mut builder = ProgrammableTransactionBuilder::new();

        SuiValue::Object(upgrade_capability).into_argument(&mut builder, self)?; // Argument::Input(0)
        let upgrade_arg = builder.pure(policy).unwrap();
        let digest: Vec<u8> = MovePackage::compute_digest_for_modules_and_deps(
            &modules_bytes,
            &dependencies,
            /* hash_modules */ true,
        )
        .into();
        let digest_arg = builder.pure(digest).unwrap();

        let upgrade_ticket = builder.programmable_move_call(
            SUI_FRAMEWORK_OBJECT_ID,
            ident_str!("package").to_owned(),
            ident_str!("authorize_upgrade").to_owned(),
            vec![],
            vec![Argument::Input(0), upgrade_arg, digest_arg],
        );

        let package_id = self
            .compiled_state
            .resolve_named_address(package.as_str())
            .into();
        let upgrade_receipt =
            builder.upgrade(package_id, upgrade_ticket, dependencies, modules_bytes);

        builder.programmable_move_call(
            SUI_FRAMEWORK_OBJECT_ID,
            ident_str!("package").to_owned(),
            ident_str!("commit_upgrade").to_owned(),
            vec![],
            vec![Argument::Input(0), upgrade_receipt],
        );

        let pt = builder.finish();

        let data =
            |sender, gas| TransactionData::new_programmable(sender, vec![gas], pt, gas_budget, 1);

        let transaction = self.sign_txn(Some(sender), data);
        let summary = self.execute_txn(transaction, gas_budget, false)?;
        let created_package = summary
            .created
            .iter()
            .find_map(|id| {
                let package = self.storage.get_object(id).unwrap().data.try_as_package()?;
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
        let output = self.object_summary_output(&summary);
        Ok(output)
    }

    fn sign_txn(
        &mut self,
        sender: Option<String>,
        txn_data: impl FnOnce(/* sender */ SuiAddress, /* gas */ ObjectRef) -> TransactionData,
    ) -> VerifiedTransaction {
        let test_account = match sender {
            Some(n) => match self.accounts.get(&n) {
                Some(test_account) => test_account,
                None => panic!("Unbound account {}", n),
            },
            None => &self.default_account,
        };
        let gas_payment = self
            .storage
            .get_object(&test_account.gas)
            .unwrap()
            .compute_object_reference();
        let data = txn_data(test_account.address, gas_payment);
        to_sender_signed_transaction(data, &test_account.key_pair)
    }

    fn execute_txn(
        &mut self,
        transaction: VerifiedTransaction,
        gas_budget: u64,
        uncharged: bool,
    ) -> anyhow::Result<TxnSummary> {
        let mut gas_status = if transaction.inner().is_system_tx() {
            SuiGasStatus::new_unmetered(&self.protocol_config)
        } else {
            SuiCostTable::new(&self.protocol_config).into_gas_status_for_testing(
                gas_budget,
                self.gas_price,
                self.protocol_config.storage_gas_price(),
            )
        };
        // Unmetered is set in the transaction run without metering. NB that this will still keep
        // in place the normal transaction execution limits.
        if uncharged {
            match &mut gas_status {
                SuiGasStatus::V1(gas_status) => {
                    gas_status.gas_status.set_metering(false);
                }
                SuiGasStatus::V2(gas_status) => {
                    gas_status.gas_status.set_metering(false);
                }
            }
        }
        transaction
            .data()
            .transaction_data()
            .validity_check(&self.protocol_config)?;
        let transaction_digest = TransactionDigest::new(self.rng.gen());
        let (input_objects, objects) = transaction
            .data()
            .intent_message()
            .value
            .input_objects()?
            .into_iter()
            .flat_map(|kind| {
                let id = kind.object_id();
                // might be none if passed a bad object to invoke
                let obj = self.storage.get_object(&id)?.clone();
                Some((kind, obj))
            })
            .unzip();
        let input_objects = check_objects(
            transaction.data().transaction_data(),
            input_objects,
            objects,
        )?;
        let transaction_dependencies = input_objects.transaction_dependencies();
        let shared_object_refs: Vec<_> = input_objects.filter_shared_objects();
        let temporary_store = TemporaryStore::new(
            self.storage.clone(),
            input_objects,
            transaction_digest,
            &self.protocol_config,
        );
        let transaction_data = &transaction
            .into_inner()
            .into_data()
            .intent_message()
            .value
            .clone();
        let (kind, signer, gas) = transaction_data.execution_parts();

        let (
            inner,
            effects,
            /*
            TransactionEffects {
                status,
                created,
                mutated,
                unwrapped,
                // TODO display all these somehow
                transaction_digest: _,
                deleted,
                wrapped,
                gas_object: _,
                ..
            },
            */
            execution_error,
        ) = execution_engine::execute_transaction_to_effects::<execution_mode::Normal, _>(
            shared_object_refs,
            temporary_store,
            kind,
            signer,
            &gas,
            transaction_digest,
            transaction_dependencies,
            &self.vm,
            gas_status,
            // TODO: Support different epochs in transactional tests.
            &EpochData::new_test(),
            &self.protocol_config,
            self.metrics.clone(),
            false, // enable_expensive_checks
        );
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

        let effects_accum =
            accumulate_effects(&*self.storage, vec![effects.clone()], &self.protocol_config);

        let mut before = accumulate_in_memory_store(&self.storage);

        // update storage
        Arc::get_mut(&mut self.storage)
            .unwrap()
            .finish(inner.written, inner.deleted);

        let after = accumulate_in_memory_store(&self.storage);

        before.union(&effects_accum);
        assert_eq!(before.digest(), after.digest());

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
            ExecutionStatus::Success { .. } => Ok(TxnSummary {
                events: inner.events.data,
                gas_summary: gas_summary.clone(),
                created: created_ids,
                mutated: mutated_ids,
                unwrapped: unwrapped_ids,
                deleted: deleted_ids,
                unwrapped_then_deleted: unwrapped_then_deleted_ids,
                wrapped: wrapped_ids,
            }),
            ExecutionStatus::Failure { error, .. } => {
                Err(anyhow::anyhow!(self.stabilize_str(format!(
                    "Transaction Effects Status: {}\nExecution Error: {}",
                    error,
                    execution_error.expect_err(
                        "to have an execution error if a transaction's status is a failure"
                    )
                ))))
            }
        }
    }

    // stable way of sorting objects by type. Does not however, produce a stable sorting
    // between objects of the same type
    fn get_object_sorting_key(&self, id: &ObjectID) -> String {
        match &self.storage.get_object(id).unwrap().data {
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
    ) -> Option<String> {
        let mut out = String::new();
        if !events.is_empty() {
            write!(out, "events: {}", self.list_events(events)).unwrap();
        }
        if !created.is_empty() {
            if !out.is_empty() {
                out.push('\n')
            }
            write!(out, "created: {}", self.list_objs(created)).unwrap();
        }
        if !mutated.is_empty() {
            if !out.is_empty() {
                out.push('\n')
            }
            write!(out, "mutated: {}", self.list_objs(mutated)).unwrap();
        }
        if !unwrapped.is_empty() {
            if !out.is_empty() {
                out.push('\n')
            }
            write!(out, "unwrapped: {}", self.list_objs(unwrapped)).unwrap();
        }
        if !deleted.is_empty() {
            if !out.is_empty() {
                out.push('\n')
            }
            write!(out, "deleted: {}", self.list_objs(deleted)).unwrap();
        }
        if !unwrapped_then_deleted.is_empty() {
            if !out.is_empty() {
                out.push('\n')
            }
            write!(
                out,
                "unwrapped_then_deleted: {}",
                self.list_objs(unwrapped_then_deleted)
            )
            .unwrap();
        }
        if !wrapped.is_empty() {
            if !out.is_empty() {
                out.push('\n')
            }
            write!(out, "wrapped: {}", self.list_objs(wrapped)).unwrap();
        }
        out.push('\n');
        write!(out, "gas summary: {}", gas_summary).unwrap();

        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    }

    fn list_events(&self, events: &[Event]) -> String {
        events
            .iter()
            .map(|event| self.stabilize_str(format!("{:?}", event)))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn list_objs(&self, objs: &[ObjectID]) -> String {
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
