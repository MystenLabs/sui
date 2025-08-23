// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Execution module for replay.
//! The call to the executor `execute_transaction_to_effects` is here
//! and the logic to call it is pretty straightforward.
//! `execute_transaction_to_effects` requires info from the `EpochStore`
//! (epoch, protocol config, epoch start timestamp, rgp), from the `TransactionStore`
//! as in transaction data and effects, and from the `ObjectStore` for dynamic loads
//! (e.g. dynamic fields).
//! This module also contains the traits used by execution to talk to
//! the store (BackingPackageStore, ObjectStore, ChildObjectResolver)

use crate::{
    replay_interface::{EpochStore, ObjectKey, ObjectStore, VersionQuery},
    replay_txn::{get_input_objects_for_replay, ReplayTransaction},
};
use anyhow::{bail, Context};
use move_core_types::{
    account_address::AccountAddress,
    annotated_value::{MoveTypeLayout, MoveValue},
    annotated_visitor as AV,
    language_storage::ModuleId,
    resolver::ModuleResolver,
};
use move_trace_format::format::MoveTraceBuilder;
use serde_json::json;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, HashSet},
    path::PathBuf,
    sync::Arc,
};
use sui_execution::Executor;
use sui_package_resolver::{Package, PackageStore, Resolver};
use sui_types::{
    balance_change::BalanceChange,
    base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress, VersionNumber},
    committee::EpochId,
    digests::TransactionDigest,
    effects::{TransactionEffects, TransactionEffectsAPI},
    error::{ExecutionError, SuiError, SuiResult},
    execution_params::{get_early_execution_error, BalanceWithdrawStatus, ExecutionOrEarlyError},
    gas::SuiGasStatus,
    gas_coin::GAS,
    id::UID,
    inner_temporary_store::InnerTemporaryStore,
    metrics::LimitsMetrics,
    object::{balance_traversal::BalanceTraversal, Object, Owner},
    storage::{BackingPackageStore, ChildObjectResolver, PackageObject, ParentSync},
    supported_protocol_versions::ProtocolConfig,
    transaction::{CheckedInputObjects, TransactionDataAPI},
    TypeTag,
};
use tracing::{debug, trace};

// Executor for the replay. Created and used by `ReplayTransaction`.
pub struct ReplayExecutor {
    protocol_config: ProtocolConfig,
    executor: Arc<dyn Executor + Send + Sync>,
    metrics: Arc<LimitsMetrics>,
}

// Returned struct from execution. Contains all the data related to a transaction.
// Transaction data and effects (both expected and actual) and the caches containing
// the objects used during execution.
pub struct TxnContextAndEffects {
    pub execution_effects: TransactionEffects, // effects of the replay execution
    pub expected_effects: TransactionEffects,  // expected effects as found in the transaction data
    pub gas_status: SuiGasStatus,              // gas status of the replay execution
    pub object_cache: BTreeMap<ObjectID, BTreeMap<u64, Object>>, // object cache
    pub inner_store: InnerTemporaryStore,      // temporary store used during execution
}

// Entry point. Executes a transaction.
// Return all the information that can be used by a client
// to verify execution.
#[allow(clippy::type_complexity)]
pub fn execute_transaction_to_effects(
    txn: ReplayTransaction,
    epoch_store: &dyn EpochStore,
    object_store: &dyn ObjectStore,
    trace_builder_opt: &mut Option<MoveTraceBuilder>,
) -> Result<
    (
        Result<(), ExecutionError>, // transaction result
        TxnContextAndEffects,       // data touched and changed during execution
    ),
    anyhow::Error,
