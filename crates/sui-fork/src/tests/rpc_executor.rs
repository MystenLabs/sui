// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Adapter-level integration tests for `ForkedTransactionExecutor`: runs
//! `ExecuteTransactionRequestV3` requests through the same entry point the gRPC
//! `TransactionExecutionService` uses, and asserts the forked Simulacrum
//! produced matching effects — including error paths. The gRPC socket is
//! intentionally not exercised here.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

use rand::rngs::OsRng;

use simulacrum::Simulacrum;
use simulacrum::SimulatorStore;
use simulacrum::store::in_mem_store::KeyStore;
use sui_protocol_config::Chain;
use sui_swarm_config::network_config::NetworkConfig;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::crypto::{AccountKeyPair, KeypairTraits, get_key_pair};
use sui_types::effects::TransactionEffectsAPI;
use sui_types::error::SuiErrorKind;
use sui_types::execution_status::ExecutionErrorKind;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::gas_coin::GasCoin;
use sui_types::object::{Object, Owner};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{GasData, Transaction, TransactionData, TransactionKind};
use sui_types::transaction_driver_types::{
    EffectsFinalityInfo, ExecuteTransactionRequestV3, TransactionSubmissionError,
};
use sui_types::transaction_executor::{TransactionChecks, TransactionExecutor};

use crate::context::Context;
use crate::rpc::executor::ForkedTransactionExecutor;
use crate::store::DataStore;

/// Test harness that sets up a Simulacrum and a transaction executor to run transactions. Each test
/// creates a new harness to ensure isolation and a fresh state.
struct TestHarness {
    executor: ForkedTransactionExecutor,
    sender: SuiAddress,
    sender_key: AccountKeyPair,
    gas_object: Object,
    reference_gas_price: u64,
    checkpoint_receiver: tokio::sync::mpsc::Receiver<Checkpoint>,
    temp: tempfile::TempDir,
}

impl TestHarness {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("failed to create tempdir");
        let mut rng = OsRng;
        let config = ConfigBuilder::new_with_temp_dir()
            .rng(&mut rng)
            .deterministic_committee_size(NonZeroUsize::MIN)
            .build();

        // Initialize a DataStore with the genesis objects, so the Simulacrum can read them.
        let mut data_store = DataStore::new_for_testing(temp.path().to_path_buf());
        let written: BTreeMap<ObjectID, Object> = config
            .genesis
            .objects()
            .iter()
            .map(|o| (o.id(), o.clone()))
            .collect();
        data_store.update_objects(written, vec![]);

        // Create a simulacrum object from the genesis config
        let keystore = KeyStore::from_network_config(&config);
        let sim = Simulacrum::new_from_custom_state(
            keystore,
            config.genesis.checkpoint(),
            config.genesis.sui_system_object(),
            &config,
            data_store.clone(),
            rng,
        );
        let reference_gas_price = sim.reference_gas_price();

        let (sender, sender_key) = {
            let (addr, key) = sim
                .keystore()
                .accounts()
                .next()
                .expect("at least one account");
            (*addr, key.copy())
        };

        let gas_object = Self::find_gas_coin(&config, sender);

        let (checkpoint_sender, checkpoint_receiver) = tokio::sync::mpsc::channel(4);
        let context = Arc::new(Context::new(sim, Chain::Unknown, checkpoint_sender));
        let executor = ForkedTransactionExecutor::new(context);

        Self {
            executor,
            sender,
            sender_key,
            gas_object,
            reference_gas_price,
            checkpoint_receiver,
            temp,
        }
    }

    fn find_gas_coin(config: &NetworkConfig, owner: SuiAddress) -> Object {
        config
            .genesis
            .objects()
            .iter()
            .find(|obj| obj.owner == Owner::AddressOwner(owner) && obj.is_gas_coin())
            .expect("owner should have a gas coin in genesis")
            .clone()
    }

    /// Create transaction data where `amount` of SUI is transferred to a random recipient.
    fn build_transfer_tx_data(&self, amount: u64) -> TransactionData {
        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            builder.transfer_sui(SuiAddress::random_for_testing_only(), Some(amount));
            builder.finish()
        };
        TransactionData::new_with_gas_data(
            TransactionKind::ProgrammableTransaction(pt),
            self.sender,
            GasData {
                payment: vec![self.gas_object.compute_object_reference()],
                owner: self.sender,
                price: self.reference_gas_price,
                budget: 100_000_000,
            },
        )
    }

    /// Create a transaction where `amount` of SUI is transferred to a random recipient.
    fn build_transfer_tx(&self, amount: u64) -> Transaction {
        let tx_data = self.build_transfer_tx_data(amount);
        Transaction::from_data_and_signer(tx_data, vec![&self.sender_key])
    }
}

