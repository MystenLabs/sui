// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! A `Simulacrum` of Sui.
//!
//! The word simulacrum is latin for "likeness, semblance", it is also a spell in D&D which creates
//! a copy of a creature which then follows the player's commands and wishes. As such this crate
//! provides the [`Simulacrum`] type which is a implementation or instantiation of a sui
//! blockcahin, one which doesn't do anything unless acted upon.
//!
//! [`Simulacrum`]: crate::Simulacrum

use anyhow::{anyhow, Result};
use rand::rngs::OsRng;
use sui_config::{genesis, transaction_deny_config::TransactionDenyConfig};
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_types::{
    base_types::SuiAddress,
    committee::Committee,
    crypto::{AuthoritySignInfo, AuthoritySignature, SuiAuthoritySignature},
    effects::TransactionEffects,
    gas_coin::MIST_PER_SUI,
    inner_temporary_store::InnerTemporaryStore,
    messages_checkpoint::{
        CertifiedCheckpointSummary, CheckpointSummary, EndOfEpochData, VerifiedCheckpoint,
    },
    signature::VerifyParams,
    transaction::{Transaction, VerifiedTransaction},
};

use self::checkpoint_builder::CheckpointBuilder;
use self::epoch_state::EpochState;
pub use self::store::InMemoryStore;
use self::store::KeyStore;

mod checkpoint_builder;
mod epoch_state;
mod store;

/// A `Simulacrum` of Sui.
///
/// This type represents a simulated instantiation of a Sui blockchain that needs to be driven
/// manually, that is time doesn't advance and checkpoints are not formed unless explicitly
/// requested.
///
/// See [module level][mod] documentation for more details.
///
/// [mod]: index.html
pub struct Simulacrum<R = OsRng> {
    rng: R,
    keystore: KeyStore,
    #[allow(unused)]
    genesis: genesis::Genesis,
    store: InMemoryStore,
    checkpoint_builder: CheckpointBuilder,

    // Epoch specific data
    epoch_state: EpochState,

    // Other
    deny_config: TransactionDenyConfig,
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
            .build();
        let keystore = KeyStore::from_newtork_config(&config);
        let store = InMemoryStore::new(&config.genesis);
        let checkpoint_builder = CheckpointBuilder::new(config.genesis.checkpoint());

        let genesis = config.genesis;
        let epoch_state = EpochState::new(genesis.sui_system_object());

        Self {
            rng,
            keystore,
            genesis,
            store,
            checkpoint_builder,
            epoch_state,
            deny_config: TransactionDenyConfig::default(),
        }
    }
}

impl<R> Simulacrum<R> {
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
    pub fn execute_transaction(&mut self, transaction: Transaction) -> Result<TransactionEffects> {
        // This only supports traditional authenticators and not zklogin
        let transaction = transaction.verify(&VerifyParams::default())?;

        let (inner_temporary_store, effects, _execution_error_opt) = self
            .epoch_state
            .execute_transaction(&self.store, &self.deny_config, &transaction)?;

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

        Ok(effects)
    }

    /// Creates the next Checkpoint using the Transactions enqueued since the last checkpoint was
    /// created.
    pub fn create_checkpoint(&mut self) -> VerifiedCheckpoint {
        let committee = CommitteeWithKeys::new(&self.keystore, self.epoch_state.committee());
        let (checkpoint, contents) = self
            .checkpoint_builder
            .build(&committee, self.store.get_clock().timestamp_ms());
        self.store.insert_checkpoint(checkpoint.clone());
        self.store.insert_checkpoint_contents(contents);
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
            VerifiedTransaction::new_consensus_commit_prologue(epoch, round, timestamp_ms);

        self.execute_transaction(consensus_commit_prologue_transaction.into())
            .expect("advancing the clock cannot fail")
    }

