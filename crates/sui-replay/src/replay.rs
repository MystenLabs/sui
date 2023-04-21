// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::executor::block_on;
use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::account_address::AccountAddress;
use move_core_types::language_storage::{ModuleId, StructTag};
use move_core_types::parser::parse_struct_tag;
use move_core_types::resolver::{ModuleResolver, ResourceResolver};
use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use sui_adapter::adapter;
use sui_adapter::execution_engine::execute_transaction_to_effects_impl;
use sui_adapter::execution_mode;
use sui_config::node::ExpensiveSafetyCheckConfig;
use sui_core::authority::TemporaryStore;
use sui_framework::BuiltInFramework;
use sui_json_rpc_types::{
    EventFilter, SuiEvent, SuiGetPastObjectRequest, SuiObjectData, SuiObjectDataOptions,
    SuiObjectRef, SuiPastObjectResponse, SuiTransactionBlockEffectsV1,
    SuiTransactionBlockResponseOptions,
};
use sui_json_rpc_types::{
    SuiObjectResponse, SuiTransactionBlockEffects, SuiTransactionBlockEffectsAPI,
};
use sui_protocol_config::ProtocolConfig;
use sui_sdk::error::Error as SuiRpcError;
use sui_sdk::{SuiClient, SuiClientBuilder};
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress, VersionNumber};
use sui_types::committee::EpochId;
use sui_types::digests::{ObjectDigest, TransactionDigest};
use sui_types::error::{SuiError, SuiObjectResponseError, SuiResult, UserInputError};
use sui_types::gas::SuiGasStatus;
use sui_types::messages::{InputObjectKind, InputObjects, TransactionKind};
use sui_types::messages::{SenderSignedData, TransactionDataAPI};
use sui_types::metrics::LimitsMetrics;
use sui_types::object::{Data, Object, Owner};
use sui_types::storage::get_module_by_id;
use sui_types::storage::{BackingPackageStore, ChildObjectResolver, ObjectStore, ParentSync};
use sui_types::DEEPBOOK_OBJECT_ID;
use thiserror::Error;
use tracing::{error, warn};

// TODO: add persistent cache. But perf is good enough already.
// TODO: handle safe mode

// These are very testnet specific
const GENESIX_TX_DIGEST: &str = "Cgww1sn7XViCPSdDcAPmVcARueWuexJ8af8zD842Ff43";
const SAFE_MODETX_1_DIGEST: &str = "AGBCaUGj4iGpGYyQvto9Bke1EwouY8LGMoTzzuPMx4nd";

const EPOCH_CHANGE_STRUCT_TAG: &str = "0x3::sui_system_state_inner::SystemEpochInfoEvent";

pub struct LocalExec {
    pub client: SuiClient,
    // For a given protocol version, what TX created it, and what is the valid range of epochs
    // at this protocol version.
    pub protocol_version_epoch_table: BTreeMap<u64, (TransactionDigest, u64, u64)>,
    // For a given protocol version, the mapping valid sequence numbers for each framework package
    pub protocol_version_system_package_table: BTreeMap<u64, BTreeMap<ObjectID, SequenceNumber>>,

    pub current_protocol_version: u64,

    pub store: BTreeMap<ObjectID, Object>,
    pub package_cache: Arc<Mutex<BTreeMap<ObjectID, Object>>>,
    pub object_version_cache: Arc<Mutex<BTreeMap<(ObjectID, SequenceNumber), Object>>>,

    pub exec_store_events: Arc<Mutex<Vec<ExecutionStoreEvent>>>,

    pub metrics: Arc<LimitsMetrics>,
}

#[derive(Clone, Debug)]
struct TxInfo {
    pub sender: SuiAddress,
    pub input_objects: Vec<InputObjectKind>,
    pub kind: TransactionKind,
    pub modified_at_versions: Vec<(ObjectID, SequenceNumber)>,
    pub shared_object_refs: Vec<SuiObjectRef>,
    pub gas: Vec<(ObjectID, SequenceNumber, ObjectDigest)>,
    pub gas_budget: u64,
    pub gas_price: u64,
    pub executed_epoch: u64,
    pub dependencies: Vec<TransactionDigest>,
    pub effects: SuiTransactionBlockEffectsV1,
    pub protocol_config: ProtocolConfig,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Serialize, Deserialize, Error, Clone)]
pub enum LocalExecError {
    #[error("SuiError: {:#?}", err)]
    SuiError { err: SuiError },

    #[error("SuiRpcError: {:#?}", err)]
    SuiRpcError { err: String },

    #[error("SuiObjectResponseError: {:#?}", err)]
    SuiObjectResponseError { err: SuiObjectResponseError },

    #[error("UserInputError: {:#?}", err)]
    UserInputError { err: UserInputError },

    #[error("GeneralError: {:#?}", err)]
    GeneralError { err: String },

    #[error("ObjectNotExist: {:#?}", id)]
    ObjectNotExist { id: ObjectID },

    #[error("ObjectVersionNotFound: {:#?} version {}", id, version)]
    ObjectVersionNotFound {
        id: ObjectID,
        version: SequenceNumber,
    },

    #[error(
        "ObjectVersionTooHigh: {:#?}, requested version {}, latest version found {}",
        id,
        asked_version,
        latest_version
    )]
    ObjectVersionTooHigh {
        id: ObjectID,
        asked_version: SequenceNumber,
        latest_version: SequenceNumber,
    },

    #[error(
        "ObjectDeleted: {:#?} at version {:#?} digest {:#?}",
        id,
        version,
        digest
    )]
    ObjectDeleted {
        id: ObjectID,
        version: SequenceNumber,
        digest: ObjectDigest,
    },

    #[error(
        "EffectsForked: Effects for digest {} forked with diff {}",
        digest,
        diff
    )]
    EffectsForked {
        digest: TransactionDigest,
        diff: String,
        on_chain: Box<SuiTransactionBlockEffectsV1>,
        local: Box<SuiTransactionBlockEffectsV1>,
    },

    #[error("Genesis replay not supported digest {:#?}", digest)]
    GenesisReplayNotSupported { digest: TransactionDigest },
}

