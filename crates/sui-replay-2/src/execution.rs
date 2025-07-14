// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Execution module for replay.
//! The call to the executor `execute_transaction_to_effects` is here
//! and the logic to call it is pretty straightforward.
//! `execute_transaction_to_effects` requires info from the `EpochStore`
//! (epoch, protocol config, epoch start timestamp, rgp), from the `TransactionStore`
//! as in transaction data and effects, and from the `ObjectStore` for dynamic loads
//! (e.g. synamic fields).
//! This module also contains the traits used by execution to talk to
//! the store (BackingPackageStore, ObjectStore, ChildObjectResolver)

use crate::{
    replay_interface::{EpochStore, ObjectKey, ObjectStore, VersionQuery},
    replay_txn::{get_input_objects_for_replay, ReplayTransaction},
};
use anyhow::Context;
use move_core_types::{language_storage::ModuleId, resolver::ModuleResolver};
use move_trace_format::format::MoveTraceBuilder;
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashSet},
    path::PathBuf,
    sync::Arc,
};
use sui_execution::Executor;
use sui_types::{
    base_types::{ObjectID, ObjectRef, SequenceNumber, VersionNumber},
    committee::EpochId,
    digests::TransactionDigest,
    effects::{TransactionEffects, TransactionEffectsAPI},
    error::{ExecutionError, SuiError, SuiResult},
    gas::SuiGasStatus,
    inner_temporary_store::InnerTemporaryStore,
    metrics::LimitsMetrics,
    object::Object,
    storage::{BackingPackageStore, ChildObjectResolver, PackageObject, ParentSync},
    supported_protocol_versions::ProtocolConfig,
    transaction::{CheckedInputObjects, TransactionDataAPI},
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
    let certificate_deny_set: HashSet<TransactionDigest> = HashSet::new();

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
    let epoch_data = epoch_store.epoch_info(epoch)?;
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
    let (inner_store, gas_status, effects, _execution_timing, result) =
        executor.executor.execute_transaction_to_effects(
            &store,
            protocol_config,
            executor.metrics.clone(),
            false, // expensive checks
            &certificate_deny_set,
            &epoch,
            epoch_start_timestamp,
            CheckedInputObjects::new_for_replay(input_objects),
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
            .next()?;
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
            // we could have used `VersionQuery::ImmutableOrLatest`
            // but we would have to track system packages separetly
            // which we may want to consider
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
        let package = package.into_iter().next().unwrap().unwrap();
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
                .next()?,
        };
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
        let object = object.into_iter().next().unwrap();
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
