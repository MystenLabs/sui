// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! A `Simulacrum` of Sui.
//!
//! The word simulacrum is latin for "likeness, semblance", it is also a spell in D&D which creates
//! a copy of a creature which then follows the player's commands and wishes. As such this crate
//! provides the [`Simulacrum`] type which is a implementation or instantiation of a sui
//! blockchain, one which doesn't do anything unless acted upon.
//!
//! [`Simulacrum`]: crate::Simulacrum

use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use fastcrypto::traits::Signer;
use rand::rngs::OsRng;
use sui_config::verifier_signing_config::VerifierSigningConfig;
use sui_config::{genesis, transaction_deny_config::TransactionDenyConfig};
use sui_protocol_config::ProtocolVersion;
use sui_storage::blob::{Blob, BlobEncoding};
use sui_swarm_config::genesis_config::AccountConfig;
use sui_swarm_config::network_config::NetworkConfig;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_types::base_types::{AuthorityName, ObjectID, VersionNumber};
use sui_types::crypto::AuthoritySignature;
use sui_types::digests::ConsensusCommitDigest;
use sui_types::messages_consensus::ConsensusDeterminedVersionAssignments;
use sui_types::object::Object;
use sui_types::storage::{ObjectStore, ReadStore, RpcStateReader};
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemState;
use sui_types::transaction::EndOfEpochTransactionKind;
use sui_types::{
    base_types::SuiAddress,
    committee::Committee,
    effects::TransactionEffects,
    error::ExecutionError,
    gas_coin::MIST_PER_SUI,
    inner_temporary_store::InnerTemporaryStore,
    messages_checkpoint::{EndOfEpochData, VerifiedCheckpoint},
    signature::VerifyParams,
    transaction::{Transaction, VerifiedTransaction},
};

use self::epoch_state::EpochState;
pub use self::store::in_mem_store::InMemoryStore;
use self::store::in_mem_store::KeyStore;
pub use self::store::SimulatorStore;
use sui_types::messages_checkpoint::{CheckpointContents, CheckpointSequenceNumber};
use sui_types::mock_checkpoint_builder::{MockCheckpointBuilder, ValidatorKeypairProvider};
use sui_types::{
    gas_coin::GasCoin,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{GasData, TransactionData, TransactionKind},
};

mod epoch_state;
pub mod store;

/// A `Simulacrum` of Sui.
///
/// This type represents a simulated instantiation of a Sui blockchain that needs to be driven
/// manually, that is time doesn't advance and checkpoints are not formed unless explicitly
/// requested.
///
/// See [module level][mod] documentation for more details.
///
/// [mod]: index.html
pub struct Simulacrum<R = OsRng, Store: SimulatorStore = InMemoryStore> {
    rng: R,
    keystore: KeyStore,
    #[allow(unused)]
    genesis: genesis::Genesis,
    store: Store,
    checkpoint_builder: MockCheckpointBuilder,

    // Epoch specific data
    epoch_state: EpochState,

    // Other
    deny_config: TransactionDenyConfig,
    data_ingestion_path: Option<PathBuf>,
    verifier_signing_config: VerifierSigningConfig,
}

impl Simulacrum {
    /// Create a new, random Simulacrum instance using an `OsRng` as the source of randomness.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self::new_with_rng(OsRng)
    }
}

