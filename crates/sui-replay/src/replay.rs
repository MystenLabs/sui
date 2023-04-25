// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::data_fetcher::DataFetcher;
use crate::data_fetcher::RemoteFetcher;
use crate::types::*;
use futures::executor::block_on;
use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::account_address::AccountAddress;
use move_core_types::language_storage::{ModuleId, StructTag};
use move_core_types::parser::parse_struct_tag;
use move_core_types::resolver::{ModuleResolver, ResourceResolver};
use similar::{ChangeTag, TextDiff};
use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use sui_adapter::adapter;
use sui_adapter::execution_engine::execute_transaction_to_effects_impl;
use sui_adapter::execution_mode;
use sui_config::node::ExpensiveSafetyCheckConfig;
use sui_core::authority::TemporaryStore;
use sui_framework::BuiltInFramework;
use sui_json_rpc_types::{EventFilter, SuiEvent, SuiTransactionBlockEffectsV1};
use sui_json_rpc_types::{SuiTransactionBlockEffects, SuiTransactionBlockEffectsAPI};
use sui_protocol_config::ProtocolConfig;
use sui_sdk::{SuiClient, SuiClientBuilder};
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress, VersionNumber};
use sui_types::committee::EpochId;
use sui_types::digests::TransactionDigest;
use sui_types::error::{SuiError, SuiResult};
use sui_types::gas::SuiGasStatus;
use sui_types::messages::{InputObjectKind, InputObjects, TransactionKind};
use sui_types::messages::{SenderSignedData, TransactionDataAPI};
use sui_types::metrics::LimitsMetrics;
use sui_types::object::{Data, Object, Owner};
use sui_types::storage::get_module_by_id;
use sui_types::storage::{BackingPackageStore, ChildObjectResolver, ObjectStore, ParentSync};
use sui_types::DEEPBOOK_OBJECT_ID;
use tracing::{error, warn};

// TODO: add persistent cache. But perf is good enough already.
// TODO: handle safe mode

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolVersionSummary {
    /// Protocol version at this point
    pub protocol_version: u64,
    /// The first epoch that uses this protocol version
    pub epoch_start: u64,
    /// The last epoch that uses this protocol version
    pub epoch_end: u64,
    /// The first checkpoint in this protocol v ersion
    pub checkpoint_start: u64,
    /// The last checkpoint in this protocol version
    pub checkpoint_end: u64,
    /// The transaction which triggered this epoch change
    pub epoch_change_tx: TransactionDigest,
}

pub struct Storage {
    /// These are objects at the frontier of the execution's view
    /// They might not be the latest object currently but they are the latest objects
    /// for the TX at the time it was run
    /// This store cannot be shared between runners
    pub live_objects_store: BTreeMap<ObjectID, Object>,

    /// Package cache and object version cache can be shared between runners
    /// Non system packages are immutable so we can cache these
    pub package_cache: Arc<Mutex<BTreeMap<ObjectID, Object>>>,
    /// Object contents are frozen at their versions so we can cache these
    /// We must place system packages here as well
    pub object_version_cache: Arc<Mutex<BTreeMap<(ObjectID, SequenceNumber), Object>>>,
}

impl std::fmt::Display for Storage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Live object store")?;
        for (id, obj) in self.live_objects_store.iter() {
            writeln!(f, "{}: {:?}", id, obj.compute_object_reference())?;
        }
        writeln!(f, "Package cache")?;
        for (id, obj) in self.package_cache.lock().expect("Unable to lock").iter() {
            writeln!(f, "{}: {:?}", id, obj.compute_object_reference())?;
        }
        writeln!(f, "Object version cache")?;
        for (id, _) in self
            .object_version_cache
            .lock()
            .expect("Unable to lock")
            .iter()
        {
            writeln!(f, "{}: {}", id.0, id.1)?;
        }

        write!(f, "")
    }
}

