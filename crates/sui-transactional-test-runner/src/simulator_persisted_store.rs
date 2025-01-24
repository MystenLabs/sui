// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};

use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::{language_storage::ModuleId, resolver::ModuleResolver};
use simulacrum::Simulacrum;
use std::num::NonZeroUsize;
use sui_config::genesis;
use sui_protocol_config::ProtocolVersion;
use sui_swarm_config::genesis_config::AccountConfig;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_types::storage::{ReadStore, RpcStateReader};
use sui_types::{
    base_types::{ObjectID, SequenceNumber, SuiAddress, VersionNumber},
    committee::{Committee, EpochId},
    crypto::AccountKeyPair,
    digests::{ObjectDigest, TransactionDigest, TransactionEventsDigest},
    effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents},
    error::{SuiError, UserInputError},
    messages_checkpoint::{
        CheckpointContents, CheckpointContentsDigest, CheckpointDigest, CheckpointSequenceNumber,
        VerifiedCheckpoint,
    },
    object::{Object, Owner},
    storage::{
        load_package_object_from_object_store, BackingPackageStore, ChildObjectResolver,
        ObjectStore, PackageObject, ParentSync,
    },
    transaction::VerifiedTransaction,
};
use tempfile::tempdir;
use typed_store::traits::TableSummary;
use typed_store::traits::TypedStoreDebug;
use typed_store::DBMapUtils;
use typed_store::Map;
use typed_store::{
    metrics::SamplingInterval,
    rocks::{DBMap, MetricConf},
};

use super::SimulatorStore;

pub struct PersistedStore {
    pub path: PathBuf,
    pub read_write: PersistedStoreInner,
}

pub struct PersistedStoreInnerReadOnlyWrapper {
    pub path: PathBuf,
    pub inner: PersistedStoreInnerReadOnly,
}

#[derive(Debug, DBMapUtils)]
pub struct PersistedStoreInner {
    // Checkpoint data
    checkpoints: DBMap<CheckpointSequenceNumber, sui_types::messages_checkpoint::TrustedCheckpoint>,
    checkpoint_digest_to_sequence_number: DBMap<CheckpointDigest, CheckpointSequenceNumber>,
    checkpoint_contents: DBMap<CheckpointContentsDigest, CheckpointContents>,

    // Transaction data
    transactions: DBMap<TransactionDigest, sui_types::transaction::TrustedTransaction>,
    effects: DBMap<TransactionDigest, TransactionEffects>,
    events: DBMap<TransactionEventsDigest, TransactionEvents>,
    events_tx_digest_index: DBMap<TransactionDigest, TransactionEventsDigest>,

    // Committee data
    epoch_to_committee: DBMap<(), Vec<Committee>>,

    // Object data
    live_objects: DBMap<ObjectID, SequenceNumber>,
    objects: DBMap<ObjectID, BTreeMap<SequenceNumber, Object>>,
}

impl PersistedStore {
    pub fn new(genesis: &genesis::Genesis, path: PathBuf) -> Self {
        let samp: SamplingInterval = SamplingInterval::new(Duration::from_secs(60), 0);
        let read_write = PersistedStoreInner::open_tables_read_write(
            path.clone(),
            MetricConf::new("persisted").with_sampling(samp.clone()),
            None,
            None,
        );

        let mut res = Self { path, read_write };
        res.init_with_genesis(genesis);

        res
    }

    pub fn read_replica(&self) -> PersistedStoreInnerReadOnlyWrapper {
        let samp: SamplingInterval = SamplingInterval::new(Duration::from_secs(60), 0);
        PersistedStoreInnerReadOnlyWrapper {
            path: self.path.clone(),
            inner: PersistedStoreInner::get_read_only_handle(
                self.path.clone(),
                None,
                None,
                MetricConf::new("persisted_readonly").with_sampling(samp),
            ),
        }
    }

