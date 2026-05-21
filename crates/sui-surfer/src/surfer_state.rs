// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use indexmap::{IndexMap, IndexSet};
use move_binary_format::file_format::Visibility;
use move_binary_format::normalized;
use move_core_types::identifier::IdentStr;
use move_core_types::language_storage::StructTag;
use mysten_common::fatal;
use rand::Rng;
use rand::rngs::StdRng;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use sui_move_build::BuildConfig;
use sui_protocol_config::{Chain, ProtocolConfig};
use sui_types::base_types::{
    ConsensusObjectSequenceKey, ObjectID, ObjectRef, SequenceNumber, SuiAddress,
};
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::object::{Object, Owner};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::storage::WriteKind;
use sui_types::transaction::{CallArg, ObjectArg, TEST_ONLY_GAS_UNIT_FOR_PUBLISH, TransactionData};
use sui_types::{Identifier, SUI_FRAMEWORK_ADDRESS};
use test_cluster::TestCluster;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

/// How a transaction's gas is paid. The surfer randomizes this to exercise the
/// different gas-payment code paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GasMode {
    /// Pay with the account's gas coin (the common case).
    Normal,
    /// Pay gas from the sender's address balance (accumulator).
    AddressBalance,
    /// Free-tier transaction: address-balance gas with price 0.
    Gasless,
}

/// Max number of times to retry submitting a transaction before giving up. Some
/// gas modes (e.g. address-balance with no funded balance) or stale consensus
/// inputs can fail deterministically; capping avoids spinning forever.
const MAX_TX_SUBMIT_ATTEMPTS: usize = 10;

type Type = normalized::Type<normalized::ArcIdentifier>;

/// Gas budget for publishing the building-block packages. These packages have
/// grown to many modules, so the budget must be well above the per-move-call
/// publish unit. Kept below the protocol `max_tx_gas` (10 SUI).
const GAS_BUDGET_FOR_PUBLISH: u64 = 5_000_000_000;

#[derive(Debug, Clone)]
pub struct EntryFunction {
    pub package: ObjectID,
    pub module: String,
    pub function: String,
    pub parameters: Vec<Type>,
}

#[derive(Debug, Default)]
pub struct SurfStatistics {
    pub num_successful_transactions: u64,
    pub num_failed_transactions: u64,
    pub num_owned_obj_transactions: u64,
    pub num_shared_obj_transactions: u64,
    /// Transactions that paid gas from the sender's address balance.
    pub num_address_balance_gas_transactions: u64,
    /// Free-tier (price 0) gasless transactions.
    pub num_gasless_transactions: u64,
    /// Transactions that received an object (`ObjectArg::Receiving`).
    pub num_receiving_transactions: u64,
    /// Transactions that used at least one party (consensus-address-owned) input.
    pub num_party_object_transactions: u64,
    pub unique_move_functions_called: HashSet<(ObjectID, String, String)>,
}