impl<R> Simulacrum<R>
where
    R: rand::RngCore + rand::CryptoRng,
{
    /// Create a new Simulacrum instance using the provided `rng`.
    ///
    /// This allows you to create a fully deterministic initial chainstate when a seeded rng is
    /// used.
    ///
    /// ```
    /// use simulacrum::Simulacrum;
    /// use rand::{SeedableRng, rngs::StdRng};
    ///
    /// # fn main() {
    /// let mut rng = StdRng::seed_from_u64(1);
    /// let simulacrum = Simulacrum::new_with_rng(rng);
    /// # }
    /// ```
    pub fn new_with_rng(mut rng: R) -> Self {
        let config = ConfigBuilder::new_with_temp_dir()
            .rng(&mut rng)
            .with_chain_start_timestamp_ms(1)
            .deterministic_committee_size(NonZeroUsize::new(1).unwrap())
            .build();
        Self::new_with_network_config_in_mem(&config, rng)
    }

    pub fn new_with_protocol_version_and_accounts(
        mut rng: R,
        chain_start_timestamp_ms: u64,
        protocol_version: ProtocolVersion,
        account_configs: Vec<AccountConfig>,
    ) -> Self {
        let config = ConfigBuilder::new_with_temp_dir()
            .rng(&mut rng)
            .with_chain_start_timestamp_ms(chain_start_timestamp_ms)
            .deterministic_committee_size(NonZeroUsize::new(1).unwrap())
            .with_protocol_version(protocol_version)
            .with_accounts(account_configs)
            .build();
        Self::new_with_network_config_in_mem(&config, rng)
    }

    fn new_with_network_config_in_mem(config: &NetworkConfig, rng: R) -> Self {
        let store = InMemoryStore::new(&config.genesis);
        Self::new_with_network_config_store(config, rng, store)
    }
}

impl<R, S: store::SimulatorStore> Simulacrum<R, S> {
    pub fn new_with_network_config_store(config: &NetworkConfig, rng: R, store: S) -> Self {
        let keystore = KeyStore::from_network_config(config);
        let checkpoint_builder = MockCheckpointBuilder::new(config.genesis.checkpoint());

        let genesis = &config.genesis;
        let epoch_state = EpochState::new(genesis.sui_system_object());

        Self {
            rng,
            keystore,
            genesis: genesis.clone(),
            store,
            checkpoint_builder,
            epoch_state,
            deny_config: TransactionDenyConfig::default(),
            verifier_signing_config: VerifierSigningConfig::default(),
            data_ingestion_path: None,
        }
    }

    /// Attempts to execute the provided Transaction.
    ///
    /// The provided Transaction undergoes the same types of checks that a Validator does prior to
    /// signing and executing in the production system. Some of these checks are as follows:
    /// - User signature is valid
    /// - Sender owns all OwnedObject inputs
    /// - etc
    ///
    /// If the above checks are successful then the transaction is immediately executed, enqueued
    /// to be included in the next checkpoint (the next time `create_checkpoint` is called) and the
    /// corresponding TransactionEffects are returned.
    pub fn execute_transaction(
        &mut self,
        transaction: Transaction,
    ) -> anyhow::Result<(TransactionEffects, Option<ExecutionError>)> {
        let transaction = transaction
            .try_into_verified_for_testing(self.epoch_state.epoch(), &VerifyParams::default())?;

        let (inner_temporary_store, _, effects, execution_error_opt) =
            self.epoch_state.execute_transaction(
                &self.store,
                &self.deny_config,
                &self.verifier_signing_config,
                &transaction,
            )?;

        let InnerTemporaryStore {
            written, events, ..
        } = inner_temporary_store;

        self.store.insert_executed_transaction(
            transaction.clone(),
            effects.clone(),
            events,
            written,
        );

        // Insert into checkpoint builder
        self.checkpoint_builder
            .push_transaction(transaction, effects.clone());
        Ok((effects, execution_error_opt.err()))
    }

    /// Creates the next Checkpoint using the Transactions enqueued since the last checkpoint was
    /// created.
    pub fn create_checkpoint(&mut self) -> VerifiedCheckpoint {
        let committee = CommitteeWithKeys::new(&self.keystore, self.epoch_state.committee());
        let (checkpoint, contents, _) = self
            .checkpoint_builder
            .build(&committee, self.store.get_clock().timestamp_ms());
        self.store.insert_checkpoint(checkpoint.clone());
        self.store.insert_checkpoint_contents(contents.clone());
        self.process_data_ingestion(checkpoint.clone(), contents)
            .unwrap();
        checkpoint
    }

    /// Advances the clock by `duration`.
    ///
    /// This creates and executes a ConsensusCommitPrologue transaction which advances the chain
    /// Clock by the provided duration.
    pub fn advance_clock(&mut self, duration: std::time::Duration) -> TransactionEffects {
        let epoch = self.epoch_state.epoch();
        let round = self.epoch_state.next_consensus_round();
        let timestamp_ms = self.store.get_clock().timestamp_ms() + duration.as_millis() as u64;

        let consensus_commit_prologue_transaction =
            VerifiedTransaction::new_consensus_commit_prologue_v3(
                epoch,
                round,
                timestamp_ms,
                ConsensusCommitDigest::default(),
                ConsensusDeterminedVersionAssignments::empty_for_testing(),
            );

        self.execute_transaction(consensus_commit_prologue_transaction.into())
            .expect("advancing the clock cannot fail")
            .0
    }