    pub fn new_sim_replica_with_protocol_version_and_accounts<R>(
        mut rng: R,
        chain_start_timestamp_ms: u64,
        protocol_version: ProtocolVersion,
        account_configs: Vec<AccountConfig>,
        validator_keys: Option<Vec<AccountKeyPair>>,
        reference_gas_price: Option<u64>,
        path: Option<PathBuf>,
    ) -> (Simulacrum<R, Self>, PersistedStoreInnerReadOnlyWrapper)
    where
        R: rand::RngCore + rand::CryptoRng,
    {
        let path: PathBuf = path.unwrap_or(tempdir().unwrap().into_path());

        let mut builder = ConfigBuilder::new_with_temp_dir()
            .rng(&mut rng)
            .with_chain_start_timestamp_ms(chain_start_timestamp_ms)
            .deterministic_committee_size(NonZeroUsize::new(1).unwrap())
            .with_protocol_version(protocol_version)
            .with_accounts(account_configs);

        if let Some(validator_keys) = validator_keys {
            builder = builder.deterministic_committee_validators(validator_keys)
        };
        if let Some(reference_gas_price) = reference_gas_price {
            builder = builder.with_reference_gas_price(reference_gas_price)
        };

        let config = builder.build();

        let genesis = &config.genesis;

        let store = PersistedStore::new(genesis, path);
        let read_only_wrapper = store.read_replica();
        (
            Simulacrum::new_with_network_config_store(&config, rng, store),
            read_only_wrapper,
        )
    }

    pub fn new_sim_with_protocol_version_and_accounts<R>(
        rng: R,
        chain_start_timestamp_ms: u64,
        protocol_version: ProtocolVersion,
        account_configs: Vec<AccountConfig>,
        path: Option<PathBuf>,
    ) -> Simulacrum<R, Self>
    where
        R: rand::RngCore + rand::CryptoRng,
    {
        Self::new_sim_replica_with_protocol_version_and_accounts(
            rng,
            chain_start_timestamp_ms,
            protocol_version,
            account_configs,
            None,
            None,
            path,
        )
        .0
    }
}

