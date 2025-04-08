// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use indexmap::IndexSet;
use move_binary_format::file_format::Visibility;
use move_binary_format::normalized::Type;
use move_core_types::language_storage::StructTag;
use mysten_common::fatal;
use parking_lot::RwLock;
use rand::rngs::StdRng;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use sui_json_rpc_types::{
    SuiExecutionStatus, SuiTransactionBlockEffects, SuiTransactionBlockEffectsAPI,
};
use sui_move_build::BuildConfig;
use sui_protocol_config::{Chain, ProtocolConfig};
use sui_types::base_types::{ConsensusObjectSequenceKey, ObjectID, ObjectRef, SuiAddress};
use sui_types::execution_config_utils::to_binary_config;
use sui_types::object::{Object, Owner};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::storage::WriteKind;
use sui_types::transaction::{
    Argument, CallArg, ObjectArg, TransactionData, TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
};
use sui_types::{Identifier, SUI_FRAMEWORK_ADDRESS, SUI_FRAMEWORK_PACKAGE_ID};
use test_cluster::TestCluster;
use tokio::sync::watch;
use tracing::{debug, error, info};

use crate::surf_strategy::ErrorChecks;
use crate::EntryFunctionFilterFn;

#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct EntryFunction {
    pub package: ObjectID,
    pub module: String,
    pub function: String,
    pub parameters: Vec<Type>,
    // return type must be either none or a single object (by value)
    pub return_type: Option<Type>,
}

impl EntryFunction {
    pub fn qualified_name(&self) -> String {
        format!("{}::{}", self.module, self.function)
    }
}
#[derive(Clone, Debug, Default)]
pub struct SurfStatistics {
    pub num_successful_transactions: u64,
    pub num_failed_transactions: u64,
    pub num_owned_obj_transactions: u64,
    pub num_shared_obj_transactions: u64,
    pub unique_move_functions_called: HashSet<(ObjectID, String, String)>,
    pub unique_move_functions_called_success: HashSet<(ObjectID, String, String)>,
}

impl SurfStatistics {
    pub fn record_transaction(
        &mut self,
        has_shared_object: bool,
        tx_succeeded: bool,
        package: ObjectID,
        module: String,
        function: String,
    ) {
        if tx_succeeded {
            self.num_successful_transactions += 1;
            self.unique_move_functions_called_success.insert((
                package,
                module.clone(),
                function.clone(),
            ));
        } else {
            self.num_failed_transactions += 1;
        }
        if has_shared_object {
            self.num_shared_obj_transactions += 1;
        } else {
            self.num_owned_obj_transactions += 1;
        }
        self.unique_move_functions_called
            .insert((package, module, function));
    }

    pub fn aggregate(stats: Vec<Self>) -> Self {
        let mut result = Self::default();
        for stat in stats {
            result.num_successful_transactions += stat.num_successful_transactions;
            result.num_failed_transactions += stat.num_failed_transactions;
            result.num_owned_obj_transactions += stat.num_owned_obj_transactions;
            result.num_shared_obj_transactions += stat.num_shared_obj_transactions;
            result
                .unique_move_functions_called
                .extend(stat.unique_move_functions_called);
            result
                .unique_move_functions_called_success
                .extend(stat.unique_move_functions_called_success);
        }
        result
    }

    pub fn print_stats(&self) {
        info!(
            "Executed {} transactions, {} succeeded, {} failed",
            self.num_successful_transactions + self.num_failed_transactions,
            self.num_successful_transactions,
            self.num_failed_transactions
        );
        info!(
            "{} are owned object transactions, {} are shared object transactions",
            self.num_owned_obj_transactions, self.num_shared_obj_transactions
        );
        info!(
            "Unique move functions called: {}",
            self.unique_move_functions_called.len()
        );
        info!(
            "Unique move functions called successfully: {}",
            self.unique_move_functions_called_success.len()
        );
    }
}

pub type OwnedObjects = HashMap<StructTag, IndexSet<ObjectRef>>;

pub type ImmObjects = Arc<RwLock<HashMap<StructTag, Vec<ObjectRef>>>>;

/// Map from StructTag to a vector of shared objects, where each shared object is a tuple of
/// (object ID, initial shared version).
pub type SharedObjects = Arc<RwLock<HashMap<StructTag, Vec<ConsensusObjectSequenceKey>>>>;

pub struct SurferState {
    pub id: usize,
    pub cluster: Arc<TestCluster>,
    pub rng: StdRng,

