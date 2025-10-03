// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prost_types::FieldMask;
use shared_crypto::intent::Intent;
use sui_keys::keystore::AccountKeystore;
use sui_macros::sim_test;
use sui_rpc::proto::sui::rpc::v2::transaction_execution_service_client::TransactionExecutionServiceClient;
use sui_rpc::proto::sui::rpc::v2::Argument;
use sui_rpc::proto::sui::rpc::v2::Bcs;
use sui_rpc::proto::sui::rpc::v2::Command;
use sui_rpc::proto::sui::rpc::v2::ExecuteTransactionRequest;
use sui_rpc::proto::sui::rpc::v2::GasPayment;
use sui_rpc::proto::sui::rpc::v2::Input;
use sui_rpc::proto::sui::rpc::v2::MoveCall;
use sui_rpc::proto::sui::rpc::v2::ObjectReference;
use sui_rpc::proto::sui::rpc::v2::ProgrammableTransaction;
use sui_rpc::proto::sui::rpc::v2::SimulateTransactionRequest;
use sui_rpc::proto::sui::rpc::v2::Transaction;
use sui_rpc::proto::sui::rpc::v2::TransactionKind;
use sui_rpc::proto::sui::rpc::v2::TransferObjects;
use sui_rpc::proto::sui::rpc::v2::UserSignature;
use sui_rpc_api::Client;
use sui_types::base_types::SuiAddress;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::Command as SuiCommand;
use sui_types::transaction::{ObjectArg, TransactionData, TransactionDataAPI};
use test_cluster::TestClusterBuilder;

fn proto_to_response(
    proto: sui_rpc::proto::sui::rpc::v2::SimulateTransactionResponse,
) -> (
    sui_types::transaction::TransactionData,
    sui_types::effects::TransactionEffects,
    Option<sui_types::effects::TransactionEvents>,
) {
    let executed_transaction = proto.transaction.unwrap();
    let transaction = executed_transaction
        .transaction
        .unwrap()
        .bcs
        .unwrap()
        .deserialize()
        .unwrap();
    let effects = executed_transaction
        .effects
        .unwrap()
        .bcs
        .unwrap()
        .deserialize()
        .unwrap();
    let events = executed_transaction
        .events
        .map(|events| events.bcs.unwrap().deserialize().unwrap());

    (transaction, effects, events)
}

#[sim_test]
async fn resolve_transaction_simple_transfer() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let client = Client::new(test_cluster.rpc_url()).unwrap();
    let mut alpha_client =
        TransactionExecutionServiceClient::connect(test_cluster.rpc_url().to_owned())
            .await
            .unwrap();
    let recipient = SuiAddress::random_for_testing_only();

    let (sender, mut gas) = test_cluster.wallet.get_one_account().await.unwrap();
    gas.sort_by_key(|object_ref| object_ref.0);
    let obj_to_send = gas.first().unwrap().0;

    let mut unresolved_transaction = Transaction::default();
    unresolved_transaction.kind = Some(TransactionKind::from({
        let mut ptb = ProgrammableTransaction::default();
        ptb.inputs = vec![
            {
                let mut message = Input::default();
                message.object_id = Some(obj_to_send.to_canonical_string(true));
                message
            },
            {
                let mut message = Input::default();
                message.literal = Some(Box::new(recipient.to_string().into()));
                message
            },
        ];
        ptb.commands = vec![Command::from({
            let mut message = TransferObjects::default();
            message.objects = vec![Argument::new_input(0)];
            message.address = Some(Argument::new_input(1));
            message
        })];
        ptb
    }));
    unresolved_transaction.sender = Some(sender.to_string());

    let resolved = alpha_client
        .simulate_transaction(
            SimulateTransactionRequest::new(unresolved_transaction).with_do_gas_selection(true),
        )
        .await
        .unwrap()
        .into_inner();
    let (transaction, effects_from_simulation, _events) = proto_to_response(resolved);

    let signed_transaction = test_cluster.wallet.sign_transaction(&transaction).await;
    let effects = client
        .execute_transaction(&signed_transaction)
        .await
        .unwrap()
        .effects;

    assert!(effects.status().is_ok());
    assert_eq!(effects_from_simulation, effects);
}

