// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{args::*, in_memory_storage::InMemoryStorage};
use anyhow::{anyhow, bail};
use bimap::btree::BiBTreeMap;
use itertools::Itertools;
use move_binary_format::{file_format::CompiledScript, CompiledModule};
use move_command_line_common::files::verify_and_create_named_address_mapping;
use move_compiler::{
    shared::{NumberFormat, NumericalAddress, PackagePaths},
    Flags, FullyCompiledProgram,
};
use move_core_types::{
    account_address::AccountAddress,
    identifier::{IdentStr, Identifier},
    language_storage::{ModuleId, TypeTag},
    transaction_argument::TransactionArgument,
};
use move_transactional_test_runner::{
    framework::{CompiledState, MoveTestAdapter},
    tasks::{EmptyCommand, InitCommand, SyntaxChoice},
};
use move_vm_runtime::{
    move_vm::MoveVM, native_functions::NativeFunctionTable, session::SerializedReturnValues,
};
use once_cell::sync::Lazy;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
};
use sui_adapter::{adapter::new_move_vm, genesis};
use sui_core::{authority::AuthorityTemporaryStore, execution_engine};
use sui_framework::DEFAULT_FRAMEWORK_PATH;
use sui_types::{
    base_types::{ObjectID, ObjectRef, SuiAddress, TransactionDigest, SUI_ADDRESS_LENGTH},
    crypto::{get_key_pair, KeyPair, Signature},
    error::SuiError,
    gas,
    messages::{
        ExecutionStatus, InputObjectKind, Transaction, TransactionData, TransactionEffects,
    },
    object::{Object, GAS_VALUE_FOR_TESTING},
    MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS,
};

/// This module contains the transactional test runner instantiation for the Sui adapter

// initial value for fake object ID mapping
const INIT_NEXT_OBJECT: usize = 100;

pub struct SuiTestAdapter<'a> {
    vm: Arc<MoveVM>,
    pub(crate) storage: Arc<InMemoryStorage>,
    native_functions: NativeFunctionTable,
    pub(crate) compiled_state: CompiledState<'a>,
    accounts: BTreeMap<String, (SuiAddress, KeyPair)>,
    default_syntax: SyntaxChoice,
    object_enumeration: BiBTreeMap<ObjectID, [u8; SUI_ADDRESS_LENGTH]>,
    next_object: usize,
}