    /// Advances the epoch.
    ///
    /// This creates and executes an EndOfEpoch transaction which advances the chain into the next
    /// epoch. Since it is required to be the final transaction in an epoch, the final checkpoint in
    /// the epoch is also created.
    ///
    /// create_random_state controls whether a `RandomStateCreate` end of epoch transaction is
    /// included as part of this epoch change (to initialise on-chain randomness for the first
    /// time).
    ///
    /// NOTE: This function does not currently support updating the protocol version or the system
    /// packages
    pub fn advance_epoch(&mut self, create_random_state: bool) {
        let next_epoch = self.epoch_state.epoch() + 1;
        let next_epoch_protocol_version = self.epoch_state.protocol_version();
        let gas_cost_summary = self.checkpoint_builder.epoch_rolling_gas_cost_summary();
        let epoch_start_timestamp_ms = self.store.get_clock().timestamp_ms();
        let next_epoch_system_package_bytes = vec![];

        let mut kinds = vec![];

        if create_random_state {
            kinds.push(EndOfEpochTransactionKind::new_randomness_state_create());
        }

        kinds.push(EndOfEpochTransactionKind::new_change_epoch(
            next_epoch,
            next_epoch_protocol_version,
            gas_cost_summary.storage_cost,
            gas_cost_summary.computation_cost,
            gas_cost_summary.storage_rebate,
            gas_cost_summary.non_refundable_storage_fee,
            epoch_start_timestamp_ms,
            next_epoch_system_package_bytes,
        ));

        let tx = VerifiedTransaction::new_end_of_epoch_transaction(kinds);
        self.execute_transaction(tx.into())
            .expect("advancing the epoch cannot fail");

        let new_epoch_state = EpochState::new(self.store.get_system_state());
        let end_of_epoch_data = EndOfEpochData {
            next_epoch_committee: new_epoch_state.committee().voting_rights.clone(),
            next_epoch_protocol_version,
            epoch_commitments: vec![],
        };
        let committee = CommitteeWithKeys::new(&self.keystore, self.epoch_state.committee());
        let (checkpoint, contents, _) = self.checkpoint_builder.build_end_of_epoch(
            &committee,
            self.store.get_clock().timestamp_ms(),
            next_epoch,
            end_of_epoch_data,
        );

        self.store.insert_checkpoint(checkpoint.clone());
        self.store.insert_checkpoint_contents(contents.clone());
        self.process_data_ingestion(checkpoint, contents).unwrap();
        self.epoch_state = new_epoch_state;
    }

    pub fn store(&self) -> &dyn SimulatorStore {
        &self.store
    }

    pub fn keystore(&self) -> &KeyStore {
        &self.keystore
    }

    pub fn epoch_start_state(&self) -> &EpochStartSystemState {
        self.epoch_state.epoch_start_state()
    }

    /// Return a handle to the internally held RNG.
    ///
    /// Returns a handle to the RNG used to create this Simulacrum for use as a source of
    /// randomness. Using a seeded RNG to build a Simulacrum and then utilizing the stored RNG as a
    /// source of randomness can lead to a fully deterministic chain evolution.
    pub fn rng(&mut self) -> &mut R {
        &mut self.rng
    }

    /// Return the reference gas price for the current epoch
    pub fn reference_gas_price(&self) -> u64 {
        self.epoch_state.reference_gas_price()
    }