impl From<SuiObjectResponseError> for LocalExecError {
    fn from(err: SuiObjectResponseError) -> Self {
        match err {
            SuiObjectResponseError::NotExists { object_id } => {
                LocalExecError::ObjectNotExist { id: object_id }
            }
            SuiObjectResponseError::Deleted {
                object_id,
                digest,
                version,
            } => LocalExecError::ObjectDeleted {
                id: object_id,
                version,
                digest,
            },
            _ => LocalExecError::SuiObjectResponseError { err },
        }
    }
}

fn convert_past_obj_response(resp: SuiPastObjectResponse) -> Result<Object, LocalExecError> {
    match resp {
        SuiPastObjectResponse::VersionFound(o) => obj_from_sui_obj_data(&o),
        SuiPastObjectResponse::ObjectDeleted(r) => Err(LocalExecError::ObjectDeleted {
            id: r.object_id,
            version: r.version,
            digest: r.digest,
        }),
        SuiPastObjectResponse::ObjectNotExists(id) => Err(LocalExecError::ObjectNotExist { id }),
        SuiPastObjectResponse::VersionNotFound(id, version) => {
            Err(LocalExecError::ObjectVersionNotFound { id, version })
        }
        SuiPastObjectResponse::VersionTooHigh {
            object_id,
            asked_version,
            latest_version,
        } => Err(LocalExecError::ObjectVersionTooHigh {
            id: object_id,
            asked_version,
            latest_version,
        }),
    }
}

impl From<LocalExecError> for SuiError {
    fn from(err: LocalExecError) -> Self {
        SuiError::Unknown(format!("{:#?}", err))
    }
}

impl From<SuiError> for LocalExecError {
    fn from(err: SuiError) -> Self {
        LocalExecError::SuiError { err }
    }
}
impl From<SuiRpcError> for LocalExecError {
    fn from(err: SuiRpcError) -> Self {
        LocalExecError::SuiRpcError {
            err: format!("{:#?}", err),
        }
    }
}

impl From<UserInputError> for LocalExecError {
    fn from(err: UserInputError) -> Self {
        LocalExecError::UserInputError { err }
    }
}

impl From<anyhow::Error> for LocalExecError {
    fn from(err: anyhow::Error) -> Self {
        LocalExecError::GeneralError {
            err: format!("{:#?}", err),
        }
    }
}

impl LocalExec {
    pub async fn new_from_fn_url(http_url: &str) -> Result<Self, LocalExecError> {
        Ok(Self::new(
            SuiClientBuilder::default().build(http_url).await?,
        ))
    }

    pub async fn init_for_execution(mut self) -> Result<Self, LocalExecError> {
        self.populate_protocol_version_tables().await?;
        Ok(self)
    }

    pub fn new(client: SuiClient) -> Self {
        // Use a throwaway metrics registry for local execution.
        let registry = prometheus::Registry::new();
        let metrics = Arc::new(LimitsMetrics::new(&registry));
        Self {
            client,
            store: BTreeMap::new(),
            package_cache: Arc::new(Mutex::new(BTreeMap::new())),
            object_version_cache: Arc::new(Mutex::new(BTreeMap::new())),
            protocol_version_epoch_table: BTreeMap::new(),
            protocol_version_system_package_table: BTreeMap::new(),
            current_protocol_version: 0,
            exec_store_events: Arc::new(Mutex::new(Vec::new())),
            metrics,
        }
    }

    #[allow(clippy::wrong_self_convention)]
    pub fn to_temporary_store(
        &mut self,
        tx_digest: &TransactionDigest,
        input_objects: InputObjects,
        protocol_config: &ProtocolConfig,
    ) -> TemporaryStore<&mut LocalExec> {
        TemporaryStore::new(self, input_objects, *tx_digest, protocol_config)
    }

    pub async fn multi_download_and_store_sui_obj_ref(
        &mut self,
        refs: &[SuiObjectRef],
    ) -> Result<Vec<Object>, LocalExecError> {
        let sub_refs: Vec<_> = refs.iter().map(|r| (r.object_id, r.version)).collect();
        self.multi_download_and_store(&sub_refs).await
    }

    pub async fn multi_download(
        &self,
        objs: &[(ObjectID, SequenceNumber)],
    ) -> Result<Vec<Object>, LocalExecError> {
        let objs: Vec<_> = objs
            .iter()
            .map(|(object_id, version)| SuiGetPastObjectRequest {
                object_id: *object_id,
                version: *version,
            })
            .collect();
        let options = SuiObjectDataOptions::bcs_lossless();
        let objects = self
            .client
            .read_api()
            .try_multi_get_parsed_past_object(objs, options)
            .await
            .map_err(|q| LocalExecError::SuiRpcError { err: q.to_string() })?;

        let objects: Vec<_> = objects
            .iter()
            .map(|o| convert_past_obj_response(o.clone()))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(objects)
    }

    pub async fn multi_download_and_store(
        &mut self,
        objs: &[(ObjectID, SequenceNumber)],
    ) -> Result<Vec<Object>, LocalExecError> {
        let objs = self.multi_download(objs).await?;
        for obj in objs.iter() {
            let o_ref = obj.compute_object_reference();
            self.store.insert(o_ref.0, obj.clone());
            self.object_version_cache
                .lock()
                .expect("Cannot lock")
                .insert((o_ref.0, o_ref.1), obj.clone());
            if obj.is_package() {
                self.package_cache
                    .lock()
                    .expect("Cannot lock")
                    .insert(o_ref.0, obj.clone());
            }
        }
        Ok(objs)
    }

