// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::ident_str;
use sui_macros::sim_test;
use sui_rpc::Client;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::GetTransactionRequest;
use sui_rpc::proto::sui::rpc::v2::get_checkpoint_request::CheckpointId;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_rpc::proto::sui::rpc::v2::{Checkpoint, ExecutedTransaction, GetCheckpointRequest, Object};
use sui_types::base_types::ObjectID;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::TransactionData;
use test_cluster::TestClusterBuilder;

use crate::{stake_with_validator, transfer_coin};

#[sim_test]
async fn get_checkpoint() {
    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .disable_fullnode_pruning()
        .build()
        .await;

    let _transaction_digest = transfer_coin(&test_cluster.wallet).await;
    let transaction_digest = stake_with_validator(&test_cluster).await;

    let mut client = LedgerServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    // Request with no provided read_mask
    let Checkpoint {
        sequence_number,
        digest,
        summary,
        signature,
        contents,
        transactions,
        objects,
        ..
    } = client
        .get_checkpoint(GetCheckpointRequest::default())
        .await
        .unwrap()
        .into_inner()
        .checkpoint
        .unwrap();

    assert!(sequence_number.is_some());
    assert!(digest.is_some());
    assert!(summary.is_none());
    assert!(signature.is_none());
    assert!(contents.is_none());
    assert!(transactions.is_empty());
    assert!(objects.is_none());

    // Request all fields
    let Checkpoint {
        sequence_number,
        digest,
        summary,
        signature,
        contents,
        transactions,
        objects,
        ..
    } = client
        .get_checkpoint(
            GetCheckpointRequest::latest().with_read_mask(FieldMask::from_paths([
                "sequence_number",
                "digest",
                "summary",
                "signature",
                "contents",
                "transactions",
                "objects",
            ])),
        )
        .await
        .unwrap()
        .into_inner()
        .checkpoint
        .unwrap();

    assert!(sequence_number.is_some());
    assert!(digest.is_some());
    assert!(summary.is_some());
    assert!(signature.is_some());
    assert!(contents.is_some());
    assert!(!transactions.is_empty());
    assert!(objects.is_some());

    // Request by digest
    let response = client
        .get_checkpoint({
            let mut message = GetCheckpointRequest::default();
            message.checkpoint_id = Some(CheckpointId::Digest(digest.clone().unwrap()));
            message
        })
        .await
        .unwrap()
        .into_inner()
        .checkpoint
        .unwrap();
    assert_eq!(response.digest, digest.to_owned());

    // Request by sequence_number
    let response = client
        .get_checkpoint(GetCheckpointRequest::by_sequence_number(
            sequence_number.unwrap(),
        ))
        .await
        .unwrap()
        .into_inner()
        .checkpoint
        .unwrap();
    assert_eq!(response.sequence_number, sequence_number.to_owned());
    assert_eq!(response.digest, digest.to_owned());

    // A Checkpoint that we know has a transaction that emitted an event
    let checkpoint = client
        .get_transaction(
            GetTransactionRequest::new(&transaction_digest)
                .with_read_mask(FieldMask::from_paths(["checkpoint"])),
        )
        .await
        .unwrap()
        .into_inner()
        .transaction
        .unwrap()
        .checkpoint
        .unwrap();

    let Checkpoint {
        sequence_number,
        digest,
        summary,
        signature,
        contents,
        transactions,
        objects,
        ..
    } = client
        .get_checkpoint(
            GetCheckpointRequest::by_sequence_number(checkpoint).with_read_mask(
                FieldMask::from_paths(["sequence_number", "digest", "transactions.digest"]),
            ),
        )
        .await
        .unwrap()
        .into_inner()
        .checkpoint
        .unwrap();

    assert!(sequence_number.is_some());
    assert!(digest.is_some());
    assert!(summary.is_none());
    assert!(signature.is_none());
    assert!(contents.is_none());
    assert!(objects.is_none());

    let mut found_transaction = false;
    for ExecutedTransaction {
        digest,
        transaction,
        effects,
        events,
        objects,
        signatures,
        checkpoint,
        timestamp,
        balance_changes,
        ..
    } in transactions
    {
        assert!(digest.is_some());
        if digest == Some(transaction_digest.to_string()) {
            found_transaction = true;
        }
        assert!(transaction.is_none());
        assert!(effects.is_none());
        assert!(events.is_none());
        assert!(objects.is_none());
        assert!(signatures.is_empty());
        assert!(checkpoint.is_none());
        assert!(timestamp.is_none());
        assert!(balance_changes.is_empty());
    }
    // Ensure we found the transaction we used for picking the checkpoint to test against
    assert!(found_transaction);

    // Request all fields
    let Checkpoint {
        sequence_number,
        digest,
        summary,
        signature,
        contents,
        transactions,
        objects,
        ..
    } = client
        .get_checkpoint(
            GetCheckpointRequest::by_sequence_number(checkpoint).with_read_mask(
                FieldMask::from_paths([
                    "sequence_number",
                    "digest",
                    "summary",
                    "signature",
                    "contents",
                    "transactions",
                    "objects",
                ]),
            ),
        )
        .await
        .unwrap()
        .into_inner()
        .checkpoint
        .unwrap();

    assert!(sequence_number.is_some());
    assert!(digest.is_some());
    assert!(summary.is_some());
    assert!(signature.is_some());
    assert!(contents.is_some());

    let mut found_transaction = false;
    for ExecutedTransaction {
        digest,
        transaction,
        effects,
        events,
        objects,
        ..
    } in transactions
    {
        assert!(digest.is_some());
        if digest == Some(transaction_digest.to_string()) {
            found_transaction = true;
            assert!(events.is_some());
        }
        assert!(transaction.is_some());
        assert!(effects.is_some());
        assert!(objects.is_none()); // This doesn't get populated by this API
    }

    for Object {
        bcs,
        object_id,
        version,
        digest,
        owner,
        object_type,
        ..
    } in objects.unwrap().objects
    {
        assert!(object_id.is_some());
        assert!(version.is_some());
        assert!(digest.is_some());
        assert!(bcs.is_some());
        assert!(owner.is_some());
        assert!(object_type.is_some());
    }

    // Ensure we found the transaction we used for picking the checkpoint to test against
    assert!(found_transaction);

    let Checkpoint { transactions, .. } = client
        .get_checkpoint(
            GetCheckpointRequest::by_sequence_number(checkpoint).with_read_mask(
                FieldMask::from_paths([
                    "sequence_number",
                    "transactions.digest",
                    "transactions.events.events.json",
                ]),
            ),
        )
        .await
        .unwrap()
        .into_inner()
        .checkpoint
        .unwrap();

    let mut found_transaction_with_events = false;
    for tx in transactions {
        if tx.digest == Some(transaction_digest.to_string()) {
            found_transaction_with_events = true;
            let events = tx.events.expect("events should be present");
            assert!(!events.events.is_empty(), "should have events");

            for event in events.events {
                let json = event
                    .json
                    .as_ref()
                    .expect("json field should be populated when requested in mask");

                let prost_types::value::Kind::StructValue(s) =
                    json.kind.as_ref().expect("json should have kind")
                else {
                    panic!("event json should be a struct value");
                };

                let amount = s
                    .fields
                    .get("amount")
                    .and_then(|v| {
                        if let Some(prost_types::value::Kind::StringValue(s)) = &v.kind {
                            Some(s.as_str())
                        } else {
                            None
                        }
                    })
                    .expect("amount should be a string");
                assert_eq!(amount, "30000000000000000");

                let epoch = s
                    .fields
                    .get("epoch")
                    .and_then(|v| {
                        if let Some(prost_types::value::Kind::StringValue(s)) = &v.kind {
                            Some(s.as_str())
                        } else {
                            None
                        }
                    })
                    .expect("epoch should be a string");
                assert_eq!(epoch, "0");

                for addr_field in ["pool_id", "staker_address", "validator_address"] {
                    let addr = s
                        .fields
                        .get(addr_field)
                        .and_then(|v| {
                            if let Some(prost_types::value::Kind::StringValue(s)) = &v.kind {
                                Some(s.as_str())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_else(|| panic!("{} should be a string", addr_field));
                    assert!(
                        addr.starts_with("0x"),
                        "{} should start with 0x",
                        addr_field
                    );
                    assert_eq!(
                        addr.len(),
                        66,
                        "{} should be 66 chars (0x + 64 hex)",
                        addr_field
                    );
                }
            }
        }
    }
    assert!(
        found_transaction_with_events,
        "should have found transaction with events"
    );

    tokio::time::sleep(std::time::Duration::from_secs(15)).await;
}

#[sim_test]
async fn get_checkpoint_exposes_execution_error_metadata() {
    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .disable_fullnode_pruning()
        .build()
        .await;

    let (sender, gas_object) = test_cluster
        .wallet
        .get_one_gas_object()
        .await
        .unwrap()
        .unwrap();
    let gas_price = test_cluster.wallet.get_reference_gas_price().await.unwrap();

    let mut builder = ProgrammableTransactionBuilder::new();
    builder
        .move_call(
            ObjectID::from_single_byte(1),
            ident_str!("option").to_owned(),
            ident_str!("bad_function").to_owned(),
            vec![],
            vec![],
        )
        .unwrap();
    let transaction_data = TransactionData::new_programmable(
        sender,
        vec![gas_object],
        builder.finish(),
        50_000_000,
        gas_price,
    );
    let signed_transaction = test_cluster
        .wallet
        .sign_transaction(&transaction_data)
        .await;

    let mut execution_client = Client::new(test_cluster.rpc_url().to_owned()).unwrap();
    let executed_transaction =
        super::super::execute_transaction_assert_failed(&mut execution_client, &signed_transaction)
            .await;
    let transaction_digest = executed_transaction.digest.unwrap();
    let checkpoint = executed_transaction.checkpoint.unwrap();

    let mut ledger_client = LedgerServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();
    let transactions = ledger_client
        .get_checkpoint(
            GetCheckpointRequest::by_sequence_number(checkpoint).with_read_mask(
                FieldMask::from_paths([
                    "transactions.digest",
                    "transactions.effects.status.error.metadata",
                ]),
            ),
        )
        .await
        .unwrap()
        .into_inner()
        .checkpoint
        .unwrap()
        .transactions;

    let transaction = transactions
        .into_iter()
        .find(|transaction| transaction.digest == Some(transaction_digest.clone()))
        .expect("checkpoint should include failed transaction");
    let metadata = transaction
        .effects
        .unwrap()
        .status
        .unwrap()
        .error
        .unwrap()
        .metadata
        .unwrap();

    assert!(
        metadata
            .message
            .as_deref()
            .is_some_and(|message| message.contains("bad_function")),
        "unexpected execution error metadata: {:?}",
        metadata.message
    );
}