    /// Advances the epoch.
    ///
    /// This creates and executes an EpochChange transaction which advances the chain into the next
    /// epoch. Since the EpochChange transaction is required to be the final transaction in an
    /// epoch, the final checkpoint in the epoch is also created.
    ///
    /// NOTE: This function does not currently support updating the protocol version or the system
    /// packages
    pub fn advance_epoch(&mut self) {
        let next_epoch = self.epoch_state.epoch() + 1;
        let next_epoch_protocol_version = self.epoch_state.protocol_version();
        let gas_cost_summary = self.checkpoint_builder.epoch_rolling_gas_cost_summary();
        let epoch_start_timestamp_ms = self.store.get_clock().timestamp_ms();
        let next_epoch_system_package_bytes = vec![];
        let tx = VerifiedTransaction::new_change_epoch(
            next_epoch,
            next_epoch_protocol_version,
            gas_cost_summary.storage_cost,
            gas_cost_summary.computation_cost,
            gas_cost_summary.storage_rebate,
            gas_cost_summary.non_refundable_storage_fee,
            epoch_start_timestamp_ms,
            next_epoch_system_package_bytes,
        );

        self.execute_transaction(tx.into())
            .expect("advancing the epoch cannot fail");

        let new_epoch_state = EpochState::new(self.store.get_system_state());
        let end_of_epoch_data = EndOfEpochData {
            next_epoch_committee: new_epoch_state.committee().voting_rights.clone(),
            next_epoch_protocol_version,
            epoch_commitments: vec![],
        };
        let committee = CommitteeWithKeys::new(&self.keystore, self.epoch_state.committee());
        let (checkpoint, contents) = self.checkpoint_builder.build_end_of_epoch(
            &committee,
            self.store.get_clock().timestamp_ms(),
            next_epoch,
            end_of_epoch_data,
        );

        self.store.insert_checkpoint(checkpoint);
        self.store.insert_checkpoint_contents(contents);
        self.epoch_state = new_epoch_state;
    }

    pub fn store(&self) -> &InMemoryStore {
        &self.store
    }

    pub fn keystore(&self) -> &KeyStore {
        &self.keystore
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
        let tx = Transaction::from_data_and_signer(
            tx_data,
            shared_crypto::intent::Intent::sui_transaction(),
            vec![key],
        );

        self.execute_transaction(tx)
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

    pub fn committee(&self) -> &Committee {
        self.committee
    }

    pub fn keystore(&self) -> &KeyStore {
        self.keystore
    }

    fn create_certified_checkpoint(&self, checkpoint: CheckpointSummary) -> VerifiedCheckpoint {
        let signatures = self
            .committee()
            .voting_rights
            .iter()
            .map(|(name, _)| {
                let intent_msg = shared_crypto::intent::IntentMessage::new(
                    shared_crypto::intent::Intent::sui_app(
                        shared_crypto::intent::IntentScope::CheckpointSummary,
                    ),
                    &checkpoint,
                );
                let key = self.keystore().validator(name).unwrap();
                let signature = AuthoritySignature::new_secure(&intent_msg, &checkpoint.epoch, key);
                AuthoritySignInfo {
                    epoch: checkpoint.epoch,
                    authority: *name,
                    signature,
                }
            })
            .collect();

        let checkpoint = CertifiedCheckpointSummary::new(checkpoint, signatures, self.committee())
            .unwrap()
            .verify(self.committee())
            .unwrap();

        checkpoint
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use shared_crypto::intent::Intent;
    use sui_types::{
        base_types::SuiAddress,
        effects::TransactionEffectsAPI,
        gas_coin::GasCoin,
        programmable_transaction_builder::ProgrammableTransactionBuilder,
        transaction::{GasData, TransactionData, TransactionKind},
    };

    use super::*;

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
            chain.advance_epoch();
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
        let recipient = SuiAddress::generate(sim.rng());
        let (sender, key) = sim.keystore().accounts().next().unwrap();
        let sender = *sender;

        let object = sim
            .store()
            .owned_objects(sender)
            .find(|object| object.is_gas_coin())
            .unwrap();
        let gas_coin = GasCoin::try_from(object).unwrap();
        let gas_id = object.id();
        let transfer_amount = gas_coin.value() / 2;

        gas_coin.value();
        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            builder.transfer_sui(recipient, Some(transfer_amount));
            builder.finish()
        };

        let kind = TransactionKind::ProgrammableTransaction(pt);
        let gas_data = GasData {
            payment: vec![object.compute_object_reference()],
            owner: sender,
            price: sim.reference_gas_price(),
            budget: 1_000_000_000,
        };
        let tx_data = TransactionData::new_with_gas_data(kind, sender, gas_data);
        let tx = Transaction::from_data_and_signer(tx_data, Intent::sui_transaction(), vec![key]);

        let effects = sim.execute_transaction(tx).unwrap();
        let gas_summary = effects.gas_cost_summary();
        let gas_paid = gas_summary.net_gas_usage();

        assert_eq!(
            (transfer_amount as i64 - gas_paid) as u64,
            sim.store()
                .get_object(&gas_id)
                .and_then(|object| GasCoin::try_from(object).ok())
                .unwrap()
                .value()
        );

        assert_eq!(
            transfer_amount,
            sim.store()
                .owned_objects(recipient)
                .next()
                .and_then(|object| GasCoin::try_from(object).ok())
                .unwrap()
                .value()
        );

        let checkpoint = sim.create_checkpoint();

        assert_eq!(&checkpoint.epoch_rolling_gas_cost_summary, gas_summary);
        assert_eq!(checkpoint.network_total_transactions, 2); // genesis + 1 txn
    }
}