    pub async fn multi_download_relevant_packages_and_store(
        &mut self,
        objs: Vec<ObjectID>,
        protocol_version: u64,
    ) -> Result<Vec<Object>, LocalExecError> {
        let syst_packages = self.system_package_versions_for_epoch(protocol_version)?;
        let syst_packages_objs = self.multi_download(&syst_packages).await?;

        // Download latest version of all packages that are not system packages
        // This is okay since the versions can never change
        let non_system_package_objs: Vec<_> = objs
            .into_iter()
            .filter(|o| !self.system_package_ids().contains(o))
            .collect();
        let objs = self
            .multi_download_latest(non_system_package_objs)
            .await?
            .into_iter()
            .chain(syst_packages_objs.into_iter());

        for obj in objs.clone() {
            let o_ref = obj.compute_object_reference();
            // We dont always want the latest in store
            //self.store.insert(o_ref.0, obj.clone());
            self.object_version_cache
                .lock()
                .expect("Cannot lock")
                .insert((o_ref.0, o_ref.1), obj.clone());
            if obj.is_package() {
                self.package_cache
                    .lock()
                    .expect("Cannot lock")
                    .insert(o_ref.0, obj.clone());
            }
        }
        Ok(objs.collect())
    }

    pub async fn multi_download_latest(
        &self,
        objs: Vec<ObjectID>,
    ) -> Result<Vec<Object>, LocalExecError> {
        let options = SuiObjectDataOptions::bcs_lossless();
        let objects = self
            .client
            .read_api()
            .multi_get_object_with_options(objs, options)
            .await
            .map_err(|q| LocalExecError::SuiRpcError { err: q.to_string() })?;

        let objects: Vec<_> = objects
            .iter()
            .map(obj_from_sui_obj_response)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(objects)
    }

    // TODO: remove this after `futures::executor::block_on` is removed.
    #[allow(clippy::disallowed_methods)]
    pub fn download_object(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
    ) -> Result<Object, LocalExecError> {
        println!("Downloading object {} {}", object_id, version);
        if self
            .object_version_cache
            .lock()
            .expect("Cannot lock")
            .contains_key(&(*object_id, version))
        {
            return Ok(self
                .object_version_cache
                .lock()
                .expect("Cannot lock")
                .get(&(*object_id, version))
                .ok_or(LocalExecError::GeneralError {
                    err: format!("Object not found in cache {} {}", object_id, version),
                })?
                .clone());
        }

        let options = SuiObjectDataOptions::bcs_lossless();
        // TODO: replace use of `block_on`
        let object = block_on({
            self.client
                .read_api()
                .try_get_parsed_past_object(*object_id, version, options)
        })
        .map_err(|q| LocalExecError::SuiRpcError { err: q.to_string() })?;

        let o = convert_past_obj_response(object)?;
        let o_ref = o.compute_object_reference();
        self.object_version_cache
            .lock()
            .expect("Cannot lock")
            .insert((o_ref.0, o_ref.1), o.clone());
        Ok(o)
    }

    // TODO: remove this after `futures::executor::block_on` is removed.
    #[allow(clippy::disallowed_methods)]
    pub fn download_latest_object(
        &self,
        object_id: &ObjectID,
    ) -> Result<Option<Object>, LocalExecError> {
        block_on({
            //info!("Downloading latest object {object_id}");
            self.download_latest_object_impl(object_id)
        })
    }

    pub async fn download_latest_object_impl(
        &self,
        object_id: &ObjectID,
    ) -> Result<Option<Object>, LocalExecError> {
        let options = SuiObjectDataOptions::bcs_lossless();

        self
        .client
        .read_api()
        .get_object_with_options(*object_id, options)
        .await.map(|q| match obj_from_sui_obj_response(&q){
            Ok(v) => Ok(Some(v)),
            Err(LocalExecError::ObjectNotExist { id }) => {
                error!("Could not find object {id} on RPC server. It might have been pruned, deleted, or never existed.");
                Ok(None)
            }
            Err(LocalExecError::ObjectDeleted { id, version, digest }) => {
                error!("Object {id} {version} {digest} was deleted on RPC server.");
                Ok(None)
            },
            Err(err) => Err(LocalExecError::SuiRpcError {
                err: err.to_string(),
            })
        })?
    }

    pub async fn execute_all_in_checkpoint(
        &mut self,
        checkpoint_id: u64,
        expensive_safety_check_config: ExpensiveSafetyCheckConfig,
        terminate_early: bool,
    ) -> Result<u64, LocalExecError> {
        // Get all the TXs at this checkpoint
        let checkp = self
            .client
            .read_api()
            .get_checkpoint(checkpoint_id.into())
            .await?;
        let num = checkp.transactions.len();
        for tx in checkp.transactions {
            let status = self
                .execute(&tx, expensive_safety_check_config.clone())
                .await;
            if status.is_err() {
                if terminate_early {
                    return Err(status.err().unwrap());
                }
                error!("Error executing tx: {},  {:#?}", tx, status);
                continue;
            }
        }
        Ok(num as u64)
    }