impl<'a> MoveTestAdapter<'a> for SuiTestAdapter<'a> {
    type ExtraPublishArgs = SuiPublishArgs;
    type ExtraRunArgs = SuiRunArgs;
    type Subcommand = EmptyCommand;
    type ExtraInitArgs = SuiInitArgs;

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
    ) -> Self {
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
            .map(|n| (n, get_key_pair()))
            .collect::<BTreeMap<_, _>>();

        let mut named_address_mapping = NAMED_ADDRESSES.clone();
        let additional_mapping =
            additional_mapping
                .into_iter()
                .chain(accounts.iter().map(|(n, (addr, _))| {
                    let addr = NumericalAddress::new(addr.to_inner(), NumberFormat::Hex);
                    (n.clone(), addr)
                }));
        for (name, addr) in additional_mapping {
            if named_address_mapping.contains_key(&name) || name == "Sui" {
                panic!("Invalid init. The named address '{}' is reserved", name)
            }
            named_address_mapping.insert(name, addr);
        }

        let native_functions =
            sui_framework::natives::all_natives(MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS);
        Self {
            vm: Arc::new(new_move_vm(native_functions.clone()).unwrap()),
            storage: Arc::new(InMemoryStorage::new(genesis::clone_genesis_packages())),
            native_functions,
            compiled_state: CompiledState::new(named_address_mapping, pre_compiled_deps),
            accounts,
            default_syntax,
            object_enumeration: BiBTreeMap::new(),
            next_object: INIT_NEXT_OBJECT,
        }
    }

    fn publish_module(
        &mut self,
        module: CompiledModule,
        named_addr_opt: Option<Identifier>,
        gas_budget: Option<u64>,
        extra: Self::ExtraPublishArgs,
    ) -> anyhow::Result<()> {
        let SuiPublishArgs { sender } = extra;
        let named_addr = named_addr_opt.expect(
            "Cannot publish without a named address. \
            This named address will be associated with the published package",
        );
        let module_name = module.self_id().name().to_string();
        let module_bytes = {
            let mut buf = vec![];
            module.serialize(&mut buf).unwrap();
            buf
        };
        let gas_budget = gas_budget.unwrap_or(GAS_VALUE_FOR_TESTING);
        let data = |sender, gas_payment| {
            TransactionData::new_module(sender, gas_payment, vec![module_bytes], gas_budget)
        };
        let transaction = self.sign_txn(sender, data);
        let (written, _deleted) = self.execute_txn(transaction, gas_budget)?;
        let created_package = written
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
        let prev_package = self
            .compiled_state
            .insert_named_address(named_addr.to_string(), package_addr);
        match prev_package.map(|a| a.into_inner()) {
            Some(addr) if addr != AccountAddress::ZERO => panic!(
                "Cannot reuse named address '{}' for multiple packages. \
                It should be set to 0 initially",
                named_addr
            ),
            _ => (),
        }
        Ok(())
    }

    fn call_function(
        &mut self,
        module_id: &ModuleId,
        function: &IdentStr,
        type_args: Vec<TypeTag>,
        signers: Vec<move_transactional_test_runner::tasks::RawAddress>,
        empty_args: Vec<TransactionArgument>,
        gas_budget: Option<u64>,
        extra: Self::ExtraRunArgs,
    ) -> anyhow::Result<(Option<String>, SerializedReturnValues)> {
        assert!(signers.is_empty(), "signers are not used");
        assert!(empty_args.is_empty(), "Use '--{}' instead", SUI_ARGS_LONG);
        let SuiRunArgs { args, sender } = extra;
        let arguments = args
            .into_iter()
            .map(|arg| arg.into_call_args(self))
            .collect();
        let package_id = ObjectID::from(*module_id.address());
        let package = self
            .storage
            .get_object(&package_id)
            .unwrap()
            .compute_object_reference();
        let gas_budget = gas_budget.unwrap_or(GAS_VALUE_FOR_TESTING);
        let data = |sender, gas_payment| {
            TransactionData::new_move_call(
                sender,
                package,
                module_id.name().to_owned(),
                function.to_owned(),
                type_args,
                gas_payment,
                arguments,
                gas_budget,
            )
        };
        let transaction = self.sign_txn(sender, data);
        self.execute_txn(transaction, gas_budget)?;
        let empty = SerializedReturnValues {
            mutable_reference_outputs: vec![],
            return_values: vec![],
        };
        Ok((None, empty))
    }

    fn execute_script(
        &mut self,
        _script: CompiledScript,
        _type_args: Vec<TypeTag>,
        _signers: Vec<move_transactional_test_runner::tasks::RawAddress>,
        _args: Vec<TransactionArgument>,
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
        _subcommand: move_transactional_test_runner::tasks::TaskInput<Self::Subcommand>,
    ) -> anyhow::Result<Option<String>> {
        unimplemented!()
    }
}

impl<'a> SuiTestAdapter<'a> {
    fn sign_txn(
        &mut self,
        sender: Option<String>,
        txn_data: impl FnOnce(/* sender */ SuiAddress, /* gas */ ObjectRef) -> TransactionData,
    ) -> Transaction {
        let new_key_pair;
        let (sender, sender_key) = match sender {
            Some(n) => match self.accounts.get(&n) {
                Some((sender, sender_key)) => (*sender, sender_key),
                None => panic!("Unbound account {}", n),
            },
            None => {
                let (sender, sender_key) = get_key_pair();
                new_key_pair = sender_key;
                (sender, &new_key_pair)
            }
        };
        let gas_object = Object::with_id_owner_for_testing(ObjectID::random(), sender);
        let gas_payment = gas_object.compute_object_reference();
        let storage_mut = Arc::get_mut(&mut self.storage).unwrap();
        storage_mut.insert_object(gas_object);
        let data = txn_data(sender, gas_payment);
        let signature = Signature::new(&data, sender_key);
        Transaction::new(data, signature)
    }

