// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use anyhow::anyhow;

use simulacrum::store::SimulatorStore;
use sui_protocol_config::Chain;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SuiAddress;
use sui_types::clock::Clock;
use sui_types::committee::Committee;
use sui_types::committee::EpochId;
use sui_types::digests::CheckpointContentsDigest;
use sui_types::digests::CheckpointDigest;
use sui_types::digests::ObjectDigest;
use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::effects::TransactionEvents;
use sui_types::error::SuiResult;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::object::Object;
use sui_types::storage::BackingPackageStore;
use sui_types::storage::BackingStore;
use sui_types::storage::ChildObjectResolver;
use sui_types::storage::ObjectStore;
use sui_types::storage::PackageObject;
use sui_types::storage::ParentSync;
use sui_types::storage::load_package_object_from_object_store;
use sui_types::sui_system_state::SuiSystemState;
use sui_types::transaction::VerifiedTransaction;

use crate::CheckpointRead;
use crate::GraphQLClient;
use crate::Node;
use crate::ObjectKey;
use crate::ObjectRead;
use crate::VersionQuery;
use crate::filesystem::FilesystemStore;

/// A data store for Sui data, combining a local filesystem cache with a remote GraphQL endpoint
/// for historical reads. Pre-fork data is fetched on demand and cached locally; post-fork data
/// (written by the executor) lives on disk only.
///
/// Implements [`SimulatorStore`] so it can be passed directly into
/// [`simulacrum::Simulacrum::new_from_custom_state`].
pub(crate) struct DataStore {
    forked_at_checkpoint: CheckpointSequenceNumber,
    gql: GraphQLClient,
    local: FilesystemStore,
}

impl DataStore {
    /// Create a new `DataStore` for the given network, anchored at `forked_at_checkpoint`.
    ///
    /// The local filesystem cache is rooted under a per-network, per-checkpoint directory
    /// (see [`FilesystemStore`]). The GraphQL client is constructed eagerly but no remote
    /// requests are made until reads happen.
    pub(crate) async fn new(
        node: Node,
        forked_at_checkpoint: CheckpointSequenceNumber,
        version: &str,
    ) -> Result<Self, anyhow::Error> {
        let gql = GraphQLClient::new(node.clone(), version)?;
        let local = FilesystemStore::new(&node, forked_at_checkpoint)?;

        Ok(Self {
            forked_at_checkpoint,
            gql,
            local,
        })
    }

    fn forked_at_checkpoint(&self) -> CheckpointSequenceNumber {
        self.forked_at_checkpoint
    }

    /// Return the chain (mainnet/testnet/devnet/unknown) this store is connected to.
    pub fn get_chain_identifier(&self) -> Chain {
        self.gql.chain()
    }

    /// Fetch a verified checkpoint from the remote GraphQL endpoint. When `checkpoint` is `None`,
    /// the store's `forked_at_checkpoint` is used as the default.
    pub(crate) async fn get_verified_checkpoint_from_rpc(
        &self,
        checkpoint: Option<CheckpointSequenceNumber>,
    ) -> anyhow::Result<Option<VerifiedCheckpoint>> {
        let checkpoint = checkpoint.unwrap_or(self.forked_at_checkpoint);
        let verified_checkpoint = self.gql.get_verified_checkpoint(Some(checkpoint))?;

        Ok(verified_checkpoint)
    }

    /// Get the object at the latest version available on disk. If not found, it will fetch the
    /// object at the forked checkpoint from remote rpc and save it to disk for future use. Returns
    /// `None` in the latter case.
    pub(crate) fn get_object(&self, object_id: &ObjectID) -> anyhow::Result<Option<Object>> {
        self.get_latest_object(object_id)
    }