    /// Should be called after `init_for_execution`
    pub async fn execute(
        &mut self,
        tx_digest: &TransactionDigest,
        expensive_safety_check_config: ExpensiveSafetyCheckConfig,
    ) -> Result<SuiTransactionBlockEffectsV1, LocalExecError> {
        let tx_info = self.resolve_tx_components(tx_digest).await?;

        // We need this for other activities in this session
        self.current_protocol_version = tx_info.protocol_config.version.as_u64();
        // A lot of the logic here isnt designed for genesis
        if tx_info.sender == SuiAddress::ZERO {
            // Genesis.
            return Err(LocalExecError::GenesisReplayNotSupported { digest: *tx_digest });
        }

        // Download the objects at the version right before the execution of this TX
        self.multi_download_and_store(&tx_info.modified_at_versions)
            .await?;

        // Download shared objects at the version right before the execution of this TX
        let shared_obj_refs = &tx_info.shared_object_refs;

        self.multi_download_and_store_sui_obj_ref(shared_obj_refs)
            .await?;

        // Download gas (although this should already be in cache from modified at versions?)
        let gas_refs: Vec<_> = tx_info.gas.iter().map(|w| (w.0, w.1)).collect();
        self.multi_download_and_store(&gas_refs).await?;

        // This assumes we already initialized the protocol version table `protocol_version_epoch_table`
        let protocol_config = &tx_info.protocol_config;

        let metrics = self.metrics.clone();

        // Resolve and download the input objects
        let input_objects = self.resolve_download_input_objects(&tx_info).await?;

        // Prep the object runtime for dynamic fields
        // Download the child objects accessed at the version right before the execution of this TX
        self.prepare_object_runtime(tx_digest).await?;

        // Extract the epoch start timestamp
        let epoch_start_timestamp = self
            .get_epoch_start_timestamp(tx_info.executed_epoch)
            .await?;

        // Create the gas status
        let gas_status =
            SuiGasStatus::new_with_budget(tx_info.gas_budget, tx_info.gas_price, protocol_config);

        // Temp store for data
        let temporary_store =
            self.to_temporary_store(tx_digest, InputObjects::new(input_objects), protocol_config);

        let move_vm = get_vm(protocol_config, expensive_safety_check_config)?;

        // All prep done
        let res = execute_transaction_to_effects_impl::<execution_mode::Normal, _>(
            shared_obj_refs.iter().map(|r| r.to_object_ref()).collect(),
            temporary_store,
            tx_info.kind,
            tx_info.sender,
            &tx_info.gas,
            *tx_digest,
            tx_info.dependencies.into_iter().collect(),
            &move_vm,
            gas_status,
            &tx_info.executed_epoch,
            epoch_start_timestamp,
            protocol_config,
            metrics,
            true,
        );

        let new_effects: SuiTransactionBlockEffects = res.1.try_into().unwrap();
        let SuiTransactionBlockEffects::V1(new_effects) = new_effects;

        if tx_info.effects != new_effects {
            error!("Replay tool forked {}", tx_digest);
            return Err(LocalExecError::EffectsForked {
                digest: *tx_digest,
                diff: format!("\n{}", diff_effects(&tx_info.effects, &new_effects)),
                on_chain: Box::new(tx_info.effects),
                local: Box::new(new_effects),
            });
        }

        Ok(new_effects)
    }

    fn system_package_ids(&self) -> Vec<ObjectID> {
        let mut ids = BuiltInFramework::all_package_ids();

        if self.current_protocol_version < 5 {
            ids.retain(|id| *id != DEEPBOOK_OBJECT_ID)
        }
        ids
    }

    pub async fn prepare_object_runtime(
        &mut self,
        tx_digest: &TransactionDigest,
    ) -> Result<(), LocalExecError> {
        // Get the child objects loaded

        let loaded_child_objs = match self
            .client
            .read_api()
            .get_loaded_child_objects(*tx_digest)
            .await
        {
            Ok(objs) => objs,
            Err(e) => {
                error!("Error getting dynamic fields loaded objects: {}. This RPC server might not support this feature yet", e);
                return Err(LocalExecError::GeneralError {
                    err: format!("Error getting dynamic fields loaded objects: {}", e),
                });
            }
        };

        // Fetch the refs
        let loaded_child_refs = loaded_child_objs
            .loaded_child_objects
            .iter()
            .map(|obj| (obj.object_id(), obj.sequence_number()))
            .collect::<Vec<_>>();

        // Download and save the specific versions needed
        self.multi_download_and_store(&loaded_child_refs).await?;
        Ok(())
    }

    pub fn get_or_download_object(
        &self,
        obj_id: &ObjectID,
    ) -> Result<Option<Object>, LocalExecError> {
        if let Some(obj) = self.package_cache.lock().expect("Cannot lock").get(obj_id) {
            return Ok(Some(obj.clone()));
        };

        let o = match self.store.get(obj_id) {
            Some(obj) => Some(obj.clone()),
            None => {
                assert!(
                    !self.system_package_ids().contains(obj_id),
                    "All system packages should be downloaded already"
                );
                self.download_latest_object(obj_id)?
            }
        };
        let Some(o) = o else { return Ok(None) };

        if o.is_package() {
            self.package_cache
                .lock()
                .expect("Cannot lock")
                .insert(*obj_id, o.clone());
        }
        let o_ref = o.compute_object_reference();
        self.object_version_cache
            .lock()
            .expect("Cannot lock")
            .insert((o_ref.0, o_ref.1), o.clone());
        Ok(Some(o))
    }

    /// Must be called after `populate_protocol_version_tables`
    pub fn system_package_versions_for_epoch(
        &self,
        epoch: u64,
    ) -> Result<Vec<(ObjectID, SequenceNumber)>, LocalExecError> {
        Ok(self.protocol_version_system_package_table.get(&epoch).ok_or(LocalExecError::GeneralError {
            err: format!("Fatal! N framework versions for epoch {}. Make sure version tables are populated ", epoch)
        })?.clone().into_iter().collect())
    }

