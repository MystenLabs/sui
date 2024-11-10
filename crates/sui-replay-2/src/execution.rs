// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    environment::{
        is_framework_package, ReplayEnvironment,
    }, 
    errors::ReplayError, replay_txn_data::ReplayTransaction
};
use std::{collections::HashSet, path::PathBuf, sync::Arc};
use move_core_types::{
    account_address::AccountAddress, 
    language_storage::{ModuleId, StructTag}, 
    resolver::{ModuleResolver, ResourceResolver},
};
use sui_execution::Executor;
use sui_types::{
    base_types::{ObjectID, ObjectRef, SequenceNumber, VersionNumber}, 
    committee::EpochId, 
    error::SuiResult, 
    gas::SuiGasStatus, 
    metrics::LimitsMetrics, 
    object::Object, 
    storage::{
        BackingPackageStore, ChildObjectResolver, ObjectStore, PackageObject, ParentSync,
    }, 
    supported_protocol_versions::ProtocolConfig, 
    transaction::CheckedInputObjects,
};
use tracing::info;

pub struct ReplayExecutor {
    protocol_config: ProtocolConfig,
    executor: Arc<dyn Executor + Send + Sync>,
    metrics: Arc<LimitsMetrics>,
}

pub fn execute_transaction_to_effects(
    txn: ReplayTransaction,
    env: &ReplayEnvironment,
) -> Result<(), ReplayError> {
    // TODO: Hook up...
    let certificate_deny_set = HashSet::new();

    let protocol_config = txn.executor.protocol_config;

    let gas_status = if txn.kind.is_system_tx() {
        SuiGasStatus::new_unmetered()
    } else {
        SuiGasStatus::new(
            txn.gas_budget,
            txn.gas_price,
            txn.reference_gas_price,
            &protocol_config,
        )
            .expect("Failed to create gas status")
    };

    let store: ReplayStore<'_> = ReplayStore { env, epoch: txn.epoch };
    let (_inner_store, gas_status, effects, result) =
        txn
            .executor
            .executor
            .execute_transaction_to_effects(
                &store,
                &protocol_config,
                txn.executor.metrics.clone(),
                false, // expensive checks
                &certificate_deny_set,
                &txn.epoch,
                txn.epoch_start_timestamp,
                CheckedInputObjects::new_for_replay(txn.input_objects),
                txn.gas,
                gas_status,
                txn.kind,
                txn.sender,
                txn.digest,
            );
    info!("Transaction executed: {:?}", result);
    info!("Effects: {:?}", effects);
    info!("Gas status: {:?}", gas_status);

    Ok(())
}

impl ReplayExecutor {
    pub fn new(
        protocol_config: ProtocolConfig,
        enable_profiler: Option<PathBuf>,
    ) -> Result<Self, ReplayError> {
        let silent = true; // disable Move debug API
        let executor = sui_execution::executor(&protocol_config, silent, enable_profiler)
            .map_err(|e| ReplayError::ExecutorError { err: format!("{:?}", e) })?;

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
    env: &'a ReplayEnvironment,
    epoch: u64
}

impl BackingPackageStore for ReplayStore<'_> {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        info!("Getting package object for {:?}", package_id);
        if is_framework_package(package_id) {
            let (pkg, txn_digest) = self.env.get_system_package_at_epoch(package_id, self.epoch).unwrap();
            let package = PackageObject::new(Object::new_from_package(pkg, txn_digest));
            Ok(Some(package))
        } else {
            Ok(self
                .env
                .package_objects
                .get(package_id)
                .map(|obj| PackageObject::new(obj.clone()))
            )
        }
    }
}

impl ChildObjectResolver for ReplayStore<'_> {
    fn read_child_object(
        &self,
        parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        todo!(
            "ChildObjectResolver::read_child_object {:?} -> {:?} at {:?}", 
            parent, 
            child,
            child_version_upper_bound,
        )
    }

    fn get_object_received_at_version(
        &self,
        owner: &ObjectID,
        receiving_object_id: &ObjectID,
        receive_object_at_version: SequenceNumber,
        epoch_id: EpochId,
    ) -> SuiResult<Option<Object>> {
        todo!(
            "ChildObjectResolver::get_object_received_at_version owner: {:?}, receiving_object_id: {:?}, receive_object_at_version: {:?}, epoch_id: {:?}",
            owner,
            receiving_object_id,
            receive_object_at_version,
            epoch_id,
        )
    }
}

impl ParentSync for ReplayStore<'_> {
    fn get_latest_parent_entry_ref_deprecated(&self, object_id: ObjectID) -> Option<ObjectRef> {
        todo!(
            "ParentSync::get_latest_parent_entry_ref_deprecated for {:?}",
            object_id,
        )
    }
}

impl ResourceResolver for ReplayStore<'_> {
    type Error = ReplayError;

    fn get_resource(&self, _address: &AccountAddress, _typ: &StructTag) -> Result<Option<Vec<u8>>, Self::Error> {
        todo!("ResourceResolver::get_resource")
    }
}

impl ModuleResolver for ReplayStore<'_> {
    type Error = ReplayError;

    fn get_module(&self, id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        todo!("ModuleResolver::get_module {:?}", id)
    }
}

impl ObjectStore for ReplayStore<'_> {
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        todo!("ObjectStore::get_object {:?}", object_id)
    }

    fn get_object_by_key(
        &self, 
        object_id: &ObjectID, 
        version: VersionNumber,
    ) -> Option<Object> {
        todo!("ObjectStore::get_object {:?} at {}", object_id, version.value())
    }
}