#[sim_test]
async fn resolve_transaction_transfer_with_sponsor() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let client = Client::new(test_cluster.rpc_url()).unwrap();
    let mut alpha_client =
        TransactionExecutionServiceClient::connect(test_cluster.rpc_url().to_owned())
            .await
            .unwrap();
    let recipient = SuiAddress::random_for_testing_only();

    let (sender, gas) = test_cluster.wallet.get_one_account().await.unwrap();
    let obj_to_send = gas.first().unwrap().0;
    let sponsor = test_cluster.wallet.get_addresses()[1];

    let mut unresolved_transaction = Transaction::default();
    unresolved_transaction.kind = Some(TransactionKind::from({
        let mut ptb = ProgrammableTransaction::default();
        ptb.inputs = vec![
            {
                let mut message = Input::default();
                message.object_id = Some(obj_to_send.to_canonical_string(true));
                message
            },
            {
                let mut message = Input::default();
                message.literal = Some(Box::new(recipient.to_string().into()));
                message
            },
        ];
        ptb.commands = vec![Command::from({
            let mut message = TransferObjects::default();
            message.objects = vec![Argument::new_input(0)];
            message.address = Some(Argument::new_input(1));
            message
        })];
        ptb
    }));
    unresolved_transaction.sender = Some(sender.to_string());
    unresolved_transaction.gas_payment = Some({
        let mut message = GasPayment::default();
        message.owner = Some(sponsor.to_string());
        message
    });

    let resolved = alpha_client
        .simulate_transaction(
            SimulateTransactionRequest::new(unresolved_transaction).with_do_gas_selection(true),
        )
        .await
        .unwrap()
        .into_inner();
    let (transaction, effects_from_simulation, _events) = proto_to_response(resolved);

    let sender_sig = test_cluster
        .wallet
        .config
        .keystore
        .sign_secure(&sender, &transaction, Intent::sui_transaction())
        .await
        .unwrap();
    let sponsor_sig = test_cluster
        .wallet
        .config
        .keystore
        .sign_secure(&sponsor, &transaction, Intent::sui_transaction())
        .await
        .unwrap();

    let signed_transaction =
        sui_types::transaction::Transaction::from_data(transaction, vec![sender_sig, sponsor_sig]);
    let effects = client
        .execute_transaction(&signed_transaction)
        .await
        .unwrap()
        .effects;

    assert!(effects.status().is_ok());
    assert_eq!(effects_from_simulation, effects);
}

#[sim_test]
async fn resolve_transaction_borrowed_shared_object() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let client = Client::new(test_cluster.rpc_url()).unwrap();
    let mut alpha_client =
        TransactionExecutionServiceClient::connect(test_cluster.rpc_url().to_owned())
            .await
            .unwrap();

    let sender = test_cluster.wallet.get_addresses()[0];

    let mut unresolved_transaction = Transaction::default();
    unresolved_transaction.kind = Some(TransactionKind::from({
        let mut ptb = ProgrammableTransaction::default();
        ptb.inputs = vec![{
            let mut message = Input::default();
            message.object_id = Some("0x6".to_owned());
            message
        }];
        ptb.commands = vec![Command::from({
            let mut message = MoveCall::default();
            message.package = Some("0x2".to_owned());
            message.module = Some("clock".to_owned());
            message.function = Some("timestamp_ms".to_owned());
            message.arguments = vec![Argument::new_input(0)];
            message
        })];
        ptb
    }));
    unresolved_transaction.sender = Some(sender.to_string());

    let resolved = alpha_client
        .simulate_transaction(
            SimulateTransactionRequest::new(unresolved_transaction).with_do_gas_selection(true),
        )
        .await
        .unwrap()
        .into_inner();
    let (transaction, _effects, _events) = proto_to_response(resolved);

    let signed_transaction = test_cluster.wallet.sign_transaction(&transaction).await;
    let effects = client
        .execute_transaction(&signed_transaction)
        .await
        .unwrap()
        .effects;

    assert!(effects.status().is_ok());
}

#[sim_test]
async fn resolve_transaction_mutable_shared_object() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let client = Client::new(test_cluster.rpc_url()).unwrap();
    let mut alpha_client =
        TransactionExecutionServiceClient::connect(test_cluster.rpc_url().to_owned())
            .await
            .unwrap();

    let (sender, mut gas) = test_cluster.wallet.get_one_account().await.unwrap();
    gas.sort_by_key(|object_ref| object_ref.0);
    let obj_to_stake = gas.first().unwrap().0;

    let validator_address = test_cluster.swarm.config().validator_configs()[0].sui_address();

    let mut unresolved_transaction = Transaction::default();
    unresolved_transaction.kind = Some(TransactionKind::from({
        let mut ptb = ProgrammableTransaction::default();
        ptb.inputs = vec![
            {
                let mut message = Input::default();
                message.object_id = Some("0x5".to_owned());
                message
            },
            {
                let mut message = Input::default();
                message.object_id = Some(obj_to_stake.to_canonical_string(true));
                message
            },
            {
                let mut message = Input::default();
                message.literal = Some(Box::new(validator_address.to_string().into()));
                message
            },
        ];
        ptb.commands = vec![Command::from({
            let mut message = MoveCall::default();
            message.package = Some("0x3".to_owned());
            message.module = Some("sui_system".to_owned());
            message.function = Some("request_add_stake".to_owned());
            message.arguments = vec![
                Argument::new_input(0),
                Argument::new_input(1),
                Argument::new_input(2),
            ];
            message
        })];
        ptb
    }));
    unresolved_transaction.sender = Some(sender.to_string());

    let resolved = alpha_client
        .simulate_transaction(
            SimulateTransactionRequest::new(unresolved_transaction).with_do_gas_selection(true),
        )
        .await
        .unwrap()
        .into_inner();
    let (transaction, effects_from_simulation, _events) = proto_to_response(resolved);

    let signed_transaction = test_cluster.wallet.sign_transaction(&transaction).await;
    let effects = client
        .execute_transaction(&signed_transaction)
        .await
        .unwrap()
        .effects;

    assert!(effects.status().is_ok());
    assert_eq!(effects_from_simulation, effects);
}