    pub async fn protocol_ver_to_epoch_map(
        &self,
    ) -> Result<BTreeMap<u64, (TransactionDigest, u64, u64)>, LocalExecError> {
        let mut range_map = BTreeMap::new();
        let epoch_change_events = self.get_epoch_change_events(false).await?;

        // Exception for Genesis: Protocol version 1 at epoch 0
        let mut tx_digest = TransactionDigest::from_str(GENESIX_TX_DIGEST).unwrap();
        // Somehow the genesis TX did not emit any event, but we know it was the start of version 1
        // So we need to manually add this range
        let (mut start_epoch, mut start_protocol_version) = (0, 1);

        // Exception for incident: Protocol version 2 started epoch 742
        // But this was in safe mode so no events emmitted
        // So we need to manually add this range
        let (mut curr_epoch, mut curr_protocol_version) = (742, 2);
        range_map.insert(
            start_protocol_version,
            (tx_digest, start_epoch, curr_epoch - 1),
        );
        (start_epoch, start_protocol_version) = (curr_epoch, curr_protocol_version);
        tx_digest = TransactionDigest::from_str(SAFE_MODETX_1_DIGEST).unwrap();

        for event in epoch_change_events {
            (curr_epoch, curr_protocol_version) = extract_epoch_and_version(event.clone())?;

            if curr_protocol_version < 3 {
                // Ignore protocol versions before 3 as we've handled before the loop
                continue;
            }

            if start_protocol_version == curr_protocol_version {
                // Same range
                continue;
            }

            // Change in prot version
            // Insert the last range
            range_map.insert(
                start_protocol_version,
                (tx_digest, start_epoch, curr_epoch - 1),
            );
            start_epoch = curr_epoch;
            start_protocol_version = curr_protocol_version;
            tx_digest = event.id.tx_digest;
        }

        // Insert the last range
        range_map.insert(curr_protocol_version, (tx_digest, start_epoch, curr_epoch));

        Ok(range_map)
    }

    pub fn protocol_version_for_epoch(
        epoch: u64,
        mp: &BTreeMap<u64, (TransactionDigest, u64, u64)>,
    ) -> u64 {
        // Naive impl but works for now
        // Can improve with range algos & data structures
        let mut version = 1;
        for (k, v) in mp.iter().rev() {
            if v.1 <= epoch {
                version = *k;
                break;
            }
        }
        version
    }

    pub async fn populate_protocol_version_tables(&mut self) -> Result<(), LocalExecError> {
        self.protocol_version_epoch_table = self.protocol_ver_to_epoch_map().await?;

        let system_package_revisions = self.system_package_versions().await?;

        // This can be more efficient but small footprint so okay for now
        //Table is sorted from earliest to latest
        for (prot_ver, (tx_digest, _, _)) in self.protocol_version_epoch_table.clone() {
            // Use the previous versions protocol version table
            let mut working = self
                .protocol_version_system_package_table
                .get_mut(&(prot_ver - 1))
                .unwrap_or(&mut BTreeMap::new())
                .clone();

            for (id, versions) in system_package_revisions.iter() {
                // Oldest appears first in list, so reverse
                for ver in versions.iter().rev() {
                    if ver.1 == tx_digest {
                        // Found the version for this protocol version
                        working.insert(*id, ver.0);
                        break;
                    }
                }
            }
            self.protocol_version_system_package_table
                .insert(prot_ver, working);
        }
        Ok(())
    }

    pub async fn system_package_versions(
        &self,
    ) -> Result<BTreeMap<ObjectID, Vec<(SequenceNumber, TransactionDigest)>>, LocalExecError> {
        let system_package_ids = self.system_package_ids();
        let mut system_package_objs = self.multi_download_latest(system_package_ids).await?;

        let mut mapping = BTreeMap::new();

        // Extract all the transactions which created or mutated this object
        while !system_package_objs.is_empty() {
            // For the given object and its version, record the transaction which upgraded or created it
            let previous_txs: Vec<_> = system_package_objs
                .iter()
                .map(|o| (o.compute_object_reference(), o.previous_transaction))
                .collect();

            previous_txs.iter().for_each(|((id, ver, _), tx)| {
                mapping.entry(*id).or_insert(vec![]).push((*ver, *tx));
            });

            // Next round
            // Get the previous version of each object if exists
            let previous_ver_refs: Vec<_> = previous_txs
                .iter()
                .filter_map(|(q, _)| {
                    let prev_ver = u64::from(q.1) - 1;
                    if prev_ver == 0 {
                        None
                    } else {
                        Some((q.0, SequenceNumber::from(prev_ver)))
                    }
                })
                .collect();
            system_package_objs = match self.multi_download(&previous_ver_refs).await {
                Ok(packages) => packages,
                Err(LocalExecError::ObjectNotExist { id }) => {
                    // This happens when the RPC server prunes older object
                    // Replays in the current protocol version will work but old ones might not
                    // as we cannot fetch the package
                    warn!("Object {} does not exist on RPC server. This might be due to pruning. Historical replays might not work", id);
                    break;
                }
                Err(LocalExecError::ObjectVersionNotFound { id, version }) => {
                    // This happens when the RPC server prunes older object
                    // Replays in the current protocol version will work but old ones might not
                    // as we cannot fetch the package
                    warn!("Object {} at version {} does not exist on RPC server. This might be due to pruning. Historical replays might not work", id, version);
                    break;
                }
                Err(LocalExecError::ObjectVersionTooHigh {
                    id,
                    asked_version,
                    latest_version,
                }) => {
                    warn!("Object {} at version {} does not exist on RPC server. Latest version is {}. This might be due to pruning. Historical replays might not work", id, asked_version,latest_version );
                    break;
                }
                Err(LocalExecError::ObjectDeleted {
                    id,
                    version,
                    digest,
                }) => {
                    // This happens when the RPC server prunes older object
                    // Replays in the current protocol version will work but old ones might not
                    // as we cannot fetch the package
                    warn!("Object {} at version {} digest {} deleted from RPC server. This might be due to pruning. Historical replays might not work", id, version, digest);
                    break;
                }
                Err(e) => return Err(e),
            };
        }
        Ok(mapping)
    }