    /// Get the object at the specified version. It will first try to load from disk, and if not
    /// found, it will fetch from remote rpc by making a query to fetch this version at the forked
    /// checkpoint. If none is found, it will return None. If the object is successfully fetched
    /// from remote rpc, it will be saved to disk for future use before returning the object.
    pub(crate) fn get_object_at_version(
        &self,
        object_id: &ObjectID,
        version: u64,
    ) -> anyhow::Result<Option<Object>> {
        if let Some(object) = self.local.get_object_at_version(object_id, version)? {
            return Ok(Some(object));
        }

        let object =
            self.get_object_from_remote(object_id, Some(version), self.forked_at_checkpoint())?;

        if let Some(ref object) = object {
            self.local.write_object(object)?;
        }

        Ok(object)
    }

    /// Local-first lookup for the latest known version of an object. Falls back to a remote
    /// `AtCheckpoint(forked_at_checkpoint)` query and caches the result on disk.
    fn get_latest_object(&self, object_id: &ObjectID) -> anyhow::Result<Option<Object>> {
        if let Some(object) = self.local.get_latest_object(object_id)? {
            return Ok(Some(object));
        }

        // if not found, load from remote rpc at forked checkpoint and save it to disk for future
        // use
        let object = self.get_object_from_remote(object_id, None, self.forked_at_checkpoint())?;

        if let Some(ref object) = object {
            self.local.write_object(object)?;
        }

        Ok(object)
    }

    /// Get the object at the specified checkpoint from remote rpc. If version is `None`, latest
    /// version at that checkpoint will be returned. Otherwise, the object at the specified version
    /// will be returned if it existed at that checkpoint.
    fn get_object_from_remote(
        &self,
        object_id: &ObjectID,
        version: Option<u64>,
        checkpoint: CheckpointSequenceNumber,
    ) -> anyhow::Result<Option<Object>> {
        let version_query = if let Some(version) = version {
            VersionQuery::VersionAtCheckpoint {
                version,
                checkpoint,
            }
        } else {
            VersionQuery::AtCheckpoint(checkpoint)
        };

        let objects = self.gql.get_objects(&[ObjectKey {
            object_id: *object_id,
            version_query,
        }])?;

        Ok(objects
            .into_iter()
            .next()
            .flatten()
            .map(|(object, _)| object))
    }

    /// Get the highest checkpoint sequence number available on disk.
    pub(crate) fn get_highest_checkpoint(&self) -> anyhow::Result<CheckpointSequenceNumber> {
        self.local.get_highest_checkpoint_sequence_number()
    }

    /// Construct a `DataStore` for tests, backed by an explicit local root and a fake (unused)
    /// GraphQL endpoint. The remote client is constructed but never called because tests should
    /// pre-populate the local cache with the data they need.
    #[cfg(test)]
    pub(crate) fn new_for_testing(root: std::path::PathBuf) -> Self {
        let gql = GraphQLClient::new(Node::Custom("http://localhost:1".to_string()), "test")
            .expect("graphql store with localhost url should construct");
        let local = FilesystemStore::new_with_root(root);
        Self {
            forked_at_checkpoint: 0,
            gql,
            local,
        }
    }
}

// ============================================================================
// SimulatorStore super-traits
// ============================================================================

/// Object reads delegate to the inherent `DataStore::get_object` / `get_object_at_version`,
/// which provide local-first lookups with remote fallback. Errors are swallowed and surfaced
/// as `None` because the trait signature does not allow propagating them.
impl ObjectStore for DataStore {
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        self.get_object(object_id).ok().flatten()
    }

    fn get_object_by_key(&self, object_id: &ObjectID, version: SequenceNumber) -> Option<Object> {
        self.get_object_at_version(object_id, version.value())
            .ok()
            .flatten()
    }
}

/// Package reads go through the standard `load_package_object_from_object_store` helper, which
/// validates that the resolved object is actually a Move package.
impl BackingPackageStore for DataStore {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        load_package_object_from_object_store(self, package_id)
    }
}