impl SurfStatistics {
    #[allow(clippy::too_many_arguments)]
    pub fn record_transaction(
        &mut self,
        has_shared_object: bool,
        tx_succeeded: bool,
        gas_mode: GasMode,
        uses_receiving: bool,
        uses_party: bool,
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
        match gas_mode {
            GasMode::Normal => (),
            GasMode::AddressBalance => self.num_address_balance_gas_transactions += 1,
            GasMode::Gasless => self.num_gasless_transactions += 1,
        }
        if uses_receiving {
            self.num_receiving_transactions += 1;
        }
        if uses_party {
            self.num_party_object_transactions += 1;
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
            result.num_address_balance_gas_transactions +=
                stat.num_address_balance_gas_transactions;
            result.num_gasless_transactions += stat.num_gasless_transactions;
            result.num_receiving_transactions += stat.num_receiving_transactions;
            result.num_party_object_transactions += stat.num_party_object_transactions;
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
            "Feature usage: {} address-balance-gas, {} gasless, {} receiving, {} party-object",
            self.num_address_balance_gas_transactions,
            self.num_gasless_transactions,
            self.num_receiving_transactions,
            self.num_party_object_transactions,
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
pub type SharedObjects = Arc<RwLock<HashMap<StructTag, Vec<ConsensusObjectSequenceKey>>>>;

/// Objects transferred to another object's address, keyed by that address (the
/// parent object's id reinterpreted as an address). These can be received via
/// `ObjectArg::Receiving` when the parent is also an input. Shared across surfer
/// tasks so any account controlling the parent can receive them.
pub type ReceivableObjects =
    Arc<RwLock<HashMap<SuiAddress, HashMap<StructTag, IndexSet<ObjectRef>>>>>;

/// Party objects (`ConsensusAddressOwner`) owned by this account, keyed by type,
/// mapping object id to its current consensus start version (which changes each
/// time the object is transferred).
pub type PartyObjects = HashMap<StructTag, IndexMap<ObjectID, SequenceNumber>>;

pub struct SurferState {
    pub pool: Arc<RwLock<normalized::ArcPool>>,
    pub id: usize,
    pub cluster: Arc<TestCluster>,
    pub rng: StdRng,

    pub address: SuiAddress,
    pub gas_object: ObjectRef,
    pub owned_objects: OwnedObjects,
    pub immutable_objects: ImmObjects,
    pub shared_objects: SharedObjects,
    pub receivable_objects: ReceivableObjects,
    pub party_objects: PartyObjects,
    pub entry_functions: Arc<RwLock<Vec<EntryFunction>>>,

    /// Monotonic nonce providing replay protection for address-balance / gasless
    /// transactions (which carry no gas object to make them unique).
    pub withdraw_nonce: u32,

    pub stats: SurfStatistics,
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
        receivable_objects: ReceivableObjects,
        entry_functions: Arc<RwLock<Vec<EntryFunction>>>,
    ) -> Self {
        Self {
            pool: Arc::new(RwLock::new(normalized::ArcPool::new())),
            id,
            cluster,
            rng,
            address,
            gas_object,
            owned_objects,
            immutable_objects,
            shared_objects,
            receivable_objects,
            party_objects: HashMap::new(),
            entry_functions,
            withdraw_nonce: 0,
            stats: Default::default(),
        }
    }

    #[tracing::instrument(skip_all, fields(surfer_id = self.id))]
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
        let uses_receiving = args
            .iter()
            .any(|arg| matches!(arg, CallArg::Object(ObjectArg::Receiving(_))));
        // Party objects are passed as shared-object inputs; detect whether any of
        // the chosen inputs is one of our tracked party objects.
        let uses_party = args.iter().any(|arg| {
            matches!(arg, CallArg::Object(ObjectArg::SharedObject { id, .. }) if self.is_party_object(id))
        });

        // Build a single-command programmable transaction (exercises the PTB path,
        // and is required for the address-balance / gasless gas modes).
        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            if builder
                .move_call(
                    package,
                    Identifier::new(module.as_str()).unwrap(),
                    Identifier::new(function.as_str()).unwrap(),
                    vec![],
                    args,
                )
                .is_err()
            {
                debug!("Failed to build move call for {module}::{function}");
                return;
            }
            builder.finish()
        };

        let budget = TEST_ONLY_GAS_UNIT_FOR_PUBLISH * rgp;
        let gas_mode = self.choose_gas_mode();
        let tx_data = match gas_mode {
            GasMode::Normal => TransactionData::new_programmable(
                self.address,
                vec![self.gas_object],
                pt,
                budget,
                rgp,
            ),
            GasMode::AddressBalance | GasMode::Gasless => {
                let price = if gas_mode == GasMode::Gasless { 0 } else { rgp };
                let nonce = self.next_withdraw_nonce();
                let epoch = self.current_epoch().await;
                TransactionData::new_programmable_with_address_balance_gas(
                    self.address,
                    pt,
                    budget,
                    price,
                    self.cluster.get_chain_identifier(),
                    epoch,
                    nonce,
                )
            }
        };

        let tx = self.cluster.wallet.sign_transaction(&tx_data).await;
        let mut attempts = 0;
        let response = loop {
            debug!("Executing transaction {:?}", tx.digest());
            match self
                .cluster
                .wallet
                .execute_transaction_may_fail(tx.clone())
                .await
            {
                Ok(response) => break Some(response),
                Err(e) => {
                    attempts += 1;
                    if attempts >= MAX_TX_SUBMIT_ATTEMPTS {
                        error!(
                            "Giving up on transaction {:?} after {attempts} attempts: {e:?}",
                            tx.digest()
                        );
                        break None;
                    }
                    error!("Error executing transaction {:?}: {e:?}", tx.digest());
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        };
        let Some(response) = response else {
            // Could not get the transaction accepted (e.g. a gas mode whose
            // preconditions aren't met). Record it as failed and move on.
            self.stats.record_transaction(
                use_shared_object,
                false,
                gas_mode,
                uses_receiving,
                uses_party,
                package,
                module,
                function,
            );
            return;
        };
        let effects = response.effects;
        info!(
            "[{:?}] Calling Move function {:?}::{:?} (gas: {:?}) returned {:?}",
            self.address,
            module,
            function,
            gas_mode,
            effects.status()
        );
        self.stats.record_transaction(
            use_shared_object,
            effects.status().is_ok(),
            gas_mode,
            uses_receiving,
            uses_party,
            package,
            module,
            function,
        );
        self.process_tx_effects(&effects).await;
    }

    fn choose_gas_mode(&mut self) -> GasMode {
        // Mostly normal gas; occasionally exercise the address-balance and gasless
        // (free-tier) gas paths.
        match self.rng.gen_range(0u8..100) {
            0..=4 => GasMode::Gasless,
            5..=9 => GasMode::AddressBalance,
            _ => GasMode::Normal,
        }
    }

    fn next_withdraw_nonce(&mut self) -> u32 {
        let nonce = self.withdraw_nonce;
        self.withdraw_nonce += 1;
        nonce
    }

    async fn current_epoch(&self) -> u64 {
        self.cluster
            .fullnode_handle
            .sui_node
            .with(|node| node.state().current_epoch_for_testing())
    }

    fn is_party_object(&self, id: &ObjectID) -> bool {
        self.party_objects.values().any(|ids| ids.contains_key(id))
    }

    #[tracing::instrument(skip_all, fields(surfer_id = self.id))]
    async fn process_tx_effects(&mut self, effects: &TransactionEffects) {
        // Drop any deleted/wrapped objects from our inventories first, so we don't
        // later submit transactions referencing stale objects (which the validators
        // would reject, forcing wasteful retries).
        for (obj_ref, _kind) in effects.all_removed_objects() {
            self.forget_object(&obj_ref.0).await;
        }

        for (obj_ref, owner, write_kind) in effects.all_changed_objects() {
            if matches!(owner, Owner::ObjectOwner(_)) {
                // For object owned objects, we don't need to do anything.
                // We also cannot read them because in the case of shared objects, there can be
                // races and the child object may no longer exist.
                continue;
            }
            let Some(object) = self
                .cluster
                .get_object_from_fullnode_store(&obj_ref.0)
                .await
            else {
                continue;
            };
            if object.is_package() {
                self.discover_entry_functions(object).await;
                continue;
            }
            let struct_tag = object.struct_tag().unwrap();
            match owner {
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
                    } else {
                        // Transferred to some other address. If that address turns out
                        // to be a parent object (transfer-to-object), this becomes
                        // receivable via `transfer::receive`.
                        self.receivable_objects
                            .write()
                            .await
                            .entry(address)
                            .or_default()
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
                Owner::ConsensusAddressOwner {
                    start_version,
                    owner: party_owner,
                } => {
                    // Party object. Only the owning account can use it as a consensus
                    // input. Like shared objects, the *input* version is the initial
                    // consensus version and stays constant across later mutations /
                    // re-transfers, so only record it when the object first becomes a
                    // party object (not on Mutate).
                    if party_owner == self.address && write_kind != WriteKind::Mutate {
                        self.party_objects
                            .entry(struct_tag)
                            .or_default()
                            .entry(obj_ref.0)
                            .or_insert(start_version);
                    }
                }
            }
            if obj_ref.0 == self.gas_object.0 {
                self.gas_object = obj_ref;
            }
        }
    }

    /// Remove all references to an object (by id) from every inventory. Used when
    /// an object is deleted or wrapped.
    async fn forget_object(&mut self, id: &ObjectID) {
        for set in self.owned_objects.values_mut() {
            set.retain(|r| &r.0 != id);
        }
        for ids in self.party_objects.values_mut() {
            ids.shift_remove(id);
        }
        {
            let mut receivable = self.receivable_objects.write().await;
            for tags in receivable.values_mut() {
                for set in tags.values_mut() {
                    set.retain(|r| &r.0 != id);
                }
            }
        }
        let mut shared = self.shared_objects.write().await;
        for refs in shared.values_mut() {
            refs.retain(|(oid, _)| oid != id);
        }
    }

    async fn discover_entry_functions(&self, package: Object) {
        let package_id = package.id();
        let move_package = package.into_inner().data.try_into_package().unwrap();
        let proto_version = self.cluster.highest_protocol_version();
        let config = ProtocolConfig::get_for_version(proto_version, Chain::Unknown);
        let binary_config = config.binary_config(None);
        let pool: &mut normalized::ArcPool = &mut *self.pool.write().await;
        let entry_functions: Vec<_> = move_package
            .normalize(pool, &binary_config, /* include code */ false)
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
                        let mut parameters = (*func.parameters).clone();
                        if let Some(last_param) = parameters.last().as_ref()
                            && is_type_tx_context(last_param)
                        {
                            parameters.pop();
                        }
                        Some(EntryFunction {
                            package: package_id,
                            module: module_name.clone(),
                            function: func_name.to_string(),
                            parameters: parameters
                                .into_iter()
                                .map(|rc_ty| (*rc_ty).clone())
                                .collect(),
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

    #[tracing::instrument(skip_all, fields(surfer_id = self.id))]
    pub async fn publish_package(&mut self, path: &Path) {
        let rgp = self.cluster.get_reference_gas_price().await;
        let package = BuildConfig::new_for_testing()
            .build_async(path)
            .await
            .unwrap();
        let modules = package.get_package_bytes(false);
        let tx_data = TransactionData::new_module(
            self.address,
            self.gas_object,
            modules,
            package.dependency_ids.published.values().cloned().collect(),
            // The building-block packages can be large (many modules), so use a
            // generous budget rather than the small per-call publish unit.
            GAS_BUDGET_FOR_PUBLISH,
            rgp,
        );
        let tx = self.cluster.wallet.sign_transaction(&tx_data).await;
        let tx_digest = *tx.digest();
        info!(?tx_digest, "Publishing package");
        let start = Instant::now();
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
                    if start.elapsed() > Duration::from_secs(120) {
                        fatal!(
                            "Failed to publish package after 120 seconds: {} {}",
                            err,
                            tx.digest()
                        );
                    }
                    error!(?tx_digest, "Failed to publish package: {}", err);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        };
        if !response.effects.status().is_ok() {
            fatal!(
                "Failed to publish package {:?}: {:?}",
                path,
                response.effects.status()
            );
        }
        info!("Successfully published package in {:?}", path);
        self.process_tx_effects(&response.effects).await;
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
    ) -> ConsensusObjectSequenceKey {
        self.shared_objects.read().await.get(type_tag).unwrap()[n]
    }

    pub fn matching_party_objects_count(&self, type_tag: &StructTag) -> usize {
        self.party_objects
            .get(type_tag)
            .map(|objects| objects.len())
            .unwrap_or(0)
    }

    pub fn choose_nth_party_object(
        &self,
        type_tag: &StructTag,
        n: usize,
    ) -> ConsensusObjectSequenceKey {
        let (id, version) = self
            .party_objects
            .get(type_tag)
            .unwrap()
            .get_index(n)
            .unwrap();
        (*id, *version)
    }

    /// Take (remove) a receivable child object of the given type that was
    /// transferred to `parent`'s address. Returns `None` if there is none.
    pub async fn take_receivable_object(
        &self,
        parent: &SuiAddress,
        type_tag: &StructTag,
    ) -> Option<ObjectRef> {
        let mut receivable = self.receivable_objects.write().await;
        let set = receivable.get_mut(parent)?.get_mut(type_tag)?;
        let obj_ref = set.iter().next().copied()?;
        set.swap_remove(&obj_ref);
        Some(obj_ref)
    }

    /// Put back a receivable child object that was taken but not used (because the
    /// surrounding transaction could not be assembled).
    pub async fn return_receivable_object(
        &self,
        parent: SuiAddress,
        type_tag: StructTag,
        obj_ref: ObjectRef,
    ) {
        self.receivable_objects
            .write()
            .await
            .entry(parent)
            .or_default()
            .entry(type_tag)
            .or_default()
            .insert(obj_ref);
    }
}

fn is_type_tx_context(ty: &Type) -> bool {
    match ty {
        Type::Reference(_, inner) => match inner.as_ref() {
            Type::Datatype(dt) => {
                dt.module.address == SUI_FRAMEWORK_ADDRESS
                    && dt.module.name.as_ident_str() == IdentStr::new("tx_context").unwrap()
                    && dt.name.as_ident_str() == IdentStr::new("TxContext").unwrap()
                    && dt.type_arguments.is_empty()
            }
            _ => false,
        },
        _ => false,
    }
}