impl SimulatorStore for PersistedStore {
    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        self.read_write
            .checkpoints
            .get(&sequence_number)
            .expect("Fatal: DB read failed")
            .map(|checkpoint| checkpoint.into())
    }

    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        self.read_write
            .checkpoint_digest_to_sequence_number
            .get(digest)
            .expect("Fatal: DB read failed")
            .and_then(|sequence_number| self.get_checkpoint_by_sequence_number(sequence_number))
    }

    fn get_highest_checkpint(&self) -> Option<VerifiedCheckpoint> {
        self.read_write
            .checkpoints
            .unbounded_iter()
            .skip_to_last()
            .next()
            .map(|(_, checkpoint)| checkpoint.into())
    }

    fn get_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        self.read_write
            .checkpoint_contents
            .get(digest)
            .expect("Fatal: DB read failed")
    }

    fn get_committee_by_epoch(&self, epoch: EpochId) -> Option<Committee> {
        self.read_write
            .epoch_to_committee
            .get(&())
            .expect("Fatal: DB read failed")
            .and_then(|committees| committees.get(epoch as usize).cloned())
    }

    fn get_transaction(&self, digest: &TransactionDigest) -> Option<VerifiedTransaction> {
        self.read_write
            .transactions
            .get(digest)
            .expect("Fatal: DB read failed")
            .map(|transaction| transaction.into())
    }

    fn get_transaction_effects(&self, digest: &TransactionDigest) -> Option<TransactionEffects> {
        self.read_write
            .effects
            .get(digest)
            .expect("Fatal: DB read failed")
    }

    fn get_transaction_events(
        &self,
        digest: &TransactionEventsDigest,
    ) -> Option<TransactionEvents> {
        self.read_write
            .events
            .get(digest)
            .expect("Fatal: DB read failed")
    }

    fn get_transaction_events_by_tx_digest(
        &self,
        tx_digest: &TransactionDigest,
    ) -> Option<TransactionEvents> {
        self.read_write
            .events_tx_digest_index
            .get(tx_digest)
            .expect("Fatal: DB read failed")
            .and_then(|x| {
                self.read_write
                    .events
                    .get(&x)
                    .expect("Fatal: DB read failed")
            })
    }

    fn get_object(&self, id: &ObjectID) -> Option<Object> {
        let version = self
            .read_write
            .live_objects
            .get(id)
            .expect("Fatal: DB read failed")?;
        self.get_object_at_version(id, version)
    }

    fn get_object_at_version(&self, id: &ObjectID, version: SequenceNumber) -> Option<Object> {
        self.read_write
            .objects
            .get(id)
            .expect("Fatal: DB read failed")
            .and_then(|versions| versions.get(&version).cloned())
    }

    fn get_system_state(&self) -> sui_types::sui_system_state::SuiSystemState {
        sui_types::sui_system_state::get_sui_system_state(self).expect("system state must exist")
    }

    fn get_clock(&self) -> sui_types::clock::Clock {
        SimulatorStore::get_object(self, &sui_types::SUI_CLOCK_OBJECT_ID)
            .expect("clock should exist")
            .to_rust()
            .expect("clock object should deserialize")
    }

    fn owned_objects(&self, owner: SuiAddress) -> Box<dyn Iterator<Item = Object> + '_> {
        Box::new(self.read_write.live_objects
            .unbounded_iter()
            .flat_map(|(id, version)| self.get_object_at_version(&id, version))
            .filter(
                move |object| matches!(object.owner, Owner::AddressOwner(addr) if addr == owner),
            ))
    }

    fn insert_checkpoint(&mut self, checkpoint: VerifiedCheckpoint) {
        self.read_write
            .checkpoint_digest_to_sequence_number
            .insert(checkpoint.digest(), checkpoint.sequence_number())
            .expect("Fatal: DB write failed");
        self.read_write
            .checkpoints
            .insert(checkpoint.sequence_number(), checkpoint.serializable_ref())
            .expect("Fatal: DB write failed");
    }

    fn insert_checkpoint_contents(&mut self, contents: CheckpointContents) {
        self.read_write
            .checkpoint_contents
            .insert(contents.digest(), &contents)
            .expect("Fatal: DB write failed");
    }

    fn insert_committee(&mut self, committee: Committee) {
        let epoch = committee.epoch as usize;

        let mut committees = self
            .read_write
            .epoch_to_committee
            .get(&())
            .expect("Fatal: DB read failed")
            .unwrap_or_default();

        if committees.get(epoch).is_some() {
            return;
        }

        if committees.len() == epoch {
            committees.push(committee);
        } else {
            panic!("committee was inserted into EpochCommitteeMap out of order");
        }
        self.read_write
            .epoch_to_committee
            .insert(&(), &committees)
            .expect("Fatal: DB write failed");
    }

    fn insert_executed_transaction(
        &mut self,
        transaction: VerifiedTransaction,
        effects: TransactionEffects,
        events: TransactionEvents,
        written_objects: BTreeMap<ObjectID, Object>,
    ) {
        let deleted_objects = effects.deleted();
        let tx_digest = *effects.transaction_digest();
        self.insert_transaction(transaction);
        self.insert_transaction_effects(effects);
        self.insert_events(&tx_digest, events);
        self.update_objects(written_objects, deleted_objects);
    }

    fn insert_transaction(&mut self, transaction: VerifiedTransaction) {
        self.read_write
            .transactions
            .insert(transaction.digest(), transaction.serializable_ref())
            .expect("Fatal: DB write failed");
    }

    fn insert_transaction_effects(&mut self, effects: TransactionEffects) {
        self.read_write
            .effects
            .insert(effects.transaction_digest(), &effects)
            .expect("Fatal: DB write failed");
    }

    fn insert_events(&mut self, tx_digest: &TransactionDigest, events: TransactionEvents) {
        self.read_write
            .events_tx_digest_index
            .insert(tx_digest, &events.digest())
            .expect("Fatal: DB write failed");
        self.read_write
            .events
            .insert(&events.digest(), &events)
            .expect("Fatal: DB write failed");
    }

    fn update_objects(
        &mut self,
        written_objects: BTreeMap<ObjectID, Object>,
        deleted_objects: Vec<(ObjectID, SequenceNumber, ObjectDigest)>,
    ) {
        for (object_id, _, _) in deleted_objects {
            self.read_write
                .live_objects
                .remove(&object_id)
                .expect("Fatal: DB write failed");
        }

        for (object_id, object) in written_objects {
            let version = object.version();
            self.read_write
                .live_objects
                .insert(&object_id, &version)
                .expect("Fatal: DB write failed");
            let mut q = self
                .read_write
                .objects
                .get(&object_id)
                .expect("Fatal: DB read failed")
                .unwrap_or_default();
            q.insert(version, object);
            self.read_write
                .objects
                .insert(&object_id, &q)
                .expect("Fatal: DB write failed");
        }
    }

    fn backing_store(&self) -> &dyn sui_types::storage::BackingStore {
        self
    }
}

impl BackingPackageStore for PersistedStore {
    fn get_package_object(
        &self,
        package_id: &ObjectID,
    ) -> sui_types::error::SuiResult<Option<PackageObject>> {
        load_package_object_from_object_store(self, package_id)
    }
}