/// `ParentSync` is only required by older protocol versions and is never called by the executor
/// for the protocol versions we target. Calling it indicates a misconfiguration.
impl ParentSync for DataStore {
    fn get_latest_parent_entry_ref_deprecated(&self, _object_id: ObjectID) -> Option<ObjectRef> {
        panic!("Never called in newer protocol versions")
    }
}

impl ChildObjectResolver for DataStore {
    fn read_child_object(
        &self,
        parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        let child_object = match self.get_object(child).ok().flatten() {
            None => return Ok(None),
            Some(obj) => obj,
        };

        if child_object.owner != sui_types::object::Owner::ObjectOwner((*parent).into()) {
            return Err(sui_types::error::SuiErrorKind::InvalidChildObjectAccess {
                object: *child,
                given_parent: *parent,
                actual_owner: child_object.owner.clone(),
            }
            .into());
        }

        if child_object.version() > child_version_upper_bound {
            return Err(sui_types::error::SuiErrorKind::UnsupportedFeatureError {
                error: "DataStore::read_child_object does not yet support bounded reads".to_owned(),
            }
            .into());
        }

        Ok(Some(child_object))
    }

    fn get_object_received_at_version(
        &self,
        owner: &ObjectID,
        receiving_object_id: &ObjectID,
        receive_object_at_version: SequenceNumber,
        _epoch_id: EpochId,
    ) -> SuiResult<Option<Object>> {
        let Some(recv_object) = self.get_object(receiving_object_id).ok().flatten() else {
            return Ok(None);
        };
        if recv_object.owner != sui_types::object::Owner::AddressOwner((*owner).into()) {
            return Ok(None);
        }
        if recv_object.version() != receive_object_at_version {
            return Ok(None);
        }
        Ok(Some(recv_object))
    }
}

// ============================================================================
// SimulatorStore
// ============================================================================

impl SimulatorStore for DataStore {
    fn get_checkpoint_by_sequence_number(
        &self,
        _sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        todo!("SimulatorStore::get_checkpoint_by_sequence_number")
    }

    fn get_checkpoint_by_digest(&self, _digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        todo!("SimulatorStore::get_checkpoint_by_digest")
    }

    fn get_highest_checkpint(&self) -> Option<VerifiedCheckpoint> {
        todo!()
    }

    fn get_checkpoint_contents(
        &self,
        _digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        todo!("SimulatorStore::get_checkpoint_contents")
    }

    fn get_committee_by_epoch(&self, _epoch: EpochId) -> Option<Committee> {
        todo!("SimulatorStore::get_committee_by_epoch")
    }

    fn get_transaction(&self, digest: &TransactionDigest) -> Option<VerifiedTransaction> {
        self.local.get_transaction(digest).ok().flatten()
    }

    fn get_transaction_effects(&self, digest: &TransactionDigest) -> Option<TransactionEffects> {
        self.local.get_transaction_effects(digest).ok().flatten()
    }

    fn get_transaction_events(&self, digest: &TransactionDigest) -> Option<TransactionEvents> {
        self.local.get_transaction_events(digest).ok().flatten()
    }

    fn get_object(&self, id: &ObjectID) -> Option<Object> {
        self.get_object(id).ok().flatten()
    }

    fn get_object_at_version(&self, id: &ObjectID, version: SequenceNumber) -> Option<Object> {
        self.get_object_at_version(id, version.value())
            .ok()
            .flatten()
    }

    fn get_system_state(&self) -> SuiSystemState {
        sui_types::sui_system_state::get_sui_system_state(self).expect("system state must exist")
    }

    fn get_clock(&self) -> Clock {
        self.get_object(&sui_types::SUI_CLOCK_OBJECT_ID)
            .ok()
            .flatten()
            .expect("clock should exist")
            .to_rust()
            .expect("clock object should deserialize")
    }