impl Storage {
    pub fn default() -> Self {
        Self {
            live_objects_store: BTreeMap::new(),
            package_cache: Arc::new(Mutex::new(BTreeMap::new())),
            object_version_cache: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }
}

pub struct LocalExec {
    pub client: SuiClient,
    // For a given protocol version, what TX created it, and what is the valid range of epochs
    // at this protocol version.
    pub protocol_version_epoch_table: BTreeMap<u64, ProtocolVersionSummary>,
    // For a given protocol version, the mapping valid sequence numbers for each framework package
    pub protocol_version_system_package_table: BTreeMap<u64, BTreeMap<ObjectID, SequenceNumber>>,
    // The current protocol version for this execution
    pub current_protocol_version: u64,
    // All state is contained here
    pub storage: Storage,
    // Debug events
    pub exec_store_events: Arc<Mutex<Vec<ExecutionStoreEvent>>>,
    // Debug events
    pub metrics: Arc<LimitsMetrics>,
    // Used for fetching data from the network or remote store
    pub fetcher: RemoteFetcher,

    // Retry policies due to RPC errors
    pub num_retries_for_timeout: u32,
    pub sleep_period_for_timeout: std::time::Duration,
}

impl LocalExec {
    /// Wrapper around fetcher in case we want to add more functionality
    /// Such as fetching from local DB from snapshot
    pub async fn multi_download(
        &self,
        objs: &[(ObjectID, SequenceNumber)],
    ) -> Result<Vec<Object>, LocalExecError> {
        let mut num_retries_for_timeout = self.num_retries_for_timeout as i64;
        while num_retries_for_timeout >= 0 {
            match self.fetcher.multi_get_versioned(objs).await {
                Ok(objs) => return Ok(objs),
                Err(LocalExecError::SuiRpcRequestTimeout) => {
                    warn!(
                        "RPC request timed out. Retries left {}. Sleeping for {}s",
                        num_retries_for_timeout,
                        self.sleep_period_for_timeout.as_secs()
                    );
                    num_retries_for_timeout -= 1;
                    tokio::time::sleep(self.sleep_period_for_timeout).await;
                }
                Err(e) => return Err(e),
            }
        }
        Err(LocalExecError::SuiRpcRequestTimeout)
    }
    /// Wrapper around fetcher in case we want to add more functionality
    /// Such as fetching from local DB from snapshot
    pub async fn multi_download_latest(
        &self,
        objs: &[ObjectID],
    ) -> Result<Vec<Object>, LocalExecError> {
        let mut num_retries_for_timeout = self.num_retries_for_timeout as i64;
        while num_retries_for_timeout >= 0 {
            match self.fetcher.multi_get_latest(objs).await {
                Ok(objs) => return Ok(objs),
                Err(LocalExecError::SuiRpcRequestTimeout) => {
                    warn!(
                        "RPC request timed out. Retries left {}. Sleeping for {}s",
                        num_retries_for_timeout,
                        self.sleep_period_for_timeout.as_secs()
                    );
                    num_retries_for_timeout -= 1;
                    tokio::time::sleep(self.sleep_period_for_timeout).await;
                }
                Err(e) => return Err(e),
            }
        }
        Err(LocalExecError::SuiRpcRequestTimeout)
    }

    pub async fn fetch_loaded_child_refs(
        &self,
        tx_digest: &TransactionDigest,
    ) -> Result<Vec<(ObjectID, SequenceNumber)>, LocalExecError> {
        // Get the child objects loaded
        self.fetcher.get_loaded_child_objects(tx_digest).await
    }

    /// Gets all the epoch change events
    pub async fn get_epoch_change_events(
        &self,
        reverse: bool,
    ) -> Result<impl Iterator<Item = SuiEvent>, LocalExecError> {
        let struct_tag_str = EPOCH_CHANGE_STRUCT_TAG.to_string();
        let struct_tag = parse_struct_tag(&struct_tag_str)?;

        // TODO: Should probably limit/page this but okay for now?
        Ok(self
            .client
            .event_api()
            .query_events(EventFilter::MoveEventType(struct_tag), None, None, reverse)
            .await
            .map_err(|e| LocalExecError::UnableToQuerySystemEvents {
                rpc_err: e.to_string(),
            })?
            .data
            .into_iter())
    }

    pub async fn new_from_fn_url(http_url: &str) -> Result<Self, LocalExecError> {
        Ok(Self::new(
            SuiClientBuilder::default()
                .request_timeout(RPC_TIMEOUT_ERR_SLEEP_RETRY_PERIOD)
                .max_concurrent_requests(MAX_CONCURRENT_REQUESTS)
                .build(http_url)
                .await?,
        ))
    }