#[tokio::test]
async fn test_tx_execution_publishes_checkpoint() {
    let mut harness = TestHarness::new();
    let signed_tx = harness.build_transfer_tx(1_000);

    let request = ExecuteTransactionRequestV3::new_v2(signed_tx);
    let response = harness
        .executor
        .execute_transaction(request, None)
        .await
        .expect("execute_transaction should succeed");

    let EffectsFinalityInfo::Checkpointed(_epoch, checkpoint_seq) = response.effects.finality_info
    else {
        panic!("forked execution should report checkpointed finality");
    };

    let checkpoint =
        tokio::time::timeout(Duration::from_secs(5), harness.checkpoint_receiver.recv())
            .await
            .expect("timed out waiting for published checkpoint")
            .expect("checkpoint channel closed");

    assert_eq!(*checkpoint.summary.sequence_number(), checkpoint_seq);
}

#[tokio::test]
async fn test_tx_execution() {
    let harness = TestHarness::new();
    let signed_tx = harness.build_transfer_tx(1_000);
    let expected_digest = *signed_tx.digest();

    let request = ExecuteTransactionRequestV3::new_v2(signed_tx);
    let response = harness
        .executor
        .execute_transaction(request, None)
        .await
        .expect("execute_transaction should succeed");

    assert!(
        response.effects.effects.status().is_ok(),
        "transfer failed: {:?}",
        response.effects.effects.status(),
    );
    assert_eq!(
        *response.effects.effects.transaction_digest(),
        expected_digest
    );
}

#[tokio::test]
async fn test_empty_signature_transaction_impersonates_sender() {
    let mut harness = TestHarness::new();
    let tx_data = harness.build_transfer_tx_data(1_000);
    let transaction = Transaction::from_generic_sig_data(tx_data, vec![]);
    let expected_digest = *transaction.digest();

    let request = ExecuteTransactionRequestV3::new_v2(transaction);
    let response = harness
        .executor
        .execute_transaction(request, None)
        .await
        .expect("empty-signature transaction should impersonate sender");

    assert!(
        response.effects.effects.status().is_ok(),
        "transfer failed: {:?}",
        response.effects.effects.status(),
    );
    assert_eq!(
        *response.effects.effects.transaction_digest(),
        expected_digest
    );

    tokio::time::timeout(Duration::from_secs(5), harness.checkpoint_receiver.recv())
        .await
        .expect("timed out waiting for published checkpoint")
        .expect("checkpoint channel closed");
}

#[tokio::test]
async fn test_insufficient_coin_balance() {
    let harness = TestHarness::new();

    let balance = GasCoin::try_from(&harness.gas_object).unwrap().value();
    let signed_tx = harness.build_transfer_tx(balance + 1);
    let expected_digest = *signed_tx.digest();

    let request = ExecuteTransactionRequestV3::new_v2(signed_tx);
    let response = harness
        .executor
        .execute_transaction(request, None)
        .await
        .expect("execute_transaction should return Ok");

    let (error_kind, _command) = response.effects.effects.status().clone().unwrap_err();
    assert!(
        matches!(error_kind, ExecutionErrorKind::InsufficientCoinBalance),
        "expected InsufficientCoinBalance, got {error_kind:?}",
    );
    assert_eq!(
        *response.effects.effects.transaction_digest(),
        expected_digest
    );
}

#[tokio::test]
async fn test_bad_signature_returns_submission_error() {
    let mut harness = TestHarness::new();

    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder.transfer_sui(SuiAddress::random_for_testing_only(), Some(1_000));
        builder.finish()
    };
    let tx_data = TransactionData::new_with_gas_data(
        TransactionKind::ProgrammableTransaction(pt),
        harness.sender,
        GasData {
            payment: vec![harness.gas_object.compute_object_reference()],
            owner: harness.sender,
            price: harness.reference_gas_price,
            budget: 100_000_000,
        },
    );

    let (_wrong_addr, wrong_key): (SuiAddress, AccountKeyPair) = get_key_pair();
    let bad_tx = Transaction::from_data_and_signer(tx_data, vec![&wrong_key]);

    let request = ExecuteTransactionRequestV3::new_v2(bad_tx);
    let err = harness
        .executor
        .execute_transaction(request, None)
        .await
        .expect_err("bad signature should be rejected");

    assert!(
        matches!(err, TransactionSubmissionError::InvalidUserSignature(_)),
        "expected InvalidUserSignature, got {err:?}",
    );
    assert!(
        harness.checkpoint_receiver.try_recv().is_err(),
        "rejected transactions must not publish checkpoints",
    );
}

#[tokio::test]
async fn test_include_input_and_output_objects() {
    let harness = TestHarness::new();
    let transaction = harness.build_transfer_tx(1_000);

    let request = ExecuteTransactionRequestV3 {
        transaction,
        include_events: false,
        include_input_objects: true,
        include_output_objects: true,
        include_auxiliary_data: false,
    };
    let response = harness
        .executor
        .execute_transaction(request, None)
        .await
        .expect("execute_transaction should succeed");

    assert!(response.effects.effects.status().is_ok());

    let input_objects = response
        .input_objects
        .expect("input_objects should be populated");
    assert!(
        !input_objects.is_empty(),
        "input_objects should contain the gas coin",
    );

    let output_objects = response
        .output_objects
        .expect("output_objects should be populated");
    assert!(
        !output_objects.is_empty(),
        "output_objects should contain mutated/created objects",
    );
}