impl ChildObjectResolver for PersistedStore {
    fn read_child_object(
        &self,
        parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> sui_types::error::SuiResult<Option<Object>> {
        let child_object = match SimulatorStore::get_object(self, child) {
            None => return Ok(None),
            Some(obj) => obj,
        };

        let parent = *parent;
        if child_object.owner != Owner::ObjectOwner(parent.into()) {
            return Err(SuiError::InvalidChildObjectAccess {
                object: *child,
                given_parent: parent,
                actual_owner: child_object.owner.clone(),
            });
        }

        if child_object.version() > child_version_upper_bound {
            return Err(SuiError::UnsupportedFeatureError {
                error: "TODO InMemoryStorage::read_child_object does not yet support bounded reads"
                    .to_owned(),
            });
        }

        Ok(Some(child_object))
    }

    fn get_object_received_at_version(
        &self,
        owner: &ObjectID,
        receiving_object_id: &ObjectID,
        receive_object_at_version: SequenceNumber,
        _epoch_id: EpochId,
        // TODO: Delete this parameter once table migration is complete.
        _use_object_per_epoch_marker_table_v2: bool,
    ) -> sui_types::error::SuiResult<Option<Object>> {
        let recv_object = match SimulatorStore::get_object(self, receiving_object_id) {
            None => return Ok(None),
            Some(obj) => obj,
        };
        if recv_object.owner != Owner::AddressOwner((*owner).into()) {
            return Ok(None);
        }

        if recv_object.version() != receive_object_at_version {
            return Ok(None);
        }
        Ok(Some(recv_object))
    }
}

impl GetModule for PersistedStore {
    type Error = SuiError;
    type Item = CompiledModule;

    fn get_module_by_id(&self, id: &ModuleId) -> Result<Option<Self::Item>, Self::Error> {
        Ok(self
            .get_module(id)?
            .map(|bytes| CompiledModule::deserialize_with_defaults(&bytes).unwrap()))
    }
}

impl ModuleResolver for PersistedStore {
    type Error = SuiError;

    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(self
            .get_package_object(&ObjectID::from(*module_id.address()))?
            .and_then(|package| {
                package
                    .move_package()
                    .serialized_module_map()
                    .get(module_id.name().as_str())
                    .cloned()
            }))
    }
}

impl ObjectStore for PersistedStore {
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        SimulatorStore::get_object(self, object_id)
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: sui_types::base_types::VersionNumber,
    ) -> Option<Object> {
        self.get_object_at_version(object_id, version)
    }
}

impl ParentSync for PersistedStore {
    fn get_latest_parent_entry_ref_deprecated(
        &self,
        _object_id: ObjectID,
    ) -> Option<sui_types::base_types::ObjectRef> {
        panic!("Never called in newer protocol versions")
    }
}

impl ObjectStore for PersistedStoreInnerReadOnlyWrapper {
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        self.sync();

        self.inner
            .live_objects
            .get(object_id)
            .expect("Fatal: DB read failed")
            .and_then(|version| self.get_object_by_key(object_id, version))
    }

    fn get_object_by_key(&self, object_id: &ObjectID, version: VersionNumber) -> Option<Object> {
        self.sync();

        self.inner
            .objects
            .get(object_id)
            .expect("Fatal: DB read failed")
            .and_then(|x| x.get(&version).cloned())
    }
}

impl ReadStore for PersistedStoreInnerReadOnlyWrapper {
    fn get_committee(&self, _epoch: EpochId) -> Option<std::sync::Arc<Committee>> {
        todo!()
    }

    fn get_latest_checkpoint(&self) -> sui_types::storage::error::Result<VerifiedCheckpoint> {
        self.sync();
        self.inner
            .checkpoints
            .unbounded_iter()
            .skip_to_last()
            .next()
            .map(|(_, checkpoint)| checkpoint.into())
            .ok_or(SuiError::UserInputError {
                error: UserInputError::LatestCheckpointSequenceNumberNotFound,
            })
            .map_err(sui_types::storage::error::Error::custom)
    }

    fn get_highest_verified_checkpoint(
        &self,
    ) -> sui_types::storage::error::Result<VerifiedCheckpoint> {
        todo!()
    }

    fn get_highest_synced_checkpoint(
        &self,
    ) -> sui_types::storage::error::Result<VerifiedCheckpoint> {
        todo!()
    }

    fn get_lowest_available_checkpoint(
        &self,
    ) -> sui_types::storage::error::Result<CheckpointSequenceNumber> {
        Ok(0)
    }