#[sim_test]
async fn resolve_transaction_insufficient_gas() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let mut alpha_client =
        TransactionExecutionServiceClient::connect(test_cluster.rpc_url().to_owned())
            .await
            .unwrap();

    // Test the case where we don't have enough coins/gas for the required budget
    let mut unresolved_transaction = Transaction::default();
    unresolved_transaction.kind = Some(TransactionKind::from({
        let mut ptb = ProgrammableTransaction::default();
        ptb.inputs = vec![{
            let mut message = Input::default();
            message.object_id = Some("0x6".to_owned());
            message
        }];
        ptb.commands = vec![Command::from({
            let mut message = MoveCall::default();
            message.package = Some("0x2".to_owned());
            message.module = Some("clock".to_owned());
            message.function = Some("timestamp_ms".to_owned());
            message.arguments = vec![Argument::new_input(0)];
            message
        })];
        ptb
    }));
    unresolved_transaction.sender = Some(SuiAddress::random_for_testing_only().to_string()); // random account with no

    let error = alpha_client
        .simulate_transaction(
            SimulateTransactionRequest::new(unresolved_transaction).with_do_gas_selection(true),
        )
        .await
        .unwrap_err();

    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert_contains(error.message(), "unable to select sufficient gas");
}

fn assert_contains(haystack: &str, needle: &str) {
    if !haystack.contains(needle) {
        panic!("{haystack:?} does not contain {needle:?}");
    }
}

#[sim_test]
async fn resolve_transaction_gas_budget_clamping() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let client = Client::new(test_cluster.rpc_url()).unwrap();
    let mut alpha_client =
        TransactionExecutionServiceClient::connect(test_cluster.rpc_url().to_owned())
            .await
            .unwrap();

    let (sender, gas_coins) = test_cluster.wallet.get_one_account().await.unwrap();

    // Use ALL gas coins - the test wallet has multiple coins that should exceed max gas budget
    let gas_objects: Vec<_> = gas_coins
        .iter()
        .map(|obj_ref| {
            let mut object_reference = ObjectReference::default();
            object_reference.object_id = Some(obj_ref.0.to_canonical_string(true));
            object_reference
        })
        .collect();

    let mut unresolved_transaction = Transaction::default();
    unresolved_transaction.kind = Some(TransactionKind::from({
        let mut ptb = ProgrammableTransaction::default();
        ptb.inputs = vec![{
            let mut message = Input::default();
            message.object_id = Some("0x6".to_owned());
            message
        }];
        ptb.commands = vec![Command::from({
            let mut message = MoveCall::default();
            message.package = Some("0x2".to_owned());
            message.module = Some("clock".to_owned());
            message.function = Some("timestamp_ms".to_owned());
            message.arguments = vec![Argument::new_input(0)];
            message
        })];
        ptb
    }));
    unresolved_transaction.sender = Some(sender.to_string());
    unresolved_transaction.gas_payment = Some({
        let mut message = GasPayment::default();
        message.owner = Some(sender.to_string());
        message.objects = gas_objects;
        message
    });

    let resolved = alpha_client
        .simulate_transaction(
            SimulateTransactionRequest::new(unresolved_transaction).with_do_gas_selection(true),
        )
        .await
        .unwrap()
        .into_inner();
    let (transaction, effects_from_simulation, _events) = proto_to_response(resolved);

    // Budget should be populated based on the real estimated gas fee which should be far less than
    // 1 sui.
    assert!(transaction.gas_data().budget > 0,);
    assert!(transaction.gas_data().budget < 1_000_000_000,);

    let signed_transaction = test_cluster.wallet.sign_transaction(&transaction).await;
    let effects = client
        .execute_transaction(&signed_transaction)
        .await
        .unwrap()
        .effects;

    assert!(effects.status().is_ok());
    assert!(effects_from_simulation.status().is_ok());
}

