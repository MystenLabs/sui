// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::ident_str;
use sui_indexer_alt_e2e_tests::FullCluster;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::GetObjectRequest;
use sui_rpc::proto::sui::rpc::v2::GetTransactionRequest;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::object::Owner;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::Transaction;
use sui_types::transaction::TransactionData;

/// 5 SUI gas budget
const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;

#[tokio::test]
async fn test_json_read_mask() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, gas) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();

    // Publish the emit_test_event package so we can emit events.
    let path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packages/event/emit_test_event");
    let (publish_fx, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(
            TestTransactionBuilder::new(sender, gas, cluster.reference_gas_price())
                .with_gas_budget(DEFAULT_GAS_BUDGET)
                .publish(path)
                .build(),
            vec![&kp],
        ))
        .expect("Failed to publish");

    let pkg_id = publish_fx
        .created()
        .into_iter()
        .find_map(|((id, v, _), owner)| {
            (v.value() == 1 && matches!(owner, Owner::Immutable)).then_some(id)
        })
        .expect("Failed to find package ID");

    // Get updated gas ref after publish.
    let gas = publish_fx
        .mutated()
        .into_iter()
        .find(|((id, _, _), _)| *id == gas.0)
        .map(|((id, version, digest), _)| (id, version, digest))
        .expect("gas object should be mutated");

    // Call emit_test_event to create a transaction with events.
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.programmable_move_call(
        pkg_id,
        ident_str!("emit_test_event").to_owned(),
        ident_str!("emit_test_event").to_owned(),
        vec![],
        vec![],
    );
    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );
    let (event_fx, error) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp]))
        .expect("emit_test_event failed");
    assert!(error.is_none(), "emit_test_event failed: {error:?}");
    let event_tx_digest = *event_fx.transaction_digest();

    cluster.create_checkpoint().await;

    let mut client = LedgerServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    // -- Object JSON: requested --
    {
        let object = client
            .get_object({
                let mut req = GetObjectRequest::default();
                req.object_id = Some(gas.0.to_canonical_string(true));
                req.read_mask = Some(FieldMask::from_paths(["json", "object_id"]));
                req
            })
            .await
            .unwrap()
            .into_inner()
            .object
            .expect("object should be present");

        // Coin<SUI> renders as a struct with `id` (UID as hex string) and `balance` (u64 as string).
        let json = object
            .json
            .expect("json should be populated for a Move object");
        let fields = match json.kind {
            Some(prost_types::value::Kind::StructValue(s)) => s.fields,
            other => panic!("expected struct value, got: {other:?}"),
        };
        assert_eq!(
            fields.len(),
            2,
            "Coin<SUI> should have exactly 2 fields (id, balance), got: {:?}",
            fields.keys().collect::<Vec<_>>()
        );
        assert!(fields.contains_key("id"), "missing 'id' field");
        assert!(fields.contains_key("balance"), "missing 'balance' field");

        // The id should be the object's hex address.
        let id_value = fields["id"].kind.as_ref().unwrap();
        match id_value {
            prost_types::value::Kind::StringValue(s) => {
                assert_eq!(s, &gas.0.to_canonical_string(true));
            }
            other => panic!("expected id to be a string, got: {other:?}"),
        }
    }

    // -- Object JSON: not requested --
    {
        let object = client
            .get_object({
                let mut req = GetObjectRequest::default();
                req.object_id = Some(gas.0.to_canonical_string(true));
                req.read_mask = Some(FieldMask::from_paths(["object_id", "version"]));
                req
            })
            .await
            .unwrap()
            .into_inner()
            .object
            .expect("object should be present");

        assert!(
            object.json.is_none(),
            "json should not be populated when not requested"
        );
    }

    // -- Transaction event JSON --
    {
        let tx = client
            .get_transaction({
                let mut req = GetTransactionRequest::default();
                req.digest = Some(event_tx_digest.to_string());
                req.read_mask = Some(FieldMask::from_paths([
                    "digest",
                    "events.events.json",
                    "events.events.event_type",
                ]));
                req
            })
            .await
            .unwrap()
            .into_inner()
            .transaction
            .expect("transaction should be present");

        assert_eq!(tx.digest(), event_tx_digest.to_string());

        let events = tx.events.expect("events should be present");
        assert_eq!(events.events.len(), 1, "expected exactly 1 event");

        let event = &events.events[0];
        assert!(
            event.event_type().contains("emit_test_event::TestEvent"),
            "unexpected event type: {}",
            event.event_type()
        );

        // The event JSON should be a struct with a single `value: 1` field.
        let json = event.json.as_ref().expect("event json should be populated");
        let fields = match &json.kind {
            Some(prost_types::value::Kind::StructValue(s)) => &s.fields,
            other => panic!("expected struct value, got: {other:?}"),
        };
        assert_eq!(fields.len(), 1, "TestEvent has one field");
        let value = fields["value"].kind.as_ref().unwrap();
        match value {
            prost_types::value::Kind::StringValue(s) => {
                assert_eq!(s, "1", "TestEvent.value should be 1");
            }
            other => panic!("expected value to be a string, got: {other:?}"),
        }
    }
}