    pub async fn get_protocol_config(
        &self,
        epoch_id: EpochId,
    ) -> Result<ProtocolConfig, LocalExecError> {
        self.protocol_version_epoch_table
            .iter()
            .rev()
            .find(|(_, rg)| epoch_id >= rg.1)
            .map(|(p, _rg)| Ok(ProtocolConfig::get_for_version((*p).into())))
            .unwrap_or_else(|| {
                Err(LocalExecError::GeneralError {
                    err: "Protocol version not found for epoch".to_string(),
                })
            })
    }

    /// Gets all the epoch change events
    pub async fn get_epoch_change_events(
        &self,
        reverse: bool,
    ) -> Result<impl Iterator<Item = SuiEvent>, LocalExecError> {
        let struct_tag_str = EPOCH_CHANGE_STRUCT_TAG.to_string();
        let struct_tag =
            parse_struct_tag(&struct_tag_str).map_err(|err| LocalExecError::GeneralError {
                err: format!("Error parsing struct tag: {:#?}", err),
            })?;

        // TODO: Should probably limit/page this but okay for now?
        Ok(self
            .client
            .event_api()
            .query_events(EventFilter::MoveEventType(struct_tag), None, None, reverse)
            .await
            .map_err(|w| LocalExecError::GeneralError {
                err: format!("Error querying system events: {:#?}", w),
            })?
            .data
            .into_iter())
    }

    pub async fn get_epoch_start_timestamp(&self, epoch_id: u64) -> Result<u64, LocalExecError> {
        // For epoch in range [3, 742), we have no data, but no user TX was executed, so return dummy
        if (2 < epoch_id) && (epoch_id < 742) {
            return Ok(0);
        }

        let event = self.get_epoch_change_events(true).await?.find(|ev| {
            match extract_epoch_and_version(ev.clone()) {
                Ok((epoch, _)) => epoch == epoch_id,
                Err(_) => false,
            }
        });

        let event = event.ok_or(LocalExecError::GeneralError {
            err: format!(
                "Unable to find event and hence epoch start timestamp for  {}",
                epoch_id
            ),
        })?;

        let epoch_change_tx = event.id.tx_digest;

        // Fetch full transaction content
        let tx_fetch_opts = SuiTransactionBlockResponseOptions::full_content();
        let tx_info = self
            .client
            .read_api()
            .get_transaction_with_options(epoch_change_tx, tx_fetch_opts)
            .await
            .map_err(LocalExecError::from)?;

        let orig_tx: SenderSignedData = bcs::from_bytes(&tx_info.raw_transaction).unwrap();
        let tx_kind_orig = orig_tx.transaction_data().kind();

        if let TransactionKind::ChangeEpoch(change) = tx_kind_orig {
            return Ok(change.epoch_start_timestamp_ms);
        }
        Err(LocalExecError::GeneralError {
            err: format!(
                "Invalid epoch change transaction in events for  {}",
                epoch_id
            ),
        })
    }

    async fn resolve_tx_components(
        &self,
        tx_digest: &TransactionDigest,
    ) -> Result<TxInfo, LocalExecError> {
        // Fetch full transaction content
        let tx_fetch_opts = SuiTransactionBlockResponseOptions::full_content();
        let tx_info = self
            .client
            .read_api()
            .get_transaction_with_options(*tx_digest, tx_fetch_opts)
            .await
            .map_err(LocalExecError::from)?;
        let sender = match tx_info.clone().transaction.unwrap().data {
            sui_json_rpc_types::SuiTransactionBlockData::V1(tx) => tx.sender,
        };

        let raw_tx_bytes = tx_info.clone().raw_transaction;
        let orig_tx: SenderSignedData = bcs::from_bytes(&raw_tx_bytes).unwrap();
        let input_objs = orig_tx
            .transaction_data()
            .input_objects()
            .map_err(|e| LocalExecError::UserInputError { err: e })?;
        let tx_kind_orig = orig_tx.transaction_data().kind();

        let SuiTransactionBlockEffects::V1(effects) = tx_info.clone().effects.unwrap();

        // Download the objects at the version right before the execution of this TX
        let modified_at_versions: Vec<(ObjectID, SequenceNumber)> = effects.modified_at_versions();

        let shared_obj_refs = effects.shared_objects();

        let gas_data = match tx_info.clone().transaction.unwrap().data {
            sui_json_rpc_types::SuiTransactionBlockData::V1(tx) => tx.gas_data,
        };
        let gas_object_refs: Vec<_> = gas_data
            .payment
            .iter()
            .map(|obj_ref| obj_ref.to_object_ref())
            .collect();

        let epoch_id = effects.executed_epoch;

        Ok(TxInfo {
            kind: tx_kind_orig.clone(),
            sender,
            modified_at_versions,
            input_objects: input_objs,
            shared_object_refs: shared_obj_refs.to_vec(),
            gas: gas_object_refs,
            gas_budget: gas_data.budget,
            gas_price: gas_data.price,
            executed_epoch: epoch_id,
            dependencies: effects.dependencies().to_vec(),
            effects,
            // Find the protocol version for this epoch
            // This assumes we already initialized the protocol version table `protocol_version_epoch_table`
            protocol_config: self.get_protocol_config(epoch_id).await?,
        })
    }

