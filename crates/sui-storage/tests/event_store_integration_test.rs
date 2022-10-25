// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_json_rpc_types::SuiEventEnvelope;
use sui_storage::event_store::{sql::SqlEventStore, test_utils, EventStore};
use sui_types::{
    base_types::{ObjectID, TransactionDigest},
    event::{EventEnvelope, EventType, TransferType},
};
#[tokio::test]
async fn test_stored_event_to_sui_event() -> Result<(), anyhow::Error> {
    telemetry_subscribers::init_for_testing();

    let db = SqlEventStore::new_memory_only_not_prod().await?;
    db.initialize().await.map_err(anyhow::Error::from)?;

    let new_obj = test_utils::new_test_newobj_event(
        1_666_000,
        TransactionDigest::random(),
        1,
        0, // event_num
        None,
        None,
        None,
    );
    insert_and_fetch_by_tx_digest_then_compare(new_obj, &db).await?;

    let move_ = test_utils::new_test_move_event(
        1_666_001,
        TransactionDigest::random(),
        2,
        0, // event_num
        ObjectID::from_hex_literal("0x3").unwrap(),
        "a_module",
        "whatever",
    );
    insert_and_fetch_by_tx_digest_then_compare(move_, &db).await?;

    let delete_obj = test_utils::new_test_deleteobj_event(
        1_666_002,
        TransactionDigest::random(),
        3,
        0, // event_num
        None,
        None,
    );
    insert_and_fetch_by_tx_digest_then_compare(delete_obj, &db).await?;

    let transfer_obj = test_utils::new_test_transfer_event(
        1_666_003,
        TransactionDigest::random(),
        4,
        0, // event_num
        1,
        TransferType::ToAddress,
        None,
        None,
        None,
    );
    insert_and_fetch_by_tx_digest_then_compare(transfer_obj, &db).await?;

    let publish = test_utils::new_test_publish_event(
        1_001_000,
        TransactionDigest::random(),
        5,
        0, // event_num
        None,
    );
    assert_eq!(db.add_events(&vec![publish.clone()]).await?, 1);
    let mut queried_events = db
        .events_by_type(1_001_000, 1_002_000, EventType::Publish, 1)
        .await?;
    assert_eq!(queried_events.len(), 1);
    let sui_event: SuiEventEnvelope = queried_events.swap_remove(0).try_into()?;
    assert!(
        publish == sui_event,
        "event: {publish:?}\nsui_event: {sui_event:?}"
    );
    Ok(())
}

async fn insert_and_fetch_by_tx_digest_then_compare(
    event_envelope: EventEnvelope,
    db: &SqlEventStore,
) -> Result<(), anyhow::Error> {
    let tx_digest = event_envelope.tx_digest.unwrap();
    assert_eq!(db.add_events(&vec![event_envelope.clone()]).await?, 1);

    let mut events = db.events_by_transaction(tx_digest, 10).await?;
    assert_eq!(events.len(), 1);
    let stored_event = events.pop().unwrap();
    let sui_event: SuiEventEnvelope = stored_event.try_into()?;
    assert!(
        event_envelope == sui_event,
        "event: {event_envelope:?}\nsui_event: {sui_event:?}"
    );
    Ok(())
}