    fn owned_objects(&self, _owner: SuiAddress) -> Box<dyn Iterator<Item = Object> + '_> {
        todo!("SimulatorStore::owned_objects")
    }

    fn insert_checkpoint(&mut self, _checkpoint: VerifiedCheckpoint) {
        todo!("SimulatorStore::insert_checkpoint")
    }

    fn insert_checkpoint_contents(&mut self, _contents: CheckpointContents) {
        todo!("SimulatorStore::insert_checkpoint_contents")
    }

    fn insert_committee(&mut self, _committee: Committee) {
        todo!("SimulatorStore::insert_committee")
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
        let digest = *transaction.digest();
        self.local
            .write_transaction(&digest, &transaction)
            .expect("failed to persist transaction to disk");
    }

    fn insert_transaction_effects(&mut self, effects: TransactionEffects) {
        let digest = *effects.transaction_digest();
        self.local
            .write_transaction_effects(&digest, &effects)
            .expect("failed to persist transaction effects to disk");
    }

    fn insert_events(&mut self, tx_digest: &TransactionDigest, events: TransactionEvents) {
        self.local
            .write_transaction_events(tx_digest, &events)
            .expect("failed to persist transaction events to disk");
    }

    fn update_objects(
        &mut self,
        written_objects: BTreeMap<ObjectID, Object>,
        _deleted_objects: Vec<(ObjectID, SequenceNumber, ObjectDigest)>,
    ) {
        for object in written_objects.values() {
            self.local
                .write_object(object)
                .expect("failed to write object to disk");
        }
    }

    fn backing_store(&self) -> &dyn BackingStore {
        self
    }
}

#[cfg(test)]
mod execution_tests {
    use std::num::NonZeroUsize;
    use std::time::Duration;

    use rand::rngs::OsRng;
    use simulacrum::Simulacrum;
    use simulacrum::store::in_mem_store::KeyStore;
    use sui_swarm_config::network_config::NetworkConfig;
    use sui_swarm_config::network_config_builder::ConfigBuilder;
    use sui_types::base_types::SuiAddress;
    use sui_types::effects::TransactionEffectsAPI;
    use sui_types::gas_coin::GasCoin;
    use sui_types::object::Owner;
    use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
    use sui_types::transaction::{GasData, Transaction, TransactionData, TransactionKind};

    use super::*;
    use sui_types::crypto::KeypairTraits;

    /// Build a `Simulacrum<OsRng, DataStore>` from a fresh genesis NetworkConfig. The DataStore's
    /// local cache lives in the returned tempdir; its remote endpoint is fake and never called.
    /// Genesis objects are populated directly via `update_objects` to avoid touching the
    /// `init_with_genesis` checkpoint/committee paths (which are still `todo!()`).
    ///
    /// Returns the simulacrum, the underlying NetworkConfig (so tests can find genesis objects
    /// and account keys), and the tempdir guarding the local cache.
    fn test_simulacrum() -> (
        Simulacrum<OsRng, DataStore>,
        NetworkConfig,
        tempfile::TempDir,
    ) {
        let temp = tempfile::tempdir().expect("failed to create tempdir");
        let mut rng = OsRng;
        let config = ConfigBuilder::new_with_temp_dir()
            .rng(&mut rng)
            .deterministic_committee_size(NonZeroUsize::MIN)
            .build();

        let mut data_store = DataStore::new_for_testing(temp.path().to_path_buf());
        let written: BTreeMap<ObjectID, Object> = config
            .genesis
            .objects()
            .iter()
            .map(|o| (o.id(), o.clone()))
            .collect();
        data_store.update_objects(written, vec![]);

        let keystore = KeyStore::from_network_config(&config);
        let sim = Simulacrum::new_from_custom_state(
            keystore,
            config.genesis.checkpoint(),
            config.genesis.sui_system_object(),
            &config,
            data_store,
            rng,
        );
        (sim, config, temp)
    }

    /// Find the first gas coin in the genesis object set owned by `owner`.
    fn find_gas_coin(config: &NetworkConfig, owner: SuiAddress) -> Object {
        config
            .genesis
            .objects()
            .iter()
            .find(|obj| obj.owner == Owner::AddressOwner(owner) && obj.is_gas_coin())
            .expect("owner should have a gas coin in genesis")
            .clone()
    }