    async fn resolve_download_input_objects(
        &mut self,
        tx_info: &TxInfo,
    ) -> Result<Vec<(InputObjectKind, Object)>, LocalExecError> {
        // Download the input objects
        let mut package_inputs = vec![];
        let mut imm_owned_inputs = vec![];
        let mut shared_inputs = vec![];

        tx_info
            .input_objects
            .iter()
            .map(|kind| match kind {
                InputObjectKind::MovePackage(i) => {
                    package_inputs.push(*i);
                    Ok(())
                }
                InputObjectKind::ImmOrOwnedMoveObject(o_ref) => {
                    imm_owned_inputs.push((o_ref.0, o_ref.1));
                    Ok(())
                }
                InputObjectKind::SharedMoveObject {
                    id,
                    initial_shared_version: _,
                    mutable: _,
                } => {
                    // We already downloaded
                    if let Some(o) = self.store.get(id) {
                        shared_inputs.push(o.clone());
                        Ok(())
                    } else {
                        Err(LocalExecError::GeneralError {
                            err: format!(
                                "Object not found in cache {}. Should've been downloaded",
                                id
                            ),
                        })
                    }
                }
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Download the imm and owned objects
        let mut in_objs = self.multi_download_and_store(&imm_owned_inputs).await?;

        // For packages, download latest if non framework
        // If framework, download relevant for the current protocol version
        in_objs.extend(
            self.multi_download_relevant_packages_and_store(
                package_inputs,
                tx_info.protocol_config.version.as_u64(),
            )
            .await?,
        );
        // Add shared objects
        in_objs.extend(shared_inputs);

        let resolved_input_objs = tx_info
            .input_objects
            .iter()
            .map(|kind| match kind {
                InputObjectKind::MovePackage(i) => {
                    // Okay to unwrap since we downloaded it
                    (
                        *kind,
                        self.package_cache
                            .lock()
                            .expect("Cannot lock")
                            .get(i)
                            .unwrap()
                            .clone(),
                    )
                }
                InputObjectKind::ImmOrOwnedMoveObject(o_ref) => (
                    *kind,
                    self.object_version_cache
                        .lock()
                        .expect("Cannot lock")
                        .get(&(o_ref.0, o_ref.1))
                        .unwrap()
                        .clone(),
                ),
                InputObjectKind::SharedMoveObject {
                    id,
                    initial_shared_version: _,
                    mutable: _,
                } => {
                    // we already downloaded
                    (*kind, self.store.get(id).unwrap().clone())
                }
            })
            .collect();

        Ok(resolved_input_objs)
    }
}

impl BackingPackageStore for LocalExec {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<Object>> {
        fn inner(self_: &LocalExec, package_id: &ObjectID) -> SuiResult<Option<Object>> {
            // If package not present fetch it from the network
            self_
                .get_or_download_object(package_id)
                .map_err(|e| SuiError::GenericStorageError(e.to_string()))
        }

        let res = inner(self, package_id);
        self.exec_store_events
            .lock()
            .expect("Unable to lock events list")
            .push(ExecutionStoreEvent::BackingPackageGetPackageObject {
                package_id: *package_id,
                result: res.clone(),
            });
        res
    }
}

impl ChildObjectResolver for LocalExec {
    fn read_child_object(&self, parent: &ObjectID, child: &ObjectID) -> SuiResult<Option<Object>> {
        fn inner(
            self_: &LocalExec,
            parent: &ObjectID,
            child: &ObjectID,
        ) -> SuiResult<Option<Object>> {
            let child_object = match self_.get_object(child)? {
                None => return Ok(None),
                Some(o) => o,
            };
            let parent = *parent;
            if child_object.owner != Owner::ObjectOwner(parent.into()) {
                return Err(SuiError::InvalidChildObjectAccess {
                    object: *child,
                    given_parent: parent,
                    actual_owner: child_object.owner,
                });
            }
            Ok(Some(child_object))
        }

        let res = inner(self, parent, child);
        self.exec_store_events
            .lock()
            .expect("Unable to lock events list")
            .push(
                ExecutionStoreEvent::ChildObjectResolverStoreReadChildObject {
                    parent: *parent,
                    child: *child,
                    result: res.clone(),
                },
            );
        res
    }
}

impl ParentSync for LocalExec {
    fn get_latest_parent_entry_ref(&self, object_id: ObjectID) -> SuiResult<Option<ObjectRef>> {
        fn inner(self_: &LocalExec, object_id: ObjectID) -> SuiResult<Option<ObjectRef>> {
            if let Some(v) = self_.store.get(&object_id) {
                return Ok(Some(v.compute_object_reference()));
            }
            Ok(None)
        }
        let res = inner(self, object_id);
        self.exec_store_events
            .lock()
            .expect("Unable to lock events list")
            .push(
                ExecutionStoreEvent::ParentSyncStoreGetLatestParentEntryRef {
                    object_id,
                    result: res.clone(),
                },
            );
        res
    }
}

impl ResourceResolver for LocalExec {
    type Error = LocalExecError;

    fn get_resource(
        &self,
        address: &AccountAddress,
        typ: &StructTag,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        fn inner(
            self_: &LocalExec,
            address: &AccountAddress,
            typ: &StructTag,
        ) -> Result<Option<Vec<u8>>, LocalExecError> {
            let Some(object) = self_.get_or_download_object(&ObjectID::from(*address))? else {
                return Ok(None);
            };

            match &object.data {
                Data::Move(m) => {
                    assert!(
                        m.is_type(typ),
                        "Invariant violation: ill-typed object in storage \
                        or bad object request from caller"
                    );
                    Ok(Some(m.contents().to_vec()))
                }
                other => unimplemented!(
                    "Bad object lookup: expected Move object, but got {:#?}",
                    other
                ),
            }
        }

        let res = inner(self, address, typ);
        self.exec_store_events
            .lock()
            .expect("Unable to lock events list")
            .push(ExecutionStoreEvent::ResourceResolverGetResource {
                address: *address,
                typ: typ.clone(),
                result: res.clone(),
            });
        res
    }
}

impl ModuleResolver for LocalExec {
    type Error = LocalExecError;

    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        fn inner(
            self_: &LocalExec,
            module_id: &ModuleId,
        ) -> Result<Option<Vec<u8>>, LocalExecError> {
            Ok(self_
                .get_package(&ObjectID::from(*module_id.address()))
                .map_err(LocalExecError::from)?
                .and_then(|package| {
                    package
                        .serialized_module_map()
                        .get(module_id.name().as_str())
                        .cloned()
                }))
        }

        let res = inner(self, module_id);
        self.exec_store_events
            .lock()
            .expect("Unable to lock events list")
            .push(ExecutionStoreEvent::ModuleResolverGetModule {
                module_id: module_id.clone(),
                result: res.clone(),
            });
        res
    }
}

impl ModuleResolver for &mut LocalExec {
    type Error = LocalExecError;

    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        // Recording event here will be double-counting since its already recorded in the get_module fn
        (**self).get_module(module_id)
    }
}

