// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use indexmap::IndexSet;
use move_binary_format::file_format::Visibility;
use move_binary_format::normalized::Type;
use move_core_types::language_storage::StructTag;
use rand::rngs::StdRng;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use sui_json_rpc_types::{SuiTransactionBlockEffects, SuiTransactionBlockEffectsAPI};
use sui_move_build::BuildConfig;
use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress};
use sui_types::messages::{CallArg, ObjectArg, TransactionData, TEST_ONLY_GAS_UNIT_FOR_PUBLISH};
use sui_types::object::{Object, Owner};
use sui_types::storage::WriteKind;
use sui_types::{Identifier, SUI_FRAMEWORK_ADDRESS};
use test_utils::network::TestCluster;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

#[derive(Debug, Clone)]
pub struct EntryFunction {
    pub package: ObjectID,
    pub module: String,
    pub function: String,
    pub parameters: Vec<Type>,
}

#[derive(Default)]
pub struct SurfStatistics {
    pub num_successful_transactions: u64,
    pub num_failed_transactions: u64,
    pub num_owned_obj_transactions: u64,
    pub num_shared_obj_transactions: u64,
    pub unique_move_functions_called: HashSet<(ObjectID, String, String)>,
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
    }
}

pub type OwnedObjects = HashMap<StructTag, IndexSet<ObjectRef>>;

pub type ImmObjects = Arc<RwLock<HashMap<StructTag, Vec<ObjectRef>>>>;

/// Map from StructTag to a vector of shared objects, where each shared object is a tuple of
/// (object ID, initial shared version).
pub type SharedObjects = Arc<RwLock<HashMap<StructTag, Vec<(ObjectID, SequenceNumber)>>>>;

pub struct SurferState {
    pub cluster: Arc<TestCluster>,
    pub rng: StdRng,

    pub address: SuiAddress,
    pub gas_object: ObjectRef,
    pub owned_objects: OwnedObjects,
    pub immutable_objects: ImmObjects,
    pub shared_objects: SharedObjects,
    pub entry_functions: Arc<RwLock<Vec<EntryFunction>>>,

    pub stats: SurfStatistics,
}

impl SurferState {
    pub fn new(
        cluster: Arc<TestCluster>,
        rng: StdRng,
        address: SuiAddress,
        gas_object: ObjectRef,
        owned_objects: OwnedObjects,
        immutable_objects: ImmObjects,
        shared_objects: SharedObjects,
        entry_functions: Arc<RwLock<Vec<EntryFunction>>>,
    ) -> Self {
        Self {
            cluster,
            rng,
            address,
            gas_object,
            owned_objects,
            immutable_objects,
            shared_objects,
            entry_functions,
            stats: Default::default(),
        }
    }

    pub async fn execute_move_transaction(
        &mut self,
        package: ObjectID,
        module: String,
        function: String,
        args: Vec<CallArg>,
    ) {
        let rgp = self.cluster.get_reference_gas_price().await;
        let use_shared_object = args
            .iter()
            .any(|arg| matches!(arg, CallArg::Object(ObjectArg::SharedObject { .. })));
        let tx_data = TransactionData::new_move_call(
            self.address,
            package,
            Identifier::new(module.as_str()).unwrap(),
            Identifier::new(function.as_str()).unwrap(),
            vec![],
            self.gas_object,
            args,
            TEST_ONLY_GAS_UNIT_FOR_PUBLISH * rgp,
            rgp,
        )
        .unwrap();
        let tx = self.cluster.sign_transaction(&self.address, &tx_data);
        let response = loop {
            match self.cluster.execute_transaction(tx.clone()).await {
                Ok(effects) => break effects,
                Err(e) => {
                    error!("Error executing transaction: {:?}", e);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        };
        debug!(
            "Successfully executed transaction {:?} with response {:?}",
            tx, response
        );
        let effects = response.effects.unwrap();
        info!(
            "[{:?}] Calling Move function {:?}::{:?} returned {:?}",
            self.address,
            module,
            function,
            effects.status()
        );
        self.stats.record_transaction(
            use_shared_object,
            effects.status().is_ok(),
            package,
            module,
            function,
        );
        self.process_tx_effects(&effects).await;
    }

    async fn process_tx_effects(&mut self, effects: &SuiTransactionBlockEffects) {
        for (owned_ref, write_kind) in effects.all_changed_objects() {
            let obj_ref = owned_ref.reference.to_object_ref();
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
                    self.immutable_objects
                        .write()
                        .await
                        .entry(struct_tag)
                        .or_default()
                        .push(obj_ref);
                }
                Owner::AddressOwner(address) => {
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
                } => {
                    if write_kind != WriteKind::Mutate {
                        self.shared_objects
                            .write()
                            .await
                            .entry(struct_tag)
                            .or_default()
                            .push((obj_ref.0, initial_shared_version));
                    }
                    // We do not need to insert it if it's a Mutate, because it means
                    // we should already have it in the inventory.
                }
            }
            if obj_ref.0 == self.gas_object.0 {
                self.gas_object = obj_ref;
            }
        }
    }

    async fn discover_entry_functions(&self, package: Object) {
        let package_id = package.id();
        let move_package = package.data.try_into_package().unwrap();
        let config = ProtocolConfig::get_for_max_version();
        let entry_functions: Vec<_> = move_package
            .normalize(
                config.move_binary_format_version(),
                config.no_extraneous_module_bytes(),
            )
            .unwrap()
            .into_iter()
            .flat_map(|(module_name, module)| {
                module
                    .functions
                    .into_iter()
                    .filter_map(|(func_name, func)| {
                        // Either public function or entry function is callable.
                        if !matches!(func.visibility, Visibility::Public) && !func.is_entry {
                            return None;
                        }
                        // Surfer doesn't support chaining transactions in a programmable transaction yet.
                        if !func.return_.is_empty() {
                            return None;
                        }
                        // Surfer doesn't support type parameter yet.
                        if !func.type_parameters.is_empty() {
                            return None;
                        }
                        let mut parameters = func.parameters;
                        if let Some(last_param) = parameters.last().as_ref() {
                            if is_type_tx_context(last_param) {
                                parameters.pop();
                            }
                        }
                        Some(EntryFunction {
                            package: package_id,
                            module: module_name.clone(),
                            function: func_name.to_string(),
                            parameters,
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
        self.entry_functions.write().await.extend(entry_functions);
    }

    pub async fn publish_package(&mut self, path: PathBuf) {
        let rgp = self.cluster.get_reference_gas_price().await;
        let package = BuildConfig::new_for_testing().build(path.clone()).unwrap();
        let modules = package.get_package_bytes(false);
        let tx_data = TransactionData::new_module(
            self.address,
            self.gas_object,
            modules,
            package.dependency_ids.published.values().cloned().collect(),
            TEST_ONLY_GAS_UNIT_FOR_PUBLISH * rgp,
            rgp,
        );
        let tx = self.cluster.sign_transaction(&self.address, &tx_data);
        let response = loop {
            match self.cluster.execute_transaction(tx.clone()).await {
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
            .await
            .get(type_tag)
            .map(|objects| objects.len())
            .unwrap_or(0)
    }

    pub async fn matching_shared_objects_count(&self, type_tag: &StructTag) -> usize {
        self.shared_objects
            .read()
            .await
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
        self.immutable_objects.read().await.get(type_tag).unwrap()[n]
    }

    pub async fn choose_nth_shared_object(
        &self,
        type_tag: &StructTag,
        n: usize,
    ) -> (ObjectID, SequenceNumber) {
        self.shared_objects.read().await.get(type_tag).unwrap()[n]
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