> {
    debug!("Start execution");
    // TODO: Hook up...
    let config_certificate_deny_set: HashSet<TransactionDigest> = HashSet::new();

    let ReplayTransaction {
        digest,
        checkpoint,
        txn_data,
        effects: expected_effects,
        executor,
        object_cache,
    } = txn;

    let input_objects = get_input_objects_for_replay(&txn_data, &digest, &object_cache)?;
    let protocol_config = &executor.protocol_config;
    let epoch = expected_effects.executed_epoch();
    let epoch_data = epoch_store
        .epoch_info(epoch)?
        .ok_or_else(|| anyhow::anyhow!(format!("Epoch {} not found", epoch)))?;
    let epoch_start_timestamp = epoch_data.start_timestamp;
    let gas_status = if txn_data.kind().is_system_tx() {
        SuiGasStatus::new_unmetered()
    } else {
        SuiGasStatus::new(
            txn_data.gas_data().budget,
            txn_data.gas_data().price,
            epoch_data.rgp,
            protocol_config,
        )
        .expect("Failed to create gas status")
    };
    let store: ReplayStore<'_> = ReplayStore {
        checkpoint,
        store: object_store,
        object_cache: RefCell::new(object_cache),
    };
    let input_objects = CheckedInputObjects::new_for_replay(input_objects);
    // TODO(address-balances): Get withdraw status from effects.
    let early_execution_error = get_early_execution_error(
        &digest,
        &input_objects,
        &config_certificate_deny_set,
        // TODO(address-balances): Support balance withdraw status for replay
        &BalanceWithdrawStatus::NoWithdraw,
    );
    let execution_params = match early_execution_error {
        Some(error) => ExecutionOrEarlyError::Err(error),
        None => ExecutionOrEarlyError::Ok(()),
    };
    let (inner_store, gas_status, effects, _execution_timing, result) =
        executor.executor.execute_transaction_to_effects(
            &store,
            protocol_config,
            executor.metrics.clone(),
            false, // expensive checks
            execution_params,
            &epoch,
            epoch_start_timestamp,
            input_objects,
            txn_data.gas_data().clone(),
            gas_status,
            txn_data.kind().clone(),
            txn_data.sender(),
            digest,
            trace_builder_opt,
        );
    let ReplayStore {
        object_cache,
        checkpoint: _,
        store: _,
    } = store;
    let object_cache = object_cache.into_inner();
    debug!("End execution");

    let mut reads = BTreeMap::new();
    for change in effects.object_changes() {
        let Some(input) = change.input_version else {
            continue;
        };

        reads.insert(
            change.id,
            object_cache
                .get(&change.id)
                .and_then(|vs| vs.get(&input.value()).cloned())
                .context("Input object not in cache")?,
        );
    }

    for (id, meta) in &inner_store.loaded_runtime_objects {
        reads.insert(
            *id,
            object_cache
                .get(&id)
                .and_then(|vs| vs.get(&meta.version.value()).cloned())
                .context("Runtime loaded object not in cache")?,
        );
    }

    #[allow(clippy::disallowed_methods)]
    let mut address_balance_changes = futures::executor::block_on(balance_changes(
        &reads,
        &inner_store.written,
        ObjectCache(object_cache.clone()),
    ))
    .context("Failed to compute balance changes")?;

    address_balance_changes.push(BalanceChange {
        address: SuiAddress::ZERO,
        coin_type: GAS::type_().into(),
        amount: effects.gas_cost_summary().net_gas_usage() as i128,
    });

    let mut balance_changes = BTreeMap::new();
    for change in &address_balance_changes {
        *balance_changes.entry(change.coin_type.clone()).or_insert(0) += change.amount;
    }

    for (coin_type, amount) in balance_changes {
        if amount != 0 {
            debug!(
                "{} not conserved: {amount}",
                coin_type.to_canonical_display(true)
            );
        }
    }

    debug!("");
    debug!(
        "Balance Changes: {}",
        serde_json::to_string_pretty(
            &address_balance_changes
                .into_iter()
                .map(|change| json!({
                    "address": change.address,
                    "coin_type": change.coin_type.to_canonical_string(true),
                    "amount": change.amount,
                }))
                .collect::<Vec<_>>()
        )
        .unwrap()
    );

    Ok((
        result,
        TxnContextAndEffects {
            execution_effects: effects,
            expected_effects,
            gas_status,
            object_cache,
            inner_store,
        },
    ))
}