    fn execute_txn(
        &mut self,
        transaction: Transaction,
        gas_budget: u64,
    ) -> anyhow::Result<(
        /* written */ BTreeSet<ObjectID>,
        /* deleted */ BTreeSet<ObjectID>,
    )> {
        let gas_status = gas::start_gas_metering(gas_budget, 1, 1).unwrap();
        let transaction_digest = TransactionDigest::random();
        let objects_by_kind = transaction
            .data
            .input_objects()
            .unwrap()
            .into_iter()
            .map(|kind| {
                let id = kind.object_id();
                let obj_opt = self.storage.get_object(&id);
                if obj_opt.is_none() {
                    dbg!(id);
                    println!("no object? {}", id);
                }
                let obj = obj_opt.unwrap().clone();
                (kind, obj)
            })
            .collect::<Vec<_>>();
        let transaction_dependencies = objects_by_kind
            .iter()
            .map(|(_, obj)| obj.previous_transaction)
            .collect();
        let shared_object_refs: Vec<_> = objects_by_kind
            .iter()
            .filter(|(kind, _)| matches!(kind, InputObjectKind::SharedMoveObject(_)))
            .map(|(_, obj)| obj.compute_object_reference())
            .sorted()
            .collect();
        let mut temporary_store =
            AuthorityTemporaryStore::new(self.storage.clone(), objects_by_kind, transaction_digest);
        let TransactionEffects {
            status,
            // TODO display all these somehow
            transaction_digest: _,
            created,
            mutated: _,
            unwrapped: _,
            deleted: _,
            wrapped: _,
            gas_object: _,
            events: _,
            ..
        } = execution_engine::execute_transaction_to_effects(
            shared_object_refs,
            &mut temporary_store,
            transaction.data,
            transaction_digest,
            transaction_dependencies,
            &self.vm,
            &self.native_functions,
            gas_status,
        )?;
        for ((id, _, _), _) in created {
            self.enumerate_object(id);
        }
        let (_objects, _active_inputs, written, deleted, _events) = temporary_store.into_inner();
        let written_ids = written.keys().copied().collect();
        let deleted_ids = deleted.keys().copied().collect();
        Arc::get_mut(&mut self.storage)
            .unwrap()
            .finish(written, deleted);
        match status {
            ExecutionStatus::Success { .. } => Ok((written_ids, deleted_ids)),
            ExecutionStatus::Failure { error, .. } => Err(self.render_sui_error(*error)),
        }
    }

    pub(crate) fn fake_to_real_object_id(&self, id: [u8; SUI_ADDRESS_LENGTH]) -> Option<ObjectID> {
        self.object_enumeration.get_by_right(&id).copied()
    }

    pub(crate) fn real_to_fake_object_id(&self, id: &ObjectID) -> Option<[u8; SUI_ADDRESS_LENGTH]> {
        self.object_enumeration.get_by_left(id).copied()
    }

    fn enumerate_object(&mut self, id: ObjectID) -> [u8; SUI_ADDRESS_LENGTH] {
        const USIZE_LENGTH: usize = std::mem::size_of::<usize>();
        let mut fake_id = [0; SUI_ADDRESS_LENGTH];
        let next_id_bytes: [u8; USIZE_LENGTH] = self.next_object.to_be_bytes();
        fake_id[(SUI_ADDRESS_LENGTH - USIZE_LENGTH)..].clone_from_slice(&next_id_bytes);
        let prev = self.object_enumeration.insert(id, fake_id);
        assert!(!prev.did_overwrite());
        self.next_object += 1;
        fake_id
    }

    fn render_sui_error(&self, sui_error: SuiError) -> anyhow::Error {
        const HEX_STR_LENGTH: usize = SUI_ADDRESS_LENGTH * 2;
        let error_string: String = format!("{}", sui_error);
        let mut hex_candidate = String::new();
        let mut result = String::new();
        for c in error_string.chars() {
            match c {
                '0'..='9' | 'a'..='f' | 'A'..='F' => hex_candidate.push(c),
                _ => {
                    match hex_candidate.len() {
                        0 => (),
                        HEX_STR_LENGTH => {
                            result.push_str(&self.remap_hex_str(hex_candidate));
                            hex_candidate = String::new();
                        }
                        _ => {
                            result.push_str(&hex_candidate);
                            hex_candidate = String::new();
                        }
                    }
                    result.push(c);
                }
            }
        }
        match hex_candidate.len() {
            0 => (),
            HEX_STR_LENGTH => {
                result.push_str(&self.remap_hex_str(hex_candidate));
            }
            _ => {
                result.push_str(&hex_candidate);
            }
        }
        anyhow!(result)
    }

    fn remap_hex_str(&self, mut hex_str: String) -> String {
        hex_str.make_ascii_lowercase();
        let bytes: [u8; SUI_ADDRESS_LENGTH] = hex::decode(&hex_str).unwrap().try_into().unwrap();
        match self.real_to_fake_object_id(&ObjectID::new(bytes)) {
            None => "_".to_string(),
            Some(addr) => format!("{}", ObjectID::new(addr)),
        }
    }
}

static NAMED_ADDRESSES: Lazy<BTreeMap<String, NumericalAddress>> = Lazy::new(|| {
    let mut map = move_stdlib::move_stdlib_named_addresses();
    assert!(map.get("Std").unwrap().into_inner() == MOVE_STDLIB_ADDRESS);
    // TODO fix Sui framework constants
    map.insert(
        "Sui".to_string(),
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