impl ObjectStore for LocalExec {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError> {
        let res = Ok(self.store.get(object_id).cloned());
        self.exec_store_events
            .lock()
            .expect("Unable to lock events list")
            .push(ExecutionStoreEvent::ObjectStoreGetObject {
                object_id: *object_id,
                result: res.clone(),
            });
        res
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>, SuiError> {
        let res = Ok(self.store.get(object_id).and_then(|obj| {
            if obj.version() == version {
                Some(obj.clone())
            } else {
                None
            }
        }));

        self.exec_store_events
            .lock()
            .expect("Unable to lock events list")
            .push(ExecutionStoreEvent::ObjectStoreGetObjectByKey {
                object_id: *object_id,
                version,
                result: res.clone(),
            });

        res
    }
}

impl ObjectStore for &mut LocalExec {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError> {
        // Recording event here will be double-counting since its already recorded in the get_module fn
        (**self).get_object(object_id)
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>, SuiError> {
        // Recording event here will be double-counting since its already recorded in the get_module fn
        (**self).get_object_by_key(object_id, version)
    }
}

impl GetModule for LocalExec {
    type Error = LocalExecError;
    type Item = CompiledModule;

    fn get_module_by_id(&self, id: &ModuleId) -> anyhow::Result<Option<Self::Item>, Self::Error> {
        let res = get_module_by_id(self, id).map_err(|e| e.into());

        self.exec_store_events
            .lock()
            .expect("Unable to lock events list")
            .push(ExecutionStoreEvent::GetModuleGetModuleByModuleId {
                id: id.clone(),
                result: res.clone(),
            });
        res
    }
}

fn obj_from_sui_obj_response(o: &SuiObjectResponse) -> Result<Object, LocalExecError> {
    let o = o.object().map_err(LocalExecError::from)?.clone();
    obj_from_sui_obj_data(&o)
}

fn obj_from_sui_obj_data(o: &SuiObjectData) -> Result<Object, LocalExecError> {
    match TryInto::<Object>::try_into(o.clone()) {
        Ok(obj) => Ok(obj),
        Err(e) => Err(e.into()),
    }
}

pub fn get_vm(
    protocol_config: &ProtocolConfig,
    expensive_safety_check_config: ExpensiveSafetyCheckConfig,
) -> Result<Arc<adapter::MoveVM>, LocalExecError> {
    let native_functions = sui_move_natives::all_natives(/* disable silent */ false);
    let move_vm = Arc::new(
        adapter::new_move_vm(
            native_functions.clone(),
            protocol_config,
            expensive_safety_check_config.enable_move_vm_paranoid_checks(),
        )
        .expect("We defined natives to not fail here"),
    );
    Ok(move_vm)
}

fn extract_epoch_and_version(ev: SuiEvent) -> Result<(u64, u64), LocalExecError> {
    if let serde_json::Value::Object(w) = ev.parsed_json {
        let epoch = u64::from_str(&w["epoch"].to_string().replace('\"', "")).unwrap();
        let version = u64::from_str(&w["protocol_version"].to_string().replace('\"', "")).unwrap();
        return Ok((epoch, version));
    }

    Err(LocalExecError::GeneralError {
        err: "Unexpected event format".to_string(),
    })
}

fn diff_effects(
    eff1: &SuiTransactionBlockEffectsV1,
    eff2: &SuiTransactionBlockEffectsV1,
) -> String {
    let on_chain_str = format!("{:#?}", eff1);
    let local_chain_str = format!("{:#?}", eff2);
    let mut res = vec![];

    let diff = TextDiff::from_lines(&on_chain_str, &local_chain_str);
    println!("On-chain vs local diff");
    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            ChangeTag::Delete => "---",
            ChangeTag::Insert => "+++",
            ChangeTag::Equal => "   ",
        };
        res.push(format!("{}{}", sign, change));
    }

    res.join("\n")
}

/// TODO: Limited set but will add more
#[derive(Debug)]
pub enum ExecutionStoreEvent {
    BackingPackageGetPackageObject {
        package_id: ObjectID,
        result: SuiResult<Option<Object>>,
    },
    ChildObjectResolverStoreReadChildObject {
        parent: ObjectID,
        child: ObjectID,
        result: SuiResult<Option<Object>>,
    },
    ParentSyncStoreGetLatestParentEntryRef {
        object_id: ObjectID,
        result: SuiResult<Option<ObjectRef>>,
    },
    ResourceResolverGetResource {
        address: AccountAddress,
        typ: StructTag,
        result: Result<Option<Vec<u8>>, LocalExecError>,
    },
    ModuleResolverGetModule {
        module_id: ModuleId,
        result: Result<Option<Vec<u8>>, LocalExecError>,
    },
    ObjectStoreGetObject {
        object_id: ObjectID,
        result: SuiResult<Option<Object>>,
    },
    ObjectStoreGetObjectByKey {
        object_id: ObjectID,
        version: VersionNumber,
        result: SuiResult<Option<Object>>,
    },
    GetModuleGetModuleByModuleId {
        id: ModuleId,
        result: Result<Option<CompiledModule>, LocalExecError>,
    },
}