impl ReplayExecutor {
    pub fn new(
        protocol_config: ProtocolConfig,
        enable_profiler: Option<PathBuf>,
    ) -> Result<Self, anyhow::Error> {
        let silent = true; // disable Move debug API
        let executor = sui_execution::executor(&protocol_config, silent, enable_profiler)
            .context("Filed to create executor. ProtocolConfig inconsistency?")?;

        let registry = prometheus::Registry::new();
        let metrics = Arc::new(LimitsMetrics::new(&registry));

        Ok(Self {
            protocol_config,
            executor,
            metrics,
        })
    }
}

//
// Execution traits implementation for ReplayEnvironment
//

struct ReplayStore<'a> {
    store: &'a dyn ObjectStore,
    object_cache: RefCell<BTreeMap<ObjectID, BTreeMap<u64, Object>>>,
    checkpoint: u64,
}

impl ReplayStore<'_> {
    // utility function shared across traits functions
    fn get_object_at_version(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Option<Object> {
        // look up in the cache
        if let Some(object) = self
            .object_cache
            .borrow()
            .get(object_id)
            .and_then(|versions| versions.get(&version.value()).cloned())
        {
            return Some(object);
        }

        // if not in the cache fetch it from the store
        let object = self
            .store
            .get_objects(&[ObjectKey {
                object_id: *object_id,
                version_query: VersionQuery::Version(version.value()),
            }])
            .map_err(|e| SuiError::Storage(e.to_string()))
            .ok()?
            .into_iter()
            .next()?
            .map(|(obj, _version)| obj);
        // add it to the cache
        if let Some(obj) = &object {
            self.object_cache
                .borrow_mut()
                .entry(obj.id())
                .or_default()
                .insert(obj.version().value(), obj.clone());
        }

        object
    }
}

impl BackingPackageStore for ReplayStore<'_> {
    // Look for a package in the object cache first.
    // If not found, fetch it from the store, add to the cache, and return it.
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        trace!("get_package_object({})", package_id);
        if let Some(versions) = self.object_cache.borrow().get(package_id) {
            debug_assert!(
                versions.len() == 1,
                "Expected only one version in cache for package object {}",
                package_id
            );
            return Ok(Some(PackageObject::new(
                versions.values().next().unwrap().clone(),
            )));
        }
        // If the package is not in the cache, fetch it from the store
        let object_key = ObjectKey {
            object_id: *package_id,
            // Using AtCheckpoint to get the package version at the time of execution
            version_query: VersionQuery::AtCheckpoint(self.checkpoint),
        };
        let package = self
            .store
            .get_objects(&[object_key])
            .map_err(|e| SuiError::Storage(e.to_string()))?;
        debug_assert!(
            package.len() == 1,
            "Expected one package object for {}",
            package_id
        );
        let (package, _version) = package.into_iter().next().unwrap().unwrap();

        self.object_cache
            .borrow_mut()
            .entry(*package_id)
            .or_default()
            .insert(package.version().value(), package.clone());

        Ok(Some(PackageObject::new(package)))
    }
}

impl sui_types::storage::ObjectStore for ReplayStore<'_> {
    // Get an object by its ID. This translates to a query for the object
    // at the checkpoint (mimic latest runtime behavior)
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        trace!("get_object({})", object_id);
        let object = match self.object_cache.borrow().get(object_id) {
            Some(versions) => versions.last_key_value().map(|(_version, obj)| obj.clone()),
            None => self
                .store
                .get_objects(&[ObjectKey {
                    object_id: *object_id,
                    version_query: VersionQuery::AtCheckpoint(self.checkpoint),
                }])
                .map_err(|e| SuiError::Storage(e.to_string()))
                .ok()?
                .into_iter()
                .next()?
                .map(|(obj, _version)| obj),
        };

        if let Some(obj) = &object {
            self.object_cache
                .borrow_mut()
                .entry(obj.id())
                .or_default()
                .insert(obj.version().value(), obj.clone());
        }

        object
    }

    // Get an object by its ID and version
    fn get_object_by_key(&self, object_id: &ObjectID, version: VersionNumber) -> Option<Object> {
        trace!("get_object_by_key({}, {})", object_id, version);
        self.get_object_at_version(object_id, version)
    }
}