    /// This captures the state of the network at a given point in time and populates
    /// prptocol version tables including which system packages to fetch
    /// If this function is called across epoch boundaries, the info might be stale.
    /// But it should only be called once per epoch.
    pub async fn init_for_execution(mut self) -> Result<Self, LocalExecError> {
        self.populate_protocol_version_tables().await?;
        Ok(self)
    }

    pub fn new(client: SuiClient) -> Self {
        // Use a throwaway metrics registry for local execution.
        let registry = prometheus::Registry::new();
        let metrics = Arc::new(LimitsMetrics::new(&registry));
        Self {
            client: client.clone(),
            protocol_version_epoch_table: BTreeMap::new(),
            protocol_version_system_package_table: BTreeMap::new(),
            current_protocol_version: 0,
            exec_store_events: Arc::new(Mutex::new(Vec::new())),
            metrics,
            storage: Storage::default(),
            fetcher: RemoteFetcher { rpc_client: client },
            // TODO: make this configurable
            num_retries_for_timeout: RPC_TIMEOUT_ERR_NUM_RETRIES,
            sleep_period_for_timeout: RPC_TIMEOUT_ERR_SLEEP_RETRY_PERIOD,
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

    pub async fn multi_download_and_store(
        &mut self,
        objs: &[(ObjectID, SequenceNumber)],
    ) -> Result<Vec<Object>, LocalExecError> {
        let objs = self.multi_download(objs).await?;

        // Backfill the store
        for obj in objs.iter() {
            let o_ref = obj.compute_object_reference();
            self.storage.live_objects_store.insert(o_ref.0, obj.clone());
            self.storage
                .object_version_cache
                .lock()
                .expect("Cannot lock")
                .insert((o_ref.0, o_ref.1), obj.clone());
            if obj.is_package() {
                self.storage
                    .package_cache
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
            .multi_download_latest(&non_system_package_objs)
            .await?
            .into_iter()
            .chain(syst_packages_objs.into_iter());

        for obj in objs.clone() {
            let o_ref = obj.compute_object_reference();
            // We dont always want the latest in store
            //self.storage.store.insert(o_ref.0, obj.clone());
            self.storage
                .object_version_cache
                .lock()
                .expect("Cannot lock")
                .insert((o_ref.0, o_ref.1), obj.clone());
            if obj.is_package() {
                self.storage
                    .package_cache
                    .lock()
                    .expect("Cannot lock")
                    .insert(o_ref.0, obj.clone());
            }
        }
        Ok(objs.collect())
    }

    // TODO: remove this after `futures::executor::block_on` is removed.
    #[allow(clippy::disallowed_methods)]
    pub fn download_object(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
    ) -> Result<Object, LocalExecError> {
        if self
            .storage
            .object_version_cache
            .lock()
            .expect("Cannot lock")
            .contains_key(&(*object_id, version))
        {
            return Ok(self
                .storage
                .object_version_cache
                .lock()
                .expect("Cannot lock")
                .get(&(*object_id, version))
                .ok_or(LocalExecError::InternalCacheInvariantViolation {
                    id: *object_id,
                    version: Some(version),
                })?
                .clone());
        }

        let o = block_on(self.multi_download(&[(*object_id, version)])).map(|mut q| {
            q.pop().unwrap_or_else(|| {
                panic!(
                    "Downloaded obj response cannot be empty {:?}",
                    (*object_id, version)
                )
            })
        })?;

        let o_ref = o.compute_object_reference();
        self.storage
            .object_version_cache
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
        let resp = block_on({
            //info!("Downloading latest object {object_id}");
            self.multi_download_latest(&[*object_id])
        })
        .map(|mut q| {
            q.pop()
                .unwrap_or_else(|| panic!("Downloaded obj response cannot be empty {}", *object_id))
        });

        match resp {
            Ok(v) => Ok(Some(v)),
            Err(LocalExecError::ObjectNotExist { id }) => {
                error!("Could not find object {id} on RPC server. It might have been pruned, deleted, or never existed.");
                Ok(None)
            }
            Err(LocalExecError::ObjectDeleted {
                id,
                version,
                digest,
            }) => {
                error!("Object {id} {version} {digest} was deleted on RPC server.");
                Ok(None)
            }
            Err(err) => Err(LocalExecError::SuiRpcError {
                err: err.to_string(),
            }),
        }
    }

    pub async fn execute_all_in_checkpoints(
        &mut self,
        checkpoint_ids: &[u64],
        expensive_safety_check_config: &ExpensiveSafetyCheckConfig,
        terminate_early: bool,
    ) -> Result<(u64, u64), LocalExecError> {
        // Get all the TXs at this checkpoint
        let mut txs = Vec::new();
        for checkpoint_id in checkpoint_ids {
            txs.extend(
                self.fetcher
                    .get_checkpoint_txs(*checkpoint_id)
                    .await
                    .map_err(|e| LocalExecError::SuiRpcError { err: e.to_string() })?,
            );
        }
        let num = txs.len();
        let mut succeeded = 0;
        for tx in txs {
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
            succeeded += 1;
        }
        Ok((succeeded, num as u64))
    }

    /// Should be called after `init_for_execution`
    pub async fn execute(
        &mut self,
        tx_digest: &TransactionDigest,
        expensive_safety_check_config: ExpensiveSafetyCheckConfig,
    ) -> Result<SuiTransactionBlockEffectsV1, LocalExecError> {
        assert!(
            !self.protocol_version_system_package_table.is_empty()
                || !self.protocol_version_epoch_table.is_empty(),
            "Required tables not populated. Must call `init_for_execution` first"
        );

        let tx_info = self.resolve_tx_components(tx_digest).await?;

        // A lot of the logic here isnt designed for genesis
        if *tx_digest == TransactionDigest::genesis() || tx_info.sender == SuiAddress::ZERO {
            // Genesis.
            warn!(
                "Genesis replay not supported: {}, skipping transaction",
                tx_digest
            );
            return Ok(tx_info.effects);
            // return Err(LocalExecError::GenesisReplayNotSupported { digest: *tx_digest });
        }

        // Initialize the state necessary for execution
        // Get the input objects
        let input_objects = self.initialize_execution_env_state(&tx_info).await?;

        // At this point we have all the objects needed for replay

        // This assumes we already initialized the protocol version table `protocol_version_epoch_table`
        let protocol_config = &tx_info.protocol_config;

        let metrics = self.metrics.clone();

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

        // We could probably cache the VM per protocol config
        let move_vm = get_vm(protocol_config, expensive_safety_check_config)?;

        // All prep done
        let res = execute_transaction_to_effects_impl::<execution_mode::Normal, _>(
            tx_info.shared_object_refs,
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

        let SuiTransactionBlockEffects::V1(new_effects) = res.1.try_into().unwrap();

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

    /// This is the only function which accesses the network during execution
    pub fn get_or_download_object(
        &self,
        obj_id: &ObjectID,
        package_expected: bool,
    ) -> Result<Option<Object>, LocalExecError> {
        if package_expected {
            if let Some(obj) = self
                .storage
                .package_cache
                .lock()
                .expect("Cannot lock")
                .get(obj_id)
            {
                return Ok(Some(obj.clone()));
            };
            // Check if its a system package because we must've downloaded all
            assert!(
                !self.system_package_ids().contains(obj_id),
                "All system packages should be downloaded already"
            );
        } else if let Some(obj) = self.storage.live_objects_store.get(obj_id) {
            return Ok(Some(obj.clone()));
        }

        let Some(o) =  self.download_latest_object(obj_id)? else { return Ok(None) };

        if o.is_package() {
            assert!(
                package_expected,
                "Did not expect package but downloaded object is a package: {obj_id}"
            );

            self.storage
                .package_cache
                .lock()
                .expect("Cannot lock")
                .insert(*obj_id, o.clone());
        }
        let o_ref = o.compute_object_reference();
        self.storage
            .object_version_cache
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
        Ok(self
            .protocol_version_system_package_table
            .get(&epoch)
            .ok_or(LocalExecError::FrameworkObjectVersionTableNotPopulated { epoch })?
            .clone()
            .into_iter()
            .collect())
    }

    pub async fn protocol_ver_to_epoch_map(
        &self,
    ) -> Result<BTreeMap<u64, ProtocolVersionSummary>, LocalExecError> {
        let mut range_map = BTreeMap::new();
        let epoch_change_events = self.get_epoch_change_events(false).await?;

        // Exception for Genesis: Protocol version 1 at epoch 0
        let mut tx_digest = TransactionDigest::from_str(GENESIX_TX_DIGEST).unwrap();
        // Somehow the genesis TX did not emit any event, but we know it was the start of version 1
        // So we need to manually add this range
        let (mut start_epoch, mut start_protocol_version, mut start_checkpoint) = (0, 1, 0u64);

        // Exception for incident: Protocol version 2 started epoch 742
        // But this was in safe mode so no events emmitted
        // So we need to manually add this range
        let (mut curr_epoch, mut curr_protocol_version) = (742, 2);
        let mut curr_checkpoint = self
            .fetcher
            .get_transaction(&TransactionDigest::from_str(SAFE_MODE_TX_1_DIGEST).unwrap())
            .await?
            .checkpoint
            .expect("Checkpoint should be present");
        range_map.insert(
            start_protocol_version,
            ProtocolVersionSummary {
                protocol_version: start_protocol_version,
                epoch_start: start_epoch,
                epoch_end: curr_epoch - 1,
                checkpoint_start: start_checkpoint,
                checkpoint_end: curr_checkpoint - 1,
                epoch_change_tx: tx_digest,
            },
        );

        (start_epoch, start_protocol_version, start_checkpoint) =
            (curr_epoch, curr_protocol_version, curr_checkpoint);
        tx_digest = TransactionDigest::from_str(SAFE_MODE_TX_1_DIGEST).unwrap();
        // This is the final tx digest for the epoch change. We need this to track the final checkpoint
        let mut end_epoch_tx_digest = tx_digest;

        for event in epoch_change_events {
            (curr_epoch, curr_protocol_version) = extract_epoch_and_version(event.clone())?;
            end_epoch_tx_digest = event.id.tx_digest;

            if curr_protocol_version < 3 {
                // Ignore protocol versions before 3 as we've handled before the loop
                continue;
            }

            if start_protocol_version == curr_protocol_version {
                // Same range
                continue;
            }

            // Change in prot version
            // Find the last checkpoint
            curr_checkpoint = self
                .fetcher
                .get_transaction(&event.id.tx_digest)
                .await?
                .checkpoint
                .expect("Checkpoint should be present");
            // Insert the last range
            range_map.insert(
                start_protocol_version,
                ProtocolVersionSummary {
                    protocol_version: start_protocol_version,
                    epoch_start: start_epoch,
                    epoch_end: curr_epoch - 1,
                    checkpoint_start: start_checkpoint,
                    checkpoint_end: curr_checkpoint - 1,
                    epoch_change_tx: tx_digest,
                },
            );

            start_epoch = curr_epoch;
            start_protocol_version = curr_protocol_version;
            tx_digest = event.id.tx_digest;
            start_checkpoint = curr_checkpoint;
        }

        // Insert the last range
        range_map.insert(
            curr_protocol_version,
            ProtocolVersionSummary {
                protocol_version: curr_protocol_version,
                epoch_start: start_epoch,
                epoch_end: curr_epoch,
                checkpoint_start: curr_checkpoint,
                checkpoint_end: self
                    .fetcher
                    .get_transaction(&end_epoch_tx_digest)
                    .await?
                    .checkpoint
                    .expect("Checkpoint should be present"),
                epoch_change_tx: tx_digest,
            },
        );

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
        for (
            prot_ver,
            ProtocolVersionSummary {
                epoch_change_tx: tx_digest,
                ..
            },
        ) in self.protocol_version_epoch_table.clone()
        {
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
        let mut system_package_objs = self.multi_download_latest(&system_package_ids).await?;

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
            .find(|(_, rg)| epoch_id >= rg.epoch_start)
            .map(|(p, _rg)| Ok(ProtocolConfig::get_for_version((*p).into())))
            .unwrap_or_else(|| Err(LocalExecError::ProtocolVersionNotFound { epoch: epoch_id }))
    }

    pub async fn checkpoints_for_epoch(&self, epoch_id: u64) -> Result<(u64, u64), LocalExecError> {
        let epoch_change_events = self
            .get_epoch_change_events(true)
            .await?
            .collect::<Vec<_>>();
        let (start_checkpoint, start_epoch_idx) = if epoch_id == 0 {
            (0, 1)
        } else {
            let idx = epoch_change_events
                .iter()
                .position(|ev| match extract_epoch_and_version(ev.clone()) {
                    Ok((epoch, _)) => epoch == epoch_id,
                    Err(_) => false,
                })
                .ok_or(LocalExecError::EventNotFound { epoch: epoch_id })?;
            let epoch_change_tx = epoch_change_events[idx].id.tx_digest;
            (
                self.fetcher
                    .get_transaction(&epoch_change_tx)
                    .await?
                    .checkpoint
                    .expect("Checkpoint should be present"),
                idx,
            )
        };

        let next_epoch_change_tx = epoch_change_events
            .get(start_epoch_idx + 1)
            .map(|v| v.id.tx_digest)
            .ok_or(LocalExecError::UnableToDetermineCheckpoint { epoch: epoch_id })?;

        let next_epoch_checkpoint = self
            .fetcher
            .get_transaction(&next_epoch_change_tx)
            .await?
            .checkpoint
            .expect("Checkpoint should be present");

        Ok((start_checkpoint, next_epoch_checkpoint - 1))
    }

    pub async fn get_epoch_start_timestamp(&self, epoch_id: u64) -> Result<u64, LocalExecError> {
        // For epoch in range [3, 742), we have no data, but no user TX was executed, so return dummy
        if (2 < epoch_id) && (epoch_id < 742) {
            return Ok(0);
        }

        let event = self
            .get_epoch_change_events(true)
            .await?
            .find(|ev| match extract_epoch_and_version(ev.clone()) {
                Ok((epoch, _)) => epoch == epoch_id,
                Err(_) => false,
            })
            .ok_or(LocalExecError::EventNotFound { epoch: epoch_id })?;

        let epoch_change_tx = event.id.tx_digest;

        // Fetch full transaction content
        let tx_info = self.fetcher.get_transaction(&epoch_change_tx).await?;

        let orig_tx: SenderSignedData = bcs::from_bytes(&tx_info.raw_transaction).unwrap();
        let tx_kind_orig = orig_tx.transaction_data().kind();

        if let TransactionKind::ChangeEpoch(change) = tx_kind_orig {
            return Ok(change.epoch_start_timestamp_ms);
        }
        Err(LocalExecError::InvalidEpochChangeTx { epoch: epoch_id })
    }

    async fn resolve_tx_components(
        &self,
        tx_digest: &TransactionDigest,
    ) -> Result<TxInfo, LocalExecError> {
        // Fetch full transaction content
        let tx_info = self.fetcher.get_transaction(tx_digest).await?;
        let sender = match tx_info.clone().transaction.unwrap().data {
            sui_json_rpc_types::SuiTransactionBlockData::V1(tx) => tx.sender,
        };
        let SuiTransactionBlockEffects::V1(effects) = tx_info.clone().effects.unwrap();

        let raw_tx_bytes = tx_info.clone().raw_transaction;
        let orig_tx: SenderSignedData = bcs::from_bytes(&raw_tx_bytes).unwrap();
        let input_objs = orig_tx
            .transaction_data()
            .input_objects()
            .map_err(|e| LocalExecError::UserInputError { err: e })?;
        let tx_kind_orig = orig_tx.transaction_data().kind();

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
            shared_object_refs: shared_obj_refs.iter().map(|r| r.to_object_ref()).collect(),
            gas: gas_object_refs,
            gas_budget: gas_data.budget,
            gas_price: gas_data.price,
            executed_epoch: epoch_id,
            dependencies: effects.dependencies().to_vec(),
            effects,
            // Find the protocol version for this epoch
            // This assumes we already initialized the protocol version table `protocol_version_epoch_table`
            protocol_config: self.get_protocol_config(epoch_id).await?,
            tx_digest: *tx_digest,
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
                    if let Some(o) = self.storage.live_objects_store.get(id) {
                        shared_inputs.push(o.clone());
                        Ok(())
                    } else {
                        Err(LocalExecError::InternalCacheInvariantViolation {
                            id: *id,
                            version: None,
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
                        self.storage
                            .package_cache
                            .lock()
                            .expect("Cannot lock")
                            .get(i)
                            .unwrap()
                            .clone(),
                    )
                }
                InputObjectKind::ImmOrOwnedMoveObject(o_ref) => (
                    *kind,
                    self.storage
                        .object_version_cache
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
                    (
                        *kind,
                        self.storage.live_objects_store.get(id).unwrap().clone(),
                    )
                }
            })
            .collect();

        Ok(resolved_input_objs)
    }

    /// Given the TxInfo, download and store the input objects, and other info necessary
    /// for execution
    async fn initialize_execution_env_state(
        &mut self,
        tx_info: &TxInfo,
    ) -> Result<Vec<(InputObjectKind, Object)>, LocalExecError> {
        // We need this for other activities in this session
        self.current_protocol_version = tx_info.protocol_config.version.as_u64();

        // Download the objects at the version right before the execution of this TX
        self.multi_download_and_store(&tx_info.modified_at_versions)
            .await?;

        // Download shared objects at the version right before the execution of this TX
        let shared_refs: Vec<_> = tx_info
            .shared_object_refs
            .iter()
            .map(|r| (r.0, r.1))
            .collect();
        self.multi_download_and_store(&shared_refs).await?;

        // Download gas (although this should already be in cache from modified at versions?)
        let gas_refs: Vec<_> = tx_info.gas.iter().map(|w| (w.0, w.1)).collect();
        self.multi_download_and_store(&gas_refs).await?;

        // Fetch the input objects we know from the raw transaction
        let input_objs = self.resolve_download_input_objects(tx_info).await?;

        // Prep the object runtime for dynamic fields
        // Download the child objects accessed at the version right before the execution of this TX
        let loaded_child_refs = self.fetch_loaded_child_refs(&tx_info.tx_digest).await?;
        self.multi_download_and_store(&loaded_child_refs).await?;

        Ok(input_objs)
    }
}

// <---------------------  Implement necessary traits for LocalExec to work with exec engine ----------------------->

impl BackingPackageStore for LocalExec {
    /// In this case we might need to download a dependency package which was not present in the
    /// modified at versions list because packages are immutable
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<Object>> {
        fn inner(self_: &LocalExec, package_id: &ObjectID) -> SuiResult<Option<Object>> {
            // If package not present fetch it from the network
            self_
                .get_or_download_object(package_id, true /* we expect a Move package*/)
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
    /// This uses `get_object`, which does not download from the network
    /// Hence all objects must be in store already
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
    /// The objects here much already exist in the store because we downloaded them earlier
    /// No download from network
    fn get_latest_parent_entry_ref(&self, object_id: ObjectID) -> SuiResult<Option<ObjectRef>> {
        fn inner(self_: &LocalExec, object_id: ObjectID) -> SuiResult<Option<ObjectRef>> {
            if let Some(v) = self_.storage.live_objects_store.get(&object_id) {
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

    /// In this case we might need to download a Move object on the fly which was not present in the
    /// modified at versions list because packages are immutable
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
            // If package not present fetch it from the network or some remote location
            let Some(object) = self_.get_or_download_object(
                &ObjectID::from(*address),false /* we expect a Move obj*/)? else {
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

    /// This fetches a module which must already be present in the store
    /// We do not download
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
    /// The object must be present in store by normal process we used to backfill store in init
    /// We dont download if not present
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError> {
        let res = Ok(self.storage.live_objects_store.get(object_id).cloned());
        self.exec_store_events
            .lock()
            .expect("Unable to lock events list")
            .push(ExecutionStoreEvent::ObjectStoreGetObject {
                object_id: *object_id,
                result: res.clone(),
            });
        res
    }

    /// The object must be present in store by normal process we used to backfill store in init
    /// We dont download if not present
    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>, SuiError> {
        let res = Ok(self
            .storage
            .live_objects_store
            .get(object_id)
            .and_then(|obj| {
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

// <--------------------- Util funcitons ----------------------->

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

    Err(LocalExecError::UnexpectedEventFormat { event: ev })
}

/// Utility ti diff effects in a human readable format
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
