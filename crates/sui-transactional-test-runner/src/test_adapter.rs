// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This module contains the transactional test runner instantiation for the Sui adapter

use crate::args::*;
use anyhow::bail;
use bimap::btree::BiBTreeMap;
use move_binary_format::{file_format::CompiledScript, CompiledModule};
use move_bytecode_utils::module_cache::GetModule;
use move_command_line_common::{
    address::ParsedAddress, files::verify_and_create_named_address_mapping,
};
use move_compiler::{
    shared::{NumberFormat, NumericalAddress, PackagePaths},
    Flags, FullyCompiledProgram,
};
use move_core_types::{
    account_address::AccountAddress,
    identifier::{IdentStr, Identifier},
    language_storage::{ModuleId, TypeTag},
    value::MoveStruct,
};
use move_transactional_test_runner::{
    framework::{CompiledState, MoveTestAdapter},
    tasks::{InitCommand, SyntaxChoice, TaskInput},
};
use move_vm_runtime::{move_vm::MoveVM, session::SerializedReturnValues};
use once_cell::sync::Lazy;
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::fmt::Write;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
};
use sui_adapter::execution_engine;
use sui_adapter::{adapter::new_move_vm, execution_mode};
use sui_core::transaction_input_checker::check_objects;
use sui_framework::DEFAULT_FRAMEWORK_PATH;
use sui_protocol_config::ProtocolConfig;
use sui_types::epoch_data::EpochData;
use sui_types::gas::SuiCostTable;
use sui_types::id::UID;
use sui_types::in_memory_storage::InMemoryStorage;
use sui_types::messages::Command;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::utils::to_sender_signed_transaction;
use sui_types::{
    base_types::{ObjectID, ObjectRef, SuiAddress, TransactionDigest, SUI_ADDRESS_LENGTH},
    crypto::{get_key_pair_from_rng, AccountKeyPair},
    event::Event,
    gas,
    messages::{
        ExecutionStatus, TransactionData, TransactionDataAPI, TransactionEffectsAPI,
        VerifiedTransaction,
    },
    object::{self, Object, ObjectFormatOptions, GAS_VALUE_FOR_TESTING},
    object::{MoveObject, Owner},
    MOVE_STDLIB_ADDRESS, SUI_CLOCK_OBJECT_ID, SUI_CLOCK_OBJECT_SHARED_VERSION,
    SUI_FRAMEWORK_ADDRESS,
};
use sui_types::{clock::Clock, object::OBJECT_START_VERSION};
use sui_types::{gas::SuiGasStatus, temporary_store::TemporaryStore};

pub(crate) type FakeID = u64;

// initial value for fake object ID mapping
const INIT_NEXT_FAKE: FakeID = 100;
// TODO use the file name as a seed
const RNG_SEED: [u8; 32] = [
    21, 23, 199, 200, 234, 250, 252, 178, 94, 15, 202, 178, 62, 186, 88, 137, 233, 192, 130, 157,
    179, 179, 65, 9, 31, 249, 221, 123, 225, 112, 199, 247,
];

pub struct SuiTestAdapter<'a> {
    vm: Arc<MoveVM>,
    pub(crate) storage: Arc<InMemoryStorage>,
    pub(crate) compiled_state: CompiledState<'a>,
    accounts: BTreeMap<String, (SuiAddress, AccountKeyPair)>,
    default_syntax: SyntaxChoice,
    object_enumeration: BiBTreeMap<ObjectID, FakeID>,
    next_fake: FakeID,
    rng: StdRng,
}

struct TxnSummary {
    created: Vec<ObjectID>,
    written: Vec<ObjectID>,
    deleted: Vec<ObjectID>,
    events: Vec<Event>,
}

static GENESIS: Lazy<Genesis> = Lazy::new(create_genesis_module_objects);
static PROTOCOL_CONSTANTS: Lazy<ProtocolConfig> = Lazy::new(ProtocolConfig::get_for_max_version);

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

pub fn get_framework_object_ref() -> ObjectRef {
    GENESIS
        .packages
        .iter()
        .find(|o| o.id() == SUI_FRAMEWORK_ADDRESS.into())
        .unwrap()
        .compute_object_reference()
}