impl ChildObjectResolver for ReplayStore<'_> {
    // Load an `Object` at a root version. That is the version that is
    // less than or equal to the given `child_version_upper_bound`.
    fn read_child_object(
        &self,
        _parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        trace!(
            "read_child_object({}, {}, {})",
            _parent,
            child,
            child_version_upper_bound,
        );
        let object_key = ObjectKey {
            object_id: *child,
            version_query: VersionQuery::RootVersion(child_version_upper_bound.value()),
        };
        let object = self
            .store
            .get_objects(&[object_key])
            .map_err(|e| SuiError::Storage(e.to_string()))?;
        debug_assert!(object.len() == 1, "Expected one object for {}", child,);
        let object = object
            .into_iter()
            .next()
            .unwrap()
            .map(|(obj, _version)| obj);

        if let Some(obj) = &object {
            self.object_cache
                .borrow_mut()
                .entry(obj.id())
                .or_default()
                .insert(obj.version().value(), obj.clone());
        }

        Ok(object)
    }

    // Load a receiving object. Results in a query at a specific version
    // (`receive_object_at_version`).
    fn get_object_received_at_version(
        &self,
        owner: &ObjectID,
        receiving_object_id: &ObjectID,
        receive_object_at_version: SequenceNumber,
        epoch_id: EpochId,
    ) -> SuiResult<Option<Object>> {
        trace!(
            "get_object_received_at_version({}, {}, {}, {})",
            owner,
            receiving_object_id,
            receive_object_at_version,
            epoch_id
        );
        Ok(self.get_object_at_version(receiving_object_id, receive_object_at_version))
    }
}

//
// unreachable traits
//

impl ParentSync for ReplayStore<'_> {
    fn get_latest_parent_entry_ref_deprecated(&self, object_id: ObjectID) -> Option<ObjectRef> {
        unreachable!(
            "unexpected ParentSync::get_latest_parent_entry_ref_deprecated({})",
            object_id,
        )
    }
}

impl ModuleResolver for ReplayStore<'_> {
    type Error = anyhow::Error;

    fn get_module(&self, id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        unreachable!("unexpected ModuleResolver::get_module({})", id)
    }
}

struct ObjectCache(BTreeMap<ObjectID, BTreeMap<u64, Object>>);

async fn balance_changes(
    read: &BTreeMap<ObjectID, Object>,
    written: &BTreeMap<ObjectID, Object>,
    cache: ObjectCache,
) -> anyhow::Result<Vec<BalanceChange>> {
    let package_resolver = Resolver::new(cache);

    let balance_in = root_balances(read, &package_resolver).await?;

    let mut write_set = read.clone();
    write_set.extend(written.clone());
    let balance_out = root_balances(&write_set, &package_resolver).await?;

    let mut balance_changes = BTreeMap::new();
    for ((address, coin_type), amount) in balance_out {
        *balance_changes.entry((address, coin_type)).or_insert(0) += amount;
    }

    for ((address, coin_type), amount) in balance_in {
        *balance_changes.entry((address, coin_type)).or_insert(0) -= amount;
    }

    Ok(balance_changes
        .into_iter()
        .map(|((address, coin_type), amount)| BalanceChange {
            address,
            coin_type,
            amount,
        })
        .collect())
}