    /// Request that `amount` Mist be sent to `address` from a faucet account.
    ///
    /// ```
    /// use simulacrum::Simulacrum;
    /// use sui_types::base_types::SuiAddress;
    /// use sui_types::gas_coin::MIST_PER_SUI;
    ///
    /// # fn main() {
    /// let mut simulacrum = Simulacrum::new();
    /// let address = SuiAddress::generate(simulacrum.rng());
    /// simulacrum.request_gas(address, MIST_PER_SUI).unwrap();
    ///
    /// // `account` now has a Coin<SUI> object with single SUI in it.
    /// // ...
    /// # }
    /// ```
    pub fn request_gas(&mut self, address: SuiAddress, amount: u64) -> Result<TransactionEffects> {
        // For right now we'll just use the first account as the `faucet` account. We may want to
        // explicitly cordon off the faucet account from the rest of the accounts though.
        let (sender, key) = self.keystore().accounts().next().unwrap();
        let object = self
            .store()
            .owned_objects(*sender)
            .find(|object| {
                object.is_gas_coin() && object.get_coin_value_unsafe() > amount + MIST_PER_SUI
            })
            .ok_or_else(|| {
                anyhow!("unable to find a coin with enough to satisfy request for {amount} Mist")
            })?;

        let gas_data = sui_types::transaction::GasData {
            payment: vec![object.compute_object_reference()],
            owner: *sender,
            price: self.reference_gas_price(),
            budget: MIST_PER_SUI,
        };

        let pt = {
            let mut builder =
                sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder::new();
            builder.transfer_sui(address, Some(amount));
            builder.finish()
        };

        let kind = sui_types::transaction::TransactionKind::ProgrammableTransaction(pt);
        let tx_data =
            sui_types::transaction::TransactionData::new_with_gas_data(kind, *sender, gas_data);
        let tx = Transaction::from_data_and_signer(tx_data, vec![key]);

        self.execute_transaction(tx).map(|x| x.0)
    }

    pub fn set_data_ingestion_path(&mut self, data_ingestion_path: PathBuf) {
        self.data_ingestion_path = Some(data_ingestion_path);
        let checkpoint = self.store.get_checkpoint_by_sequence_number(0).unwrap();
        let contents = self
            .store
            .get_checkpoint_contents(&checkpoint.content_digest);
        self.process_data_ingestion(checkpoint, contents.unwrap())
            .unwrap();
    }

    pub fn override_next_checkpoint_number(&mut self, number: CheckpointSequenceNumber) {
        let committee = CommitteeWithKeys::new(&self.keystore, self.epoch_state.committee());
        self.checkpoint_builder
            .override_next_checkpoint_number(number, &committee);
    }

    fn process_data_ingestion(
        &self,
        checkpoint: VerifiedCheckpoint,
        checkpoint_contents: CheckpointContents,
    ) -> anyhow::Result<()> {
        if let Some(path) = &self.data_ingestion_path {
            let file_name = format!("{}.chk", checkpoint.sequence_number);
            let checkpoint_data = self.get_checkpoint_data(checkpoint, checkpoint_contents)?;
            std::fs::create_dir_all(path)?;
            let blob = Blob::encode(&checkpoint_data, BlobEncoding::Bcs)?;
            std::fs::write(path.join(file_name), blob.to_bytes())?;
        }
        Ok(())
    }
}

pub struct CommitteeWithKeys<'a> {
    keystore: &'a KeyStore,
    committee: &'a Committee,
}

impl<'a> CommitteeWithKeys<'a> {
    fn new(keystore: &'a KeyStore, committee: &'a Committee) -> Self {
        Self {
            keystore,
            committee,
        }
    }

    pub fn keystore(&self) -> &KeyStore {
        self.keystore
    }
}

impl ValidatorKeypairProvider for CommitteeWithKeys<'_> {
    fn get_validator_key(&self, name: &AuthorityName) -> &dyn Signer<AuthoritySignature> {
        self.keystore.validator(name).unwrap()
    }

    fn get_committee(&self) -> &Committee {
        self.committee
    }
}

impl<T, V: store::SimulatorStore> ObjectStore for Simulacrum<T, V> {
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        store::SimulatorStore::get_object(&self.store, object_id)
    }

    fn get_object_by_key(&self, object_id: &ObjectID, version: VersionNumber) -> Option<Object> {
        self.store.get_object_by_key(object_id, version)
    }
}

impl<T, V: store::SimulatorStore> ReadStore for Simulacrum<T, V> {
    fn get_committee(
        &self,
        _epoch: sui_types::committee::EpochId,
    ) -> Option<std::sync::Arc<Committee>> {
        todo!()
    }