    fn get_checkpoint_by_digest(&self, _digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        todo!()
    }

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        self.sync();
        self.inner
            .checkpoints
            .get(&sequence_number)
            .expect("Fatal: DB read failed")
            .map(|checkpoint| checkpoint.into())
    }

    fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        self.sync();
        self.inner
            .checkpoint_contents
            .get(digest)
            .expect("Fatal: DB read failed")
    }

    fn get_checkpoint_contents_by_sequence_number(
        &self,
        _sequence_number: CheckpointSequenceNumber,
    ) -> Option<CheckpointContents> {
        todo!()
    }

    fn get_transaction(&self, tx_digest: &TransactionDigest) -> Option<Arc<VerifiedTransaction>> {
        self.sync();
        self.inner
            .transactions
            .get(tx_digest)
            .expect("Fatal: DB read failed")
            .map(|transaction| Arc::new(transaction.into()))
    }

    fn get_transaction_effects(&self, tx_digest: &TransactionDigest) -> Option<TransactionEffects> {
        self.sync();
        self.inner
            .effects
            .get(tx_digest)
            .expect("Fatal: DB read failed")
    }

    fn get_events(&self, event_digest: &TransactionEventsDigest) -> Option<TransactionEvents> {
        self.sync();
        self.inner
            .events
            .get(event_digest)
            .expect("Fatal: DB read failed")
    }

    fn get_full_checkpoint_contents_by_sequence_number(
        &self,
        _sequence_number: CheckpointSequenceNumber,
    ) -> Option<sui_types::messages_checkpoint::FullCheckpointContents> {
        todo!()
    }

    fn get_full_checkpoint_contents(
        &self,
        _digest: &CheckpointContentsDigest,
    ) -> Option<sui_types::messages_checkpoint::FullCheckpointContents> {
        todo!()
    }
}

impl RpcStateReader for PersistedStoreInnerReadOnlyWrapper {
    fn get_lowest_available_checkpoint_objects(
        &self,
    ) -> sui_types::storage::error::Result<CheckpointSequenceNumber> {
        Ok(0)
    }

    fn get_chain_identifier(
        &self,
    ) -> sui_types::storage::error::Result<sui_types::digests::ChainIdentifier> {
        Ok((*self.get_checkpoint_by_sequence_number(0).unwrap().digest()).into())
    }

    fn indexes(&self) -> Option<&dyn sui_types::storage::RpcIndexes> {
        None
    }
}

impl PersistedStoreInnerReadOnlyWrapper {
    pub fn sync(&self) {
        self.inner
            .try_catch_up_with_primary_all()
            .expect("Fatal: DB sync failed");
    }
}

impl Clone for PersistedStoreInnerReadOnlyWrapper {
    fn clone(&self) -> Self {
        let samp: SamplingInterval = SamplingInterval::new(Duration::from_secs(60), 0);
        PersistedStoreInnerReadOnlyWrapper {
            path: self.path.clone(),
            inner: PersistedStoreInner::get_read_only_handle(
                self.path.clone(),
                None,
                None,
                MetricConf::new("persisted_readonly").with_sampling(samp),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{rngs::StdRng, SeedableRng};

    #[tokio::test]
    async fn deterministic_genesis() {
        let rng = StdRng::from_seed([9; 32]);
        let chain1 = PersistedStore::new_sim_with_protocol_version_and_accounts(
            rng,
            0,
            ProtocolVersion::MAX,
            vec![],
            None,
        );
        let genesis_checkpoint_digest1 = *chain1
            .store()
            .get_checkpoint_by_sequence_number(0)
            .unwrap()
            .digest();

        let rng = StdRng::from_seed([9; 32]);
        let chain2 = PersistedStore::new_sim_with_protocol_version_and_accounts(
            rng,
            0,
            ProtocolVersion::MAX,
            vec![],
            None,
        );
        let genesis_checkpoint_digest2 = *chain2
            .store()
            .get_checkpoint_by_sequence_number(0)
            .unwrap()
            .digest();

        assert_eq!(genesis_checkpoint_digest1, genesis_checkpoint_digest2);

        // Ensure the committees are different when using different seeds
        let rng = StdRng::from_seed([0; 32]);
        let chain3 = PersistedStore::new_sim_with_protocol_version_and_accounts(
            rng,
            0,
            ProtocolVersion::MAX,
            vec![],
            None,
        );

        assert_ne!(
            chain1.store().get_committee_by_epoch(0),
            chain3.store().get_committee_by_epoch(0),
        );
    }
}