async fn root_balances(
    working_set: &BTreeMap<ObjectID, Object>,
    resolver: &Resolver<ObjectCache>,
) -> anyhow::Result<BTreeMap<(SuiAddress, TypeTag), i128>> {
    // Traverse each object to find the UIDs and balances it wraps.
    let mut wrapper = BTreeMap::new();
    let mut object_balances = BTreeMap::new();
    for (id, obj) in working_set {
        let Some(obj) = obj.data.try_as_move() else {
            continue;
        };

        let layout = resolver.type_layout(obj.type_().clone().into()).await?;
        object_balances.insert(*id, balances(&layout, obj.contents())?);

        for child in wrapped_uids(&layout, obj.contents())? {
            wrapper.insert(child, *id);
        }
    }

    // Work back through wrappers to associate each object with a root owning address. This walks
    // back through parent -> child object relationships (dynamic fields) until it finds an
    // address-owner, or a shared or immutable object.
    let mut root_owners = BTreeMap::new();
    for (id, mut obj) in working_set {
        let mut curr = *id;
        loop {
            match obj.owner() {
                Owner::AddressOwner(owner) | Owner::ConsensusAddressOwner { owner, .. } => {
                    root_owners.insert(*id, *owner);
                    break;
                }

                Owner::Immutable | Owner::Shared { .. } => {
                    root_owners.insert(*id, curr.into());
                    break;
                }

                Owner::ObjectOwner(address) => {
                    let mut next = ObjectID::from(*address);
                    if let Some(parent) = wrapper.get(&next) {
                        next = *parent;
                    }

                    let Some(next_obj) = working_set.get(&next) else {
                        bail!("Cannot find owner of {curr} in the working set: {next}");
                    };

                    curr = next;
                    obj = next_obj;
                }
            }
        }
    }

    // Accumulate balance changes to root owners.
    let mut balances = BTreeMap::new();
    for (id, obj_balances) in object_balances {
        let Some(root) = root_owners.get(&id) else {
            bail!("Cannot find root owner of {id} in the working set");
        };

        for (coin_type, amount) in obj_balances {
            *balances.entry((*root, coin_type)).or_insert(0) += amount as i128;
        }
    }

    Ok(balances)
}

fn balances(layout: &MoveTypeLayout, contents: &[u8]) -> anyhow::Result<BTreeMap<TypeTag, u64>> {
    let mut visitor = BalanceTraversal::default();
    MoveValue::visit_deserialize(contents, layout, &mut visitor)?;
    Ok(visitor.finish())
}

fn wrapped_uids(layout: &MoveTypeLayout, contents: &[u8]) -> anyhow::Result<BTreeSet<ObjectID>> {
    let mut ids = BTreeSet::new();
    struct UIDTraversal<'i>(&'i mut BTreeSet<ObjectID>);
    struct UIDCollector<'i>(&'i mut BTreeSet<ObjectID>);

    impl<'b, 'l> AV::Traversal<'b, 'l> for UIDTraversal<'_> {
        type Error = AV::Error;

        fn traverse_struct(
            &mut self,
            driver: &mut AV::StructDriver<'_, 'b, 'l>,
        ) -> Result<(), Self::Error> {
            if driver.struct_layout().type_ == UID::type_() {
                while driver.next_field(&mut UIDCollector(self.0))?.is_some() {}
            } else {
                while driver.next_field(self)?.is_some() {}
            }
            Ok(())
        }
    }

    impl<'b, 'l> AV::Traversal<'b, 'l> for UIDCollector<'_> {
        type Error = AV::Error;
        fn traverse_address(
            &mut self,
            _driver: &AV::ValueDriver<'_, 'b, 'l>,
            value: AccountAddress,
        ) -> Result<(), Self::Error> {
            self.0.insert(value.into());
            Ok(())
        }
    }

    MoveValue::visit_deserialize(contents, layout, &mut UIDTraversal(&mut ids))?;
    Ok(ids)
}

#[async_trait::async_trait]
impl PackageStore for ObjectCache {
    /// Read package contents. Fails if `id` is not an object, not a package, or is malformed in
    /// some way.
    async fn fetch(&self, id: AccountAddress) -> sui_package_resolver::Result<Arc<Package>> {
        let object_id = ObjectID::from(id);
        // HACK: assumes packages will always be found in the cache.
        let versions = self
            .0
            .get(&object_id)
            .unwrap_or_else(|| panic!("Failed to find package {object_id} in the object cache"));

        let (_, obj) = versions.last_key_value().unwrap();

        Ok(Arc::new(Package::read_from_object(obj)?))
    }
}
