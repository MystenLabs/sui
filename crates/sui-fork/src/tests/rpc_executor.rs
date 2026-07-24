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
use wiremock::MockServer;

use move_core_types::identifier::Identifier;
use prometheus::Registry;
use simulacrum::Simulacrum;
use simulacrum::SimulatorStore;
use simulacrum::store::in_mem_store::KeyStore;
use sui_swarm_config::network_config::NetworkConfig;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::AccountKeyPair;
use sui_types::crypto::KeypairTraits;
use sui_types::crypto::get_key_pair;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::execution_status::ExecutionErrorKind;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::gas_coin::GAS;
use sui_types::gas_coin::GasCoin;
use sui_types::object::Object;
use sui_types::object::Owner;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::storage::ObjectStore;
use sui_types::storage::ReadStore;
use sui_types::storage::RpcIndexes;
use sui_types::transaction::Argument;
use sui_types::transaction::GasData;
use sui_types::transaction::Transaction;
use sui_types::transaction::TransactionData;
use sui_types::transaction::TransactionDataAPI;
use sui_types::transaction::TransactionKind;
use sui_types::transaction_driver_types::EffectsFinalityInfo;
use sui_types::transaction_driver_types::ExecuteTransactionRequestV3;
use sui_types::transaction_driver_types::TransactionSubmissionError;
use sui_types::transaction_executor::TransactionChecks;
use sui_types::transaction_executor::TransactionExecutor;

use crate::context::Context;
use crate::rpc::executor::ForkedTransactionExecutor;
use crate::services::ServiceManager;
use crate::store::ForkStore;
use crate::test_support::absent_objects_gql_server;

/// Test harness that sets up a Simulacrum and a transaction executor to run transactions. Each test
/// creates a new harness to ensure isolation and a fresh state.
struct TestHarness {
    executor: ForkedTransactionExecutor,
    context: Arc<Context>,
    sender: SuiAddress,
    sender_key: AccountKeyPair,
    gas_object: Object,
    reference_gas_price: u64,
    checkpoint_receiver: tokio::sync::broadcast::Receiver<Arc<Checkpoint>>,
    temp: tempfile::TempDir,
    _services: Option<ServiceManager>,
    _gql_server: MockServer,
}