    fn get_latest_checkpoint(&self) -> sui_types::storage::error::Result<VerifiedCheckpoint> {
        Ok(self.store().get_highest_checkpint().unwrap())
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
    ) -> sui_types::storage::error::Result<sui_types::messages_checkpoint::CheckpointSequenceNumber>
    {
        // TODO wire this up to the underlying sim store, for now this will work since we never
        // prune the sim store
        Ok(0)
    }

    fn get_checkpoint_by_digest(
        &self,
        digest: &sui_types::messages_checkpoint::CheckpointDigest,
    ) -> Option<VerifiedCheckpoint> {
        self.store().get_checkpoint_by_digest(digest)
    }

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: sui_types::messages_checkpoint::CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        self.store()
            .get_checkpoint_by_sequence_number(sequence_number)
    }

    fn get_checkpoint_contents_by_digest(
        &self,
        digest: &sui_types::messages_checkpoint::CheckpointContentsDigest,
    ) -> Option<sui_types::messages_checkpoint::CheckpointContents> {
        self.store().get_checkpoint_contents(digest)
    }

    fn get_checkpoint_contents_by_sequence_number(
        &self,
        _sequence_number: sui_types::messages_checkpoint::CheckpointSequenceNumber,
    ) -> Option<sui_types::messages_checkpoint::CheckpointContents> {
        todo!()
    }

    fn get_transaction(
        &self,
        tx_digest: &sui_types::digests::TransactionDigest,
    ) -> Option<Arc<VerifiedTransaction>> {
        self.store().get_transaction(tx_digest).map(Arc::new)
    }

    fn get_transaction_effects(
        &self,
        tx_digest: &sui_types::digests::TransactionDigest,
    ) -> Option<TransactionEffects> {
        self.store().get_transaction_effects(tx_digest)
    }

    fn get_events(
        &self,
        event_digest: &sui_types::digests::TransactionEventsDigest,
    ) -> Option<sui_types::effects::TransactionEvents> {
        self.store().get_transaction_events(event_digest)
    }

    fn get_full_checkpoint_contents_by_sequence_number(
        &self,
        _sequence_number: sui_types::messages_checkpoint::CheckpointSequenceNumber,
    ) -> Option<sui_types::messages_checkpoint::FullCheckpointContents> {
        todo!()
    }

    fn get_full_checkpoint_contents(
        &self,
        _digest: &sui_types::messages_checkpoint::CheckpointContentsDigest,
    ) -> Option<sui_types::messages_checkpoint::FullCheckpointContents> {
        todo!()
    }
}

impl<T: Send + Sync, V: store::SimulatorStore + Send + Sync> RpcStateReader for Simulacrum<T, V> {
    fn get_lowest_available_checkpoint_objects(
        &self,
    ) -> sui_types::storage::error::Result<CheckpointSequenceNumber> {
        Ok(0)
    }

    fn get_chain_identifier(
        &self,
    ) -> sui_types::storage::error::Result<sui_types::digests::ChainIdentifier> {
        Ok(self
            .store()
            .get_checkpoint_by_sequence_number(0)
            .unwrap()
            .digest()
            .to_owned()
            .into())
    }

    fn indexes(&self) -> Option<&dyn sui_types::storage::RpcIndexes> {
        None
    }
}

impl Simulacrum {
    /// Generate a random transfer transaction.
    /// TODO: This is here today to make it easier to write tests. But we should utilize all the
    /// existing code for generating transactions in sui-test-transaction-builder by defining a trait
    /// that both WalletContext and Simulacrum implement. Then we can remove this function.
    pub fn transfer_txn(&mut self, recipient: SuiAddress) -> (Transaction, u64) {
        let (sender, key) = self.keystore().accounts().next().unwrap();
        let sender = *sender;

        let object = self
            .store()
            .owned_objects(sender)
            .find(|object| object.is_gas_coin())
            .unwrap();
        let gas_coin = GasCoin::try_from(&object).unwrap();
        let transfer_amount = gas_coin.value() / 2;

        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            builder.transfer_sui(recipient, Some(transfer_amount));
            builder.finish()
        };