#[sim_test]
async fn resolve_transaction_insufficient_gas_with_payment_objects() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let mut alpha_client =
        TransactionExecutionServiceClient::connect(test_cluster.rpc_url().to_owned())
            .await
            .unwrap();

    let (sender, gas_coins) = test_cluster.wallet.get_one_account().await.unwrap();

    // First, split a coin to create one with only 1 MIST
    let coin_to_split = gas_coins[0];
    let gas_for_split = gas_coins[1];

    let mut builder = ProgrammableTransactionBuilder::new();
    let coin_arg = builder
        .obj(ObjectArg::ImmOrOwnedObject(coin_to_split))
        .unwrap();
    // Split off 1M MIST (enough for min gas but not enough for actual execution)
    let amt_arg = builder.pure(1_000_000u64).unwrap();
    let split_result = builder.command(SuiCommand::SplitCoins(coin_arg, vec![amt_arg]));

    let split_coin = match split_result {
        sui_types::transaction::Argument::Result(idx) => {
            sui_types::transaction::Argument::NestedResult(idx, 0)
        }
        _ => panic!("Expected Result argument"),
    };
    builder.transfer_arg(sender, split_coin);

    let ptb = builder.finish();
    let tx_data =
        TransactionData::new_programmable(sender, vec![gas_for_split], ptb, 10_000_000, 1000);

    let signed_tx = test_cluster.wallet.sign_transaction(&tx_data).await;

    // Execute transaction and wait for checkpoint so indexes are updated
    let mut client = sui_rpc::client::v2::Client::new(test_cluster.rpc_url()).unwrap();

    let mut transaction = sui_rpc::proto::sui::rpc::v2::Transaction::default();
    transaction.bcs = Some(Bcs::serialize(signed_tx.transaction_data()).unwrap());

    let signatures = signed_tx
        .tx_signatures()
        .iter()
        .map(|s| {
            let mut message = UserSignature::default();
            message.bcs = Some({
                let mut message = Bcs::default();
                message.value = Some(s.as_ref().to_owned().into());
                message
            });
            message
        })
        .collect();

    let mut request = ExecuteTransactionRequest::default();
    request.transaction = Some(transaction);
    request.signatures = signatures;
    request.read_mask = Some(FieldMask {
        paths: vec!["transaction".to_string(), "effects".to_string()],
    });

    let executed_tx = client
        .execute_transaction_and_wait_for_checkpoint(request, std::time::Duration::from_secs(10))
        .await
        .unwrap()
        .into_inner()
        .transaction()
        .to_owned();

    // Just get the effects, the helper function already asserts success
    let effects_proto = executed_tx.effects.unwrap();

    // Convert effects to native type to find the created coin
    let effects: sui_types::effects::TransactionEffects =
        effects_proto.bcs.unwrap().deserialize().unwrap();

    // Find the newly created coin with 1M MIST from the effects
    let tiny_coin = effects
        .created()
        .into_iter()
        .map(|(obj_ref, _)| obj_ref)
        .find(|obj_ref| obj_ref.0 != coin_to_split.0)
        .expect("Should have created a new coin with 1M MIST");

    // Now try to use this 1 MIST coin as gas payment for a transaction
    let gas_objects = vec![{
        let mut object_reference = ObjectReference::default();
        object_reference.object_id = Some(tiny_coin.0.to_canonical_string(true));
        object_reference
    }];

    let mut unresolved_transaction = Transaction::default();
    unresolved_transaction.kind = Some(TransactionKind::from({
        let mut ptb = ProgrammableTransaction::default();
        ptb.inputs = vec![{
            let mut message = Input::default();
            message.object_id = Some("0x6".to_owned());
            message
        }];
        ptb.commands = vec![Command::from({
            let mut message = MoveCall::default();
            message.package = Some("0x2".to_owned());
            message.module = Some("clock".to_owned());
            message.function = Some("timestamp_ms".to_owned());
            message.arguments = vec![Argument::new_input(0)];
            message
        })];
        ptb
    }));
    unresolved_transaction.sender = Some(sender.to_string());
    unresolved_transaction.gas_payment = Some({
        let mut message = GasPayment::default();
        message.owner = Some(sender.to_string());
        message.objects = gas_objects;
        // Don't specify budget - let it be estimated
        message
    });

    // This should fail because the 1M MIST coin doesn't have enough balance
    // to cover the estimated budget for the transaction
    let error = alpha_client
        .simulate_transaction(
            SimulateTransactionRequest::new(unresolved_transaction).with_do_gas_selection(true),
        )
        .await
        .unwrap_err();

    assert_eq!(
        error.code(),
        tonic::Code::InvalidArgument,
        "Expected InvalidArgument error code"
    );
    assert_contains(
        error.message(),
        "Insufficient gas balance to cover estimated transaction cost.",
    );
}