impl TestHarness {
    async fn new() -> Self {
        let temp = tempfile::tempdir().expect("failed to create tempdir");
        let mut rng = OsRng;
        let config = ConfigBuilder::new_with_temp_dir()
            .rng(&mut rng)
            .deterministic_committee_size(NonZeroUsize::MIN)
            .build();

        let genesis_checkpoint = config.genesis.checkpoint();
        let genesis_contents = config.genesis.checkpoint_contents().clone();
        let forked_at_checkpoint = genesis_checkpoint.data().sequence_number;
        let chain_identifier = (*genesis_checkpoint.digest()).into();
        let services = ServiceManager::open(
            temp.path(),
            "localnet".to_owned(),
            forked_at_checkpoint,
            chain_identifier,
        )
        .expect("service manager should open");
        // Initialize a ForkStore with the genesis objects, so the Simulacrum can read them.
        let gql_server = absent_objects_gql_server().await;
        let mut store = ForkStore::new_for_testing_with_remote(
            temp.path().to_path_buf(),
            gql_server.uri(),
            forked_at_checkpoint,
            services.local_store(),
        );
        store
            .save_checkpoint(&genesis_checkpoint, &genesis_contents)
            .expect("genesis checkpoint should be saved");
        let written: BTreeMap<ObjectID, Object> = config
            .genesis
            .objects()
            .iter()
            .map(|o| (o.id(), o.clone()))
            .collect();
        store.update_objects(written, vec![]);

        // Create a simulacrum object from the genesis config
        let keystore = KeyStore::from_network_config(&config);
        let sim = Simulacrum::new_from_custom_state(
            keystore,
            config.genesis.checkpoint(),
            config.genesis.sui_system_object(),
            &config,
            store.clone(),
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

        let (checkpoint_sender, checkpoint_receiver) = tokio::sync::broadcast::channel(4);
        let context = Arc::new(Context::new(sim, checkpoint_sender));
        let executor = ForkedTransactionExecutor::new(context.clone());

        Self {
            executor,
            context,
            sender,
            sender_key,
            gas_object,
            reference_gas_price,
            checkpoint_receiver,
            temp,
            _services: Some(services),
            _gql_server: gql_server,
        }
    }

    async fn new_with_services() -> Self {
        let temp = tempfile::tempdir().expect("failed to create tempdir");
        let mut rng = OsRng;
        let config = ConfigBuilder::new_with_temp_dir()
            .rng(&mut rng)
            .deterministic_committee_size(NonZeroUsize::MIN)
            .build();

        let genesis_checkpoint = config.genesis.checkpoint();
        let genesis_contents = config.genesis.checkpoint_contents().clone();
        let forked_at_checkpoint = genesis_checkpoint.data().sequence_number;
        let chain_identifier = (*genesis_checkpoint.digest()).into();
        let services = ServiceManager::open(
            temp.path(),
            "localnet".to_owned(),
            forked_at_checkpoint,
            chain_identifier,
        )
        .expect("service manager should open");
        let gql_server = absent_objects_gql_server().await;
        let mut store = ForkStore::new_for_testing_with_remote(
            temp.path().to_path_buf(),
            gql_server.uri(),
            forked_at_checkpoint,
            services.local_store(),
        );
        store
            .save_checkpoint(&genesis_checkpoint, &genesis_contents)
            .expect("genesis checkpoint should be saved");
        let written: BTreeMap<ObjectID, Object> = config
            .genesis
            .objects()
            .iter()
            .map(|o| (o.id(), o.clone()))
            .collect();
        store.update_objects(written, vec![]);

        let keystore = KeyStore::from_network_config(&config);
        let sim = Simulacrum::new_from_custom_state(
            keystore,
            genesis_checkpoint,
            config.genesis.sui_system_object(),
            &config,
            store.clone(),
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
        let registry = Registry::new();
        let (checkpoint_sender, checkpoint_receiver) = tokio::sync::broadcast::channel(4);
        let context = Arc::new(
            Context::new_with_services(sim, services, checkpoint_sender, &registry)
                .await
                .expect("service-backed context should initialize"),
        );
        let executor = ForkedTransactionExecutor::new(context.clone());

        Self {
            executor,
            context,
            sender,
            sender_key,
            gas_object,
            reference_gas_price,
            checkpoint_receiver,
            temp,
            _services: None,
            _gql_server: gql_server,
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

    /// Create transaction data where `amount` of SUI is transferred to `recipient`.
    fn build_transfer_tx_data_to(&self, recipient: SuiAddress, amount: u64) -> TransactionData {
        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            builder.transfer_sui(recipient, Some(amount));
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

    /// Create transaction data where `amount` of SUI is transferred to a random recipient.
    fn build_transfer_tx_data(&self, amount: u64) -> TransactionData {
        self.build_transfer_tx_data_to(SuiAddress::random_for_testing_only(), amount)
    }

    /// Create a transaction where `amount` of SUI is transferred to a random recipient.
    fn build_transfer_tx(&self, amount: u64) -> Transaction {
        let tx_data = self.build_transfer_tx_data(amount);
        Transaction::from_data_and_signer(tx_data, vec![&self.sender_key])
    }

    fn build_send_gas_funds_tx(&self, recipient: SuiAddress) -> Transaction {
        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            let recipient = builder.pure(recipient).unwrap();
            builder.programmable_move_call(
                SUI_FRAMEWORK_PACKAGE_ID,
                Identifier::new("coin").unwrap(),
                Identifier::new("send_funds").unwrap(),
                vec![GAS::type_tag()],
                vec![Argument::GasCoin, recipient],
            );
            builder.finish()
        };
        let tx_data = TransactionData::new_with_gas_data(
            TransactionKind::ProgrammableTransaction(pt),
            self.sender,
            GasData {
                payment: vec![self.gas_object.compute_object_reference()],
                owner: self.sender,
                price: self.reference_gas_price,
                budget: 100_000_000,
            },
        );
        Transaction::from_data_and_signer(tx_data, vec![&self.sender_key])
    }
}

#[tokio::test]
async fn test_tx_execution_publishes_checkpoint() {
    let mut harness = TestHarness::new().await;
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

// `simulate_transaction` is not yet supported by the forked executor (the
// current Simulacrum has no simulate entrypoint); re-enable once it lands.
#[ignore = "simulate_transaction not yet supported by the forked network"]
#[tokio::test]
async fn test_simulate_transaction_does_not_commit_or_checkpoint() {
    let mut harness = TestHarness::new().await;
    let tx_data = harness.build_transfer_tx_data(1_000);
    let gas_id = harness.gas_object.id();
    let before = {
        let sim = harness.context.simulacrum().read().await;
        SimulatorStore::get_object(sim.store(), &gas_id)
            .expect("gas object should exist before simulation")
            .compute_object_reference()
    };

    let result = harness
        .executor
        .simulate_transaction(tx_data, TransactionChecks::Enabled, false)
        .expect("simulate_transaction should succeed");

    assert!(result.effects.status().is_ok());
    assert!(!result.objects.is_empty());
    assert!(result.mock_gas_id.is_none());
    assert!(
        harness.checkpoint_receiver.try_recv().is_err(),
        "simulation must not publish a checkpoint",
    );

    let after = {
        let sim = harness.context.simulacrum().read().await;
        SimulatorStore::get_object(sim.store(), &gas_id)
            .expect("gas object should still exist after simulation")
            .compute_object_reference()
    };
    assert_eq!(after, before, "simulation must not mutate stored objects");
}

#[ignore = "simulate_transaction not yet supported by the forked network"]
#[tokio::test]
async fn test_simulate_transaction_supports_mock_gas() {
    let harness = TestHarness::new().await;
    let mut tx_data = harness.build_transfer_tx_data(1_000);
    tx_data.gas_data_mut().payment = Vec::new();

    let result = harness
        .executor
        .simulate_transaction(tx_data, TransactionChecks::Enabled, true)
        .expect("simulate_transaction with mock gas should succeed");

    assert!(result.effects.status().is_ok());
    assert_eq!(result.mock_gas_id, Some(ObjectID::MAX));
}

#[tokio::test]
async fn test_tx_execution_indexes_checkpoint_in_rpc_store() {
    let mut harness = TestHarness::new_with_services().await;
    let signed_tx = harness.build_transfer_tx(1_000);
    let expected_digest = *signed_tx.digest();

    let request = ExecuteTransactionRequestV3 {
        transaction: signed_tx,
        include_events: true,
        include_input_objects: false,
        include_output_objects: true,
        include_auxiliary_data: false,
    };
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
            .expect("timed out waiting for indexed checkpoint broadcast")
            .expect("checkpoint channel closed");
    assert_eq!(*checkpoint.summary.sequence_number(), checkpoint_seq);

    let reader = harness.context.services().unwrap().reader();
    assert!(
        reader
            .get_checkpoint_by_sequence_number(checkpoint_seq)
            .is_some()
    );
    assert!(
        reader
            .get_checkpoint_contents_by_sequence_number(checkpoint_seq)
            .is_some()
    );
    assert!(reader.get_transaction(&expected_digest).is_some());
    assert_eq!(
        *reader
            .get_transaction_effects(&expected_digest)
            .expect("effects should be indexed")
            .transaction_digest(),
        expected_digest,
    );

    for object in response
        .output_objects
        .expect("output objects should be populated")
    {
        let stored = reader
            .get_object(&object.id())
            .expect("output object should be indexed as live");
        assert_eq!(
            stored.compute_object_reference(),
            object.compute_object_reference(),
        );
    }
}

#[tokio::test]
async fn test_rpc_reads_serve_indexed_post_fork_data_from_rpc_store() {
    let mut harness = TestHarness::new_with_services().await;
    let signed_tx = harness.build_transfer_tx(1_000);
    let expected_digest = *signed_tx.digest();

    let request = ExecuteTransactionRequestV3 {
        transaction: signed_tx,
        include_events: false,
        include_input_objects: false,
        include_output_objects: true,
        include_auxiliary_data: false,
    };
    let response = harness
        .executor
        .execute_transaction(request, None)
        .await
        .expect("execute_transaction should succeed");

    let EffectsFinalityInfo::Checkpointed(_epoch, checkpoint_seq) = response.effects.finality_info
    else {
        panic!("forked execution should report checkpointed finality");
    };

    tokio::time::timeout(Duration::from_secs(5), harness.checkpoint_receiver.recv())
        .await
        .expect("timed out waiting for indexed checkpoint broadcast")
        .expect("checkpoint channel closed");

    let reader = ForkStore::new_for_testing(
        harness.temp.path().join("empty-rpc-read-store"),
        harness.context.services().unwrap().local_store(),
    );

    assert!(ReadStore::get_checkpoint_by_sequence_number(&reader, checkpoint_seq).is_some());
    assert!(ReadStore::get_transaction(&reader, &expected_digest).is_some());
    assert_eq!(
        *ReadStore::get_transaction_effects(&reader, &expected_digest)
            .expect("effects should be indexed")
            .transaction_digest(),
        expected_digest,
    );

    for object in response
        .output_objects
        .expect("output objects should be populated")
    {
        assert!(
            ObjectStore::get_object(&reader, &object.id()).is_some(),
            "output object {} should be readable through the RPC read surface",
            object.id(),
        );
    }
}

/// The embedded indexer is the sole writer of derived indexes (owner, type,
/// balance, package) for local checkpoints, and checkpoint publication blocks
/// until it has indexed the sealed checkpoint. After an execution returns,
/// owner and balance lookups must therefore already serve the transfer
/// through the stock rpc-store reader.
#[tokio::test]
async fn test_indexer_populates_derived_indexes_for_local_execution() {
    let mut harness = TestHarness::new_with_services().await;
    let recipient = SuiAddress::random_for_testing_only();
    let transfer_amount = 1_000;
    let tx_data = harness.build_transfer_tx_data_to(recipient, transfer_amount);
    let signed_tx = Transaction::from_data_and_signer(tx_data, vec![&harness.sender_key]);

    let request = ExecuteTransactionRequestV3::new_v2(signed_tx);
    harness
        .executor
        .execute_transaction(request, None)
        .await
        .expect("execute_transaction should succeed");

    tokio::time::timeout(Duration::from_secs(5), harness.checkpoint_receiver.recv())
        .await
        .expect("timed out waiting for indexed checkpoint broadcast")
        .expect("checkpoint channel closed");

    let reader = harness.context.services().unwrap().reader();

    // Recipient: the transferred coin appears in the owner index...
    let recipient_infos: Vec<_> = RpcIndexes::owned_objects_iter(&reader, recipient, None, None)
        .expect("owned-object iterator should build")
        .map(|result| result.expect("owned-object entry should decode"))
        .collect();
    assert_eq!(recipient_infos.len(), 1);
    assert_eq!(recipient_infos[0].owner, recipient);
    assert_eq!(recipient_infos[0].balance, Some(transfer_amount));

    // ...and in the balance index.
    let balance = RpcIndexes::get_balance(&reader, &recipient, &GAS::type_())
        .expect("balance lookup should not error")
        .expect("recipient balance should be indexed");
    assert_eq!(balance.coin_balance, transfer_amount);

    // Sender: the mutated gas coin is re-indexed at its post-execution version.
    let sender_infos: Vec<_> = RpcIndexes::owned_objects_iter(&reader, harness.sender, None, None)
        .expect("owned-object iterator should build")
        .map(|result| result.expect("owned-object entry should decode"))
        .collect();
    let gas_info = sender_infos
        .iter()
        .find(|info| info.object_id == harness.gas_object.id())
        .expect("sender's gas coin should be indexed");
    assert!(gas_info.version > harness.gas_object.version());
}

#[tokio::test]
async fn test_send_gas_funds_publishes_checkpoint() {
    let mut harness = TestHarness::new().await;
    let signed_tx = harness.build_send_gas_funds_tx(SuiAddress::random_for_testing_only());

    let request = ExecuteTransactionRequestV3::new_v2(signed_tx);
    let response = harness
        .executor
        .execute_transaction(request, None)
        .await
        .expect("send_funds should execute and publish a checkpoint");

    assert!(
        response.effects.effects.status().is_ok(),
        "send_funds failed: {:?}",
        response.effects.effects.status(),
    );

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
    let harness = TestHarness::new().await;
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
    let mut harness = TestHarness::new().await;
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
    let harness = TestHarness::new().await;

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
    let mut harness = TestHarness::new().await;

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
    let harness = TestHarness::new().await;
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