    pub address: SuiAddress,
    pub gas_object: ObjectRef,
    pub owned_objects: OwnedObjects,
    pub immutable_objects: ImmObjects,
    pub shared_objects: SharedObjects,
    pub entry_functions: Arc<RwLock<Vec<EntryFunction>>>,
    pub entry_function_filter: Option<EntryFunctionFilterFn>,

    pub stats: watch::Sender<SurfStatistics>,
}

impl SurferState {
    pub fn new(
        id: usize,
        cluster: Arc<TestCluster>,
        rng: StdRng,
        address: SuiAddress,
        gas_object: ObjectRef,
        owned_objects: OwnedObjects,
        immutable_objects: ImmObjects,
        shared_objects: SharedObjects,
        entry_functions: Arc<RwLock<Vec<EntryFunction>>>,
        entry_function_filter: Option<EntryFunctionFilterFn>,
    ) -> Self {
        Self {
            id,
            cluster,
            rng,
            address,
            gas_object,
            owned_objects,
            immutable_objects,
            shared_objects,
            entry_functions,
            entry_function_filter,
            stats: watch::channel(Default::default()).0,
        }
    }

    #[tracing::instrument(skip_all, fields(surfer_id = self.id))]
    pub async fn execute_move_transaction(
        &mut self,
        entry: &EntryFunction,
        args: Vec<CallArg>,
        error_checking_mode: ErrorChecks,
    ) {
        let EntryFunction {
            package,
            module,
            function,
            return_type,
            ..
        } = entry;

        let rgp = self.cluster.get_reference_gas_price().await;
        let use_shared_object = args
            .iter()
            .any(|arg| matches!(arg, CallArg::Object(ObjectArg::SharedObject { .. })));

        let mut pt = ProgrammableTransactionBuilder::new();
        pt.move_call(
            *package,
            Identifier::new(module.as_str()).unwrap(),
            Identifier::new(function.as_str()).unwrap(),
            vec![],
            args,
        )
        .unwrap();

        // Transfer object outputs to ourselves
        if let Some(rt) = return_type {
            let Type::Struct {
                module,
                name,
                type_arguments,
                ..
            } = rt
            else {
                fatal!("Return type is not a struct: {:?}", rt);
            };

            // Special case certain output types - for instance Balance<T> must be
            // turned into Coin<T> before transferring.
            if module.to_string() == "balance" && name.to_string() == "Balance" {
                pt.programmable_move_call(
                    SUI_FRAMEWORK_PACKAGE_ID,
                    Identifier::new("coin").unwrap(),
                    Identifier::new("from_balance").unwrap(),
                    type_arguments
                        .iter()
                        .map(|t| t.clone().into_type_tag().unwrap())
                        .collect(),
                    vec![Argument::Result(0)],
                );

                pt.transfer_arg(self.address, Argument::Result(1));
            } else {
                pt.transfer_arg(self.address, Argument::Result(0));
            }
        }
        let pt = pt.finish();

        let tx_data = TransactionData::new_programmable(
            self.address,
            vec![self.gas_object],
            pt,
            TEST_ONLY_GAS_UNIT_FOR_PUBLISH * rgp,
            rgp,
        );

        let tx = self.cluster.wallet.sign_transaction(&tx_data);
        let response = loop {
            match self
                .cluster
                .wallet
                .execute_transaction_may_fail_no_local_execution(tx.clone())
                .await
            {
                Ok(effects) => break effects,
                Err(e) => {
                    error!("Error executing transaction: {}", e);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        };

        self.cluster
            .wait_for_transaction_on_fullnode(tx.digest(), Duration::from_secs(30))
            .await;

        debug!(
            "Successfully executed transaction {:?} with response {:?}",
            tx, response
        );
        let effects = response.effects.unwrap();
        info!(
            "[{:?}] Calling Move function {} returned {:?}",
            self.address,
            entry.qualified_name(),
            effects.status()
        );

        match (error_checking_mode, effects.status()) {
            (_, SuiExecutionStatus::Success) => (),
            (ErrorChecks::None, _) => (),
            (ErrorChecks::WellFormed, SuiExecutionStatus::Failure { error }) => {
                assert!(
                    error.starts_with("MoveAbort"),
                    "Unexpected error: {} for transaction {:?}",
                    error,
                    tx.transaction_data(),
                );
            }
            (ErrorChecks::Strict, SuiExecutionStatus::Failure { error }) => {
                fatal!(
                    "Unexpected error: {} for transaction {:?}",
                    error,
                    tx.transaction_data()
                );
            }
        }

        self.stats.send_modify(|stats| {
            stats.record_transaction(
                use_shared_object,
                effects.status().is_ok(),
                *package,
                module.to_string(),
                function.to_string(),
            );
        });
        self.process_tx_effects(&effects).await;
    }

    #[tracing::instrument(skip_all, fields(surfer_id = self.id))]
    async fn process_tx_effects(&mut self, effects: &SuiTransactionBlockEffects) {
        for (owned_ref, write_kind) in effects.all_changed_objects() {
            if matches!(owned_ref.owner, Owner::ObjectOwner(_)) {
                // For object owned objects, we don't need to do anything.
                // We also cannot read them because in the case of shared objects, there can be
                // races and the child object may no longer exist.
                continue;
            }
            let obj_ref = owned_ref.reference.to_object_ref();
            if obj_ref.0 == self.gas_object.0 {
                // cannot support transferring away the gas coin or otherwise
                // loosing access to it.
                assert_eq!(owned_ref.owner, self.address);
                assert_eq!(write_kind, WriteKind::Mutate);
                debug!("updating gas object ref: {:?}", obj_ref);
                self.gas_object = obj_ref;
                continue;
            }

            let object = self
                .cluster
                .get_object_from_fullnode_store(&obj_ref.0)
                .await
                .unwrap();
            if object.is_package() {
                self.discover_entry_functions(object).await;
                continue;
            }
            let struct_tag = object.struct_tag().unwrap();
            match owned_ref.owner {
                Owner::Immutable => {
                    info!(
                        "New immutable object of type {}: ID: {:?} ",
                        struct_tag.to_canonical_display(false),
                        obj_ref.0
                    );
                    self.immutable_objects
                        .write()
                        .entry(struct_tag)
                        .or_default()
                        .push(obj_ref);
                }
                Owner::AddressOwner(address) => {
                    info!(
                        "New owned object of type {}: ID: {:?} ",
                        struct_tag, obj_ref.0
                    );
                    if address == self.address {
                        self.owned_objects
                            .entry(struct_tag)
                            .or_default()
                            .insert(obj_ref);
                    }
                }
                Owner::ObjectOwner(_) => (),
                Owner::Shared {
                    initial_shared_version,
                }
                // TODO: Implement full support for ConsensusV2 objects in sui-surfer.
                | Owner::ConsensusV2 {
                    start_version: initial_shared_version,
                    ..
                } => {
                    if write_kind == WriteKind::Create {
                        info!(
                            "New shared object of type {}: ID: {:?} ",
                            struct_tag.to_canonical_display(false),
                            obj_ref.0
                        );
                        self.shared_objects
                            .write()
                            .entry(struct_tag)
                            .or_default()
                            .push((obj_ref.0, initial_shared_version));
                    }
                    // We do not need to insert it if it's a Mutate, because it means
                    // we should already have it in the inventory.
                }
            }
        }
    }

    #[tracing::instrument(skip_all, fields(surfer_id = self.id))]
    pub async fn add_package(&self, package: ObjectID) {
        let object = self
            .cluster
            .get_object_from_fullnode_store(&package)
            .await
            .unwrap();
        assert!(object.is_package());
        self.discover_entry_functions(object).await;
    }

    async fn discover_entry_functions(&self, package: Object) {
        // Sui Surfer can call functions that return nothing, or that return a single
        // object
        fn get_allowable_return_type(return_type: &[Type]) -> Result<Option<Type>, ()> {
            if return_type.is_empty() {
                return Ok(None);
            }

            if return_type.len() > 1 {
                return Err(());
            }

            let ret = &return_type[0];

            if !ret.is_closed() {
                return Err(());
            }

            if matches!(ret, Type::Struct { .. }) {
                Ok(Some(ret.clone()))
            } else {
                Err(())
            }
        }

        let package_id = package.id();
        let move_package = package.into_inner().data.try_into_package().unwrap();
        let proto_version = self.cluster.highest_protocol_version();
        let config = ProtocolConfig::get_for_version(proto_version, Chain::Unknown);
        let binary_config = to_binary_config(&config);
        let entry_functions: Vec<_> = move_package
            .normalize(&binary_config)
            .unwrap()
            .into_iter()
            .flat_map(|(module_name, module)| {
                module
                    .functions
                    .into_iter()
                    .filter_map(|(func_name, func)| {
                        info!("Checking entry function: {}::{}", module_name, func_name);

                        // check if name is excluded by regex
                        if let Some(filter) = &self.entry_function_filter {
                            if !filter(&module_name, func_name.as_str()) {
                                debug!("-- excluded by filter");
                                return None;
                            }
                        }

                        // Either public function or entry function is callable.
                        if !matches!(func.visibility, Visibility::Public) && !func.is_entry {
                            debug!("-- not callable");
                            return None;
                        }

                        let Ok(return_type) = get_allowable_return_type(&func.return_) else {
                            debug!("-- bad return type {:?}", func.return_);
                            return None;
                        };

                        // Surfer doesn't support type parameter yet.
                        if !func.type_parameters.is_empty() {
                            debug!("-- type parameters not supported");
                            return None;
                        }
                        let mut parameters = func.parameters;
                        if let Some(last_param) = parameters.last().as_ref() {
                            if is_type_tx_context(last_param) {
                                parameters.pop();
                            }
                        }
                        info!("Discovered entry function: {}::{}", module_name, func_name);
                        Some(EntryFunction {
                            package: package_id,
                            module: module_name.clone(),
                            function: func_name.to_string(),
                            parameters,
                            return_type,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        info!(
            "Number of entry functions discovered: {:?}",
            entry_functions.len()
        );
        debug!("Entry functions: {:?}", entry_functions);
        self.entry_functions.write().extend(entry_functions);
    }

    #[tracing::instrument(skip_all, fields(surfer_id = self.id))]
    pub async fn publish_package(&mut self, path: &Path) {
        let rgp = self.cluster.get_reference_gas_price().await;
        let package = BuildConfig::new_for_testing().build(path).unwrap();
        let modules = package.get_package_bytes(false);
        let tx_data = TransactionData::new_module(
            self.address,
            self.gas_object,
            modules,
            package.dependency_ids.published.values().cloned().collect(),
            TEST_ONLY_GAS_UNIT_FOR_PUBLISH * rgp,
            rgp,
        );
        let tx = self.cluster.wallet.sign_transaction(&tx_data);
        let response = loop {
            match self
                .cluster
                .wallet
                .execute_transaction_may_fail(tx.clone())
                .await
            {
                Ok(response) => {
                    break response;
                }
                Err(err) => {
                    error!("Failed to publish package: {:?}", err);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        };
        info!("Successfully published package in {:?}", path);
        self.process_tx_effects(&response.effects.unwrap()).await;
    }

    pub fn matching_owned_objects_count(&self, type_tag: &StructTag) -> usize {
        self.owned_objects
            .get(type_tag)
            .map(|objects| objects.len())
            .unwrap_or(0)
    }

    pub async fn matching_immutable_objects_count(&self, type_tag: &StructTag) -> usize {
        self.immutable_objects
            .read()
            .get(type_tag)
            .map(|objects| objects.len())
            .unwrap_or(0)
    }

    pub async fn matching_shared_objects_count(&self, type_tag: &StructTag) -> usize {
        self.shared_objects
            .read()
            .get(type_tag)
            .map(|objects| objects.len())
            .unwrap_or(0)
    }

    pub fn choose_nth_owned_object(&mut self, type_tag: &StructTag, n: usize) -> ObjectRef {
        self.owned_objects
            .get_mut(type_tag)
            .unwrap()
            .swap_remove_index(n)
            .unwrap()
    }

    pub async fn choose_nth_immutable_object(&self, type_tag: &StructTag, n: usize) -> ObjectRef {
        self.immutable_objects.read().get(type_tag).unwrap()[n]
    }

    pub async fn choose_nth_shared_object(
        &self,
        type_tag: &StructTag,
        n: usize,
    ) -> ConsensusObjectSequenceKey {
        self.shared_objects.read().get(type_tag).unwrap()[n]
    }
}

pub fn get_type_tag(arg_type: Type) -> Option<StructTag> {
    match arg_type {
        Type::Struct {
            address,
            module,
            name,
            type_arguments,
        } => Some(StructTag {
            address,
            module,
            name,
            type_params: type_arguments
                .into_iter()
                .map(|t| t.into_type_tag().unwrap())
                .collect(),
        }),
        _ => None,
    }
}

fn is_type_tx_context(ty: &Type) -> bool {
    match ty {
        Type::Reference(inner) | Type::MutableReference(inner) => match inner.as_ref() {
            Type::Struct {
                address,
                module,
                name,
                type_arguments,
            } => {
                address == &SUI_FRAMEWORK_ADDRESS
                    && module == &Identifier::new("tx_context").unwrap()
                    && name == &Identifier::new("TxContext").unwrap()
                    && type_arguments.is_empty()
            }
            _ => false,
        },
        _ => false,
    }
}