        let kind = TransactionKind::ProgrammableTransaction(pt);
        let gas_data = GasData {
            payment: vec![object.compute_object_reference()],
            owner: sender,
            price: self.reference_gas_price(),
            budget: 1_000_000_000,
        };
        let tx_data = TransactionData::new_with_gas_data(kind, sender, gas_data);
        let tx = Transaction::from_data_and_signer(tx_data, vec![key]);
        (tx, transfer_amount)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use rand::{rngs::StdRng, SeedableRng};
    use sui_types::{
        base_types::SuiAddress, effects::TransactionEffectsAPI, gas_coin::GasCoin,
        transaction::TransactionDataAPI,
    };

    use super::*;

    #[test]
    fn deterministic_genesis() {
        let rng = StdRng::from_seed([9; 32]);
        let chain1 = Simulacrum::new_with_rng(rng);
        let genesis_checkpoint_digest1 = *chain1
            .store()
            .get_checkpoint_by_sequence_number(0)
            .unwrap()
            .digest();

        let rng = StdRng::from_seed([9; 32]);
        let chain2 = Simulacrum::new_with_rng(rng);
        let genesis_checkpoint_digest2 = *chain2
            .store()
            .get_checkpoint_by_sequence_number(0)
            .unwrap()
            .digest();

        assert_eq!(genesis_checkpoint_digest1, genesis_checkpoint_digest2);

        // Ensure the committees are different when using different seeds
        let rng = StdRng::from_seed([0; 32]);
        let chain3 = Simulacrum::new_with_rng(rng);

        assert_ne!(
            chain1.store().get_committee_by_epoch(0),
            chain3.store().get_committee_by_epoch(0),
        );
    }

    #[test]
    fn simple() {
        let steps = 10;
        let mut chain = Simulacrum::new();

        let clock = chain.store().get_clock();
        let start_time_ms = clock.timestamp_ms();
        println!("clock: {:#?}", clock);
        for _ in 0..steps {
            chain.advance_clock(Duration::from_millis(1));
            chain.create_checkpoint();
            let clock = chain.store().get_clock();
            println!("clock: {:#?}", clock);
        }
        let end_time_ms = chain.store().get_clock().timestamp_ms();
        assert_eq!(end_time_ms - start_time_ms, steps);
        dbg!(chain.store().get_highest_checkpint());
    }

    #[test]
    fn simple_epoch() {
        let steps = 10;
        let mut chain = Simulacrum::new();

        let start_epoch = chain.store.get_highest_checkpint().unwrap().epoch;
        for i in 0..steps {
            chain.advance_epoch(/* create_random_state */ false);
            chain.advance_clock(Duration::from_millis(1));
            chain.create_checkpoint();
            println!("{i}");
        }
        let end_epoch = chain.store.get_highest_checkpint().unwrap().epoch;
        assert_eq!(end_epoch - start_epoch, steps);
        dbg!(chain.store().get_highest_checkpint());
    }

    #[test]
    fn transfer() {
        let mut sim = Simulacrum::new();
        let recipient = SuiAddress::random_for_testing_only();
        let (tx, transfer_amount) = sim.transfer_txn(recipient);

        let gas_id = tx.data().transaction_data().gas_data().payment[0].0;
        let effects = sim.execute_transaction(tx).unwrap().0;
        let gas_summary = effects.gas_cost_summary();
        let gas_paid = gas_summary.net_gas_usage();

        assert_eq!(
            (transfer_amount as i64 - gas_paid) as u64,
            store::SimulatorStore::get_object(sim.store(), &gas_id)
                .and_then(|object| GasCoin::try_from(&object).ok())
                .unwrap()
                .value()
        );

        assert_eq!(
            transfer_amount,
            sim.store()
                .owned_objects(recipient)
                .next()
                .and_then(|object| GasCoin::try_from(&object).ok())
                .unwrap()
                .value()
        );

        let checkpoint = sim.create_checkpoint();

        assert_eq!(&checkpoint.epoch_rolling_gas_cost_summary, gas_summary);
        assert_eq!(checkpoint.network_total_transactions, 2); // genesis + 1 txn
    }
}