/// Create and return objects wrapping the genesis modules for sui
fn create_genesis_module_objects() -> Genesis {
    let sui_modules = sui_framework::get_sui_framework();
    let std_modules = sui_framework::get_move_stdlib();
    let objects = vec![create_clock()];
    // SAFETY: unwraps safe because genesis packages should never exceed max size
    let packages = vec![
        Object::new_package(
            std_modules.clone(),
            OBJECT_START_VERSION,
            TransactionDigest::genesis(),
            PROTOCOL_CONSTANTS.max_move_package_size(),
        )
        .unwrap(),
        Object::new_package(
            sui_modules.clone(),
            OBJECT_START_VERSION,
            TransactionDigest::genesis(),
            PROTOCOL_CONSTANTS.max_move_package_size(),
        )
        .unwrap(),
    ];
    let modules = vec![std_modules, sui_modules];
    Genesis {
        objects,
        packages,
        modules,
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
            Clock::type_(),
            has_public_transfer,
            SUI_CLOCK_OBJECT_SHARED_VERSION,
            contents,
            &PROTOCOL_CONSTANTS,
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
        let (additional_mapping, account_names) = match task_opt.map(|t| t.command) {
            Some((InitCommand { named_addresses }, SuiInitArgs { accounts })) => {
                let map = verify_and_create_named_address_mapping(named_addresses).unwrap();
                let accounts = accounts
                    .map(|v| v.into_iter().collect::<BTreeSet<_>>())
                    .unwrap_or_default();
                (map, accounts)
            }
            None => (BTreeMap::new(), BTreeSet::new()),
        };
        let accounts = account_names
            .into_iter()
            .map(|n| (n, get_key_pair_from_rng(&mut rng)))
            .collect::<BTreeMap<_, _>>();

        let mut named_address_mapping = NAMED_ADDRESSES.clone();
        let additional_mapping = additional_mapping.into_iter().chain(accounts.iter().map(
            |(n, (addr, _)): (_, &(_, AccountKeyPair))| {
                let addr = NumericalAddress::new(addr.to_inner(), NumberFormat::Hex);
                (n.clone(), addr)
            },
        ));
        for (name, addr) in additional_mapping {
            if named_address_mapping.contains_key(&name) || name == "sui" {
                panic!("Invalid init. The named address '{}' is reserved", name)
            }
            named_address_mapping.insert(name, addr);
        }

        let native_functions =
            sui_framework::natives::all_natives(MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS);
        let mut objects = clone_genesis_packages();
        objects.extend(clone_genesis_objects());
        let mut account_objects = BTreeMap::new();
        for (account, (addr, _)) in &accounts {
            let obj = Object::with_id_owner_for_testing(ObjectID::new(rng.gen()), *addr);
            objects.push(obj.clone());
            account_objects.insert(account.clone(), obj);
        }

        let mut test_adapter = Self {
            vm: Arc::new(new_move_vm(native_functions, &PROTOCOL_CONSTANTS).unwrap()),
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
            default_syntax,
            object_enumeration: BiBTreeMap::new(),
            next_fake: INIT_NEXT_FAKE,
            rng,
        };
        let object_ids = test_adapter
            .storage
            .objects()
            .keys()
            .copied()
            .collect::<Vec<_>>();
        let mut output = String::new();
        for (account, obj) in account_objects {
            let fake = test_adapter.enumerate_fake(obj.id());
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

    fn publish_module(
        &mut self,
        module: CompiledModule,
        named_addr_opt: Option<Identifier>,
        gas_budget: Option<u64>,
        extra: Self::ExtraPublishArgs,
    ) -> anyhow::Result<(Option<String>, CompiledModule)> {
        let SuiPublishArgs { sender } = extra;
        let module_name = module.self_id().name().to_string();
        let module_bytes = {
            let mut buf = vec![];
            module.serialize(&mut buf).unwrap();
            buf
        };
        let gas_budget = gas_budget.unwrap_or(GAS_VALUE_FOR_TESTING);
        let data = |sender, gas| {
            let mut builder = ProgrammableTransactionBuilder::new();
            builder.command(Command::Publish(vec![module_bytes]));
            let pt = builder.finish();
            TransactionData::new_programmable_with_dummy_gas_price(sender, gas, pt, gas_budget)
        };
        let transaction = self.sign_txn(sender, data);
        let summary = self.execute_txn(transaction, gas_budget)?;
        let created_package = summary
            .created
            .iter()
            .find_map(|id| {
                let package = self.storage.get_object(id).unwrap().data.try_as_package()?;
                if package.serialized_module_map().get(&module_name).is_some() {
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
        let view_events = false;
        let output = self.object_summary_output(&summary, view_events);
        let published_module_bytes = self
            .storage
            .get_object(&created_package)
            .unwrap()
            .data
            .try_as_package()
            .unwrap()
            .serialized_module_map()
            .get(&module_name)
            .unwrap()
            .clone();
        let published_module = CompiledModule::deserialize(&published_module_bytes).unwrap();
        Ok((output, published_module))
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
        assert!(signers.is_empty(), "signers are not used");
        let SuiRunArgs {
            sender,
            view_events,
        } = extra;
        let arguments = args
            .into_iter()
            .map(|arg| arg.into_call_args(self))
            .collect::<anyhow::Result<_>>()?;
        let package_id = ObjectID::from(*module_id.address());

        let gas_budget = gas_budget.unwrap_or(GAS_VALUE_FOR_TESTING);
        let data = |sender, gas| {
            let mut builder = ProgrammableTransactionBuilder::new();
            builder
                .move_call(
                    package_id,
                    module_id.name().to_owned(),
                    function.to_owned(),
                    type_args,
                    arguments,
                )
                .unwrap();
            let pt = builder.finish();
            TransactionData::new_programmable_with_dummy_gas_price(sender, gas, pt, gas_budget)
        };
        let transaction = self.sign_txn(sender, data);
        let summary = self.execute_txn(transaction, gas_budget)?;
        let output = self.object_summary_output(&summary, view_events);
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
        let TaskInput {
            command,
            name: _,
            number,
            start_line,
            command_lines_stop,
            stop_line: _,
            data: _,
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
                let obj_arg = SuiValue::Object(fake_id).into_call_args(self)?;
                let recipient = match self.accounts.get(&recipient) {
                    Some((recipient, _)) => *recipient,
                    None => panic!("Unbound account {}", recipient),
                };
                let gas_budget = gas_budget.unwrap_or(GAS_VALUE_FOR_TESTING);
                let transaction = self.sign_txn(sender, |sender, gas| {
                    let mut builder = ProgrammableTransactionBuilder::new();
                    let rec_arg = builder.pure(recipient).unwrap();
                    let obj_arg = builder.input(obj_arg).unwrap();
                    builder.command(sui_types::messages::Command::TransferObjects(
                        vec![obj_arg],
                        rec_arg,
                    ));
                    let pt = builder.finish();
                    TransactionData::new_programmable_with_dummy_gas_price(
                        sender, gas, pt, gas_budget,
                    )
                });
                let summary = self.execute_txn(transaction, gas_budget)?;
                let output = self.object_summary_output(&summary, false);
                Ok(output)
            }
            SuiSubcommand::ConsensusCommitPrologue(ConsensusCommitPrologueCommand {
                timestamp_ms,
            }) => {
                let transaction =
                    VerifiedTransaction::new_consensus_commit_prologue(0, 0, timestamp_ms);
                let summary = self.execute_txn(transaction, GAS_VALUE_FOR_TESTING)?;
                let output = self.object_summary_output(&summary, false);
                Ok(output)
            }
        }
    }
}

impl<'a> SuiTestAdapter<'a> {
    fn sign_txn(
        &mut self,
        sender: Option<String>,
        txn_data: impl FnOnce(/* sender */ SuiAddress, /* gas */ ObjectRef) -> TransactionData,
    ) -> VerifiedTransaction {
        let gas_object_id = ObjectID::new(self.rng.gen());
        assert!(!self.object_enumeration.contains_left(&gas_object_id));
        self.enumerate_fake(gas_object_id);
        let new_key_pair;
        let (sender, sender_key) = match sender {
            Some(n) => match self.accounts.get(&n) {
                Some((sender, sender_key)) => (*sender, sender_key),
                None => panic!("Unbound account {}", n),
            },
            None => {
                let (sender, sender_key) = get_key_pair_from_rng(&mut self.rng);
                new_key_pair = sender_key;
                (sender, &new_key_pair)
            }
        };
        let gas_object = Object::with_id_owner_for_testing(gas_object_id, sender);
        let gas_payment = gas_object.compute_object_reference();
        let storage_mut = Arc::get_mut(&mut self.storage).unwrap();
        storage_mut.insert_object(gas_object);
        let data = txn_data(sender, gas_payment);
        to_sender_signed_transaction(data, sender_key)
    }

    fn execute_txn(
        &mut self,
        transaction: VerifiedTransaction,
        gas_budget: u64,
    ) -> anyhow::Result<TxnSummary> {
        let gas_status = if transaction.inner().is_system_tx() {
            SuiGasStatus::new_unmetered()
        } else {
            gas::start_gas_metering(gas_budget, 1, 1, SuiCostTable::new(&PROTOCOL_CONSTANTS))
                .unwrap()
        };
        transaction
            .data()
            .transaction_data()
            .validity_check(&PROTOCOL_CONSTANTS)?;
        let transaction_digest = TransactionDigest::new(self.rng.gen());
        let (input_objects, objects) = transaction
            .data()
            .intent_message
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
            &PROTOCOL_CONSTANTS,
        );
        let transaction_data = &transaction.into_inner().into_data().intent_message.value;
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
            &PROTOCOL_CONSTANTS,
        );

        let mut created_ids: Vec<_> = effects
            .created()
            .iter()
            .map(|((id, _, _), _)| *id)
            .collect();
        let unwrapped_ids: Vec<_> = effects
            .unwrapped()
            .iter()
            .map(|((id, _, _), _)| *id)
            .collect();
        let mut written_ids: Vec<_> = effects
            .mutated()
            .iter()
            .map(|((id, _, _), _)| *id)
            .collect();
        let mut deleted_ids: Vec<_> = effects
            .deleted()
            .iter()
            .chain(effects.wrapped())
            .map(|(id, _, _)| *id)
            .collect();

        // update storage
        Arc::get_mut(&mut self.storage)
            .unwrap()
            .finish(inner.written, inner.deleted);

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
        written_ids.extend(unwrapped_ids.into_iter());

        // sort by fake id
        created_ids.sort_by_key(|id| self.real_to_fake_object_id(id));
        written_ids.sort_by_key(|id| self.real_to_fake_object_id(id));
        deleted_ids.sort_by_key(|id| self.real_to_fake_object_id(id));

        match effects.status() {
            ExecutionStatus::Success { .. } => Ok(TxnSummary {
                created: created_ids,
                written: written_ids,
                deleted: deleted_ids,
                events: inner.events.data,
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
            sui_types::object::Data::Move(obj) => self.stabilize_str(format!("{}", obj.type_)),
            sui_types::object::Data::Package(pkg) => pkg
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
        let fake_id = self.next_fake;
        self.object_enumeration.insert(id, fake_id);
        self.next_fake += 1;
        fake_id
    }

    fn object_summary_output(
        &self,
        TxnSummary {
            created,
            written,
            deleted,
            events,
        }: &TxnSummary,
        view_events: bool,
    ) -> Option<String> {
        let mut out = String::new();
        if view_events {
            if events.is_empty() {
                out += "No events"
            } else {
                write!(out, "events: {}", self.list_events(events)).unwrap();
            }
        }
        if !created.is_empty() {
            if !out.is_empty() {
                out.push('\n')
            }
            write!(out, "created: {}", self.list_objs(created)).unwrap();
        }
        if !written.is_empty() {
            if !out.is_empty() {
                out.push('\n')
            }
            write!(out, "written: {}", self.list_objs(written)).unwrap();
        }
        if !deleted.is_empty() {
            if !out.is_empty() {
                out.push('\n')
            }
            write!(out, "deleted: {}", self.list_objs(deleted)).unwrap();
        }

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
            Some(fake) => format!("fake({})", fake),
        }
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
    map
});

pub(crate) static PRE_COMPILED: Lazy<FullyCompiledProgram> = Lazy::new(|| {
    // TODO invoke package system?
    let sui_files: &Path = Path::new(DEFAULT_FRAMEWORK_PATH);
    let sui_sources: String = {
        let mut buf = sui_files.to_path_buf();
        buf.push("sources");
        buf.to_string_lossy().to_string()
    };
    let sui_deps = {
        let mut buf = sui_files.to_path_buf();
        buf.push("deps");
        buf.push("move-stdlib");
        buf.push("sources");
        buf.to_string_lossy().to_string()
    };
    let fully_compiled_res = move_compiler::construct_pre_compiled_lib(
        vec![PackagePaths {
            name: None,
            paths: vec![sui_sources, sui_deps],
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