    #[test]
    fn test_advance_clock_executes_and_persists() {
        let (mut sim, _config, _temp) = test_simulacrum();
        let initial_ts = sim.store().get_clock().timestamp_ms;

        let effects = sim.advance_clock(Duration::from_secs(60));
        assert!(
            effects.status().is_ok(),
            "execution failed: {:?}",
            effects.status()
        );

        assert_eq!(sim.store().get_clock().timestamp_ms, initial_ts + 60_000,);

        // The transaction was persisted to the filesystem cache.
        let tx_digest = effects.transaction_digest();
        let persisted = sim.store().get_transaction(tx_digest);
        assert!(persisted.is_some(), "transaction not persisted on disk");

        let persisted_effects = sim.store().get_transaction_effects(tx_digest);
        assert_eq!(persisted_effects.unwrap(), effects);
    }

    #[test]
    fn test_transfer_sui_executes_and_persists() {
        let (mut sim, config, _temp) = test_simulacrum();

        // Pick a sender from the genesis keystore and a gas coin owned by the sender.
        let (sender, sender_key) = {
            let (addr, key) = sim
                .keystore()
                .accounts()
                .next()
                .expect("at least one account");
            (*addr, key.copy())
        };
        let gas_object = find_gas_coin(&config, sender);
        let gas_coin = GasCoin::try_from(&gas_object).unwrap();
        let initial_balance = gas_coin.value();
        let transfer_amount = initial_balance / 2;

        let recipient = SuiAddress::random_for_testing_only();

        // Build a transfer-SUI programmable transaction.
        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            builder.transfer_sui(recipient, Some(transfer_amount));
            builder.finish()
        };
        let tx_data = TransactionData::new_with_gas_data(
            TransactionKind::ProgrammableTransaction(pt),
            sender,
            GasData {
                payment: vec![gas_object.compute_object_reference()],
                owner: sender,
                price: sim.reference_gas_price(),
                budget: 100_000_000,
            },
        );

        // Sign with the real account key from the genesis keystore.
        let tx = Transaction::from_data_and_signer(tx_data, vec![&sender_key]);

        let (effects, exec_error) = sim.execute_transaction(tx).unwrap();
        assert!(
            effects.status().is_ok(),
            "transfer failed: status={:?} exec_error={:?}",
            effects.status(),
            exec_error,
        );

        // The transaction is persisted on disk.
        let tx_digest = effects.transaction_digest();
        assert!(
            sim.store().get_transaction(tx_digest).is_some(),
            "transaction not persisted on disk",
        );
        assert_eq!(
            sim.store().get_transaction_effects(tx_digest).unwrap(),
            effects,
        );

        // The recipient now owns a gas coin holding exactly `transfer_amount`.
        let recipient_coin = effects
            .created()
            .into_iter()
            .find_map(|((id, _, _), owner)| (owner == Owner::AddressOwner(recipient)).then_some(id))
            .expect("transfer should create a coin owned by the recipient");
        let recipient_obj = sim
            .store()
            .get_object(&recipient_coin)
            .expect("recipient coin lookup failed")
            .expect("recipient coin should be readable from the store");
        let recipient_gas = GasCoin::try_from(&recipient_obj).unwrap();
        assert_eq!(recipient_gas.value(), transfer_amount);

        // The sender's gas coin still exists, charged for gas, balance reduced by transfer_amount + net gas.
        let updated_gas_obj = sim
            .store()
            .get_object(&gas_object.id())
            .expect("sender gas coin lookup failed")
            .expect("sender gas coin should still exist");
        let updated_gas = GasCoin::try_from(&updated_gas_obj).unwrap();
        let net_gas = effects.gas_cost_summary().net_gas_usage();
        let expected = (initial_balance as i64 - transfer_amount as i64 - net_gas) as u64;
        assert_eq!(updated_gas.value(), expected);
    }
}
