// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use sui_kvstore::CheckpointSpan;
use sui_kvstore::ObjectStatus;
use sui_kvstore::testing::{
    BigTableEmulator, INSTANCE_ID, create_tables, require_bigtable_emulator,
};
use sui_kvstore::{BigTableClient, KeyValueStoreReader, tables};
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::object::Object;
use sui_types::storage::ObjectKey;

#[tokio::test]
async fn test_get_latest_object_bounds_scan() -> Result<()> {
    require_bigtable_emulator();
    let emulator = tokio::task::spawn_blocking(BigTableEmulator::start)
        .await
        .context("spawn_blocking panicked")??;
    create_tables(emulator.host(), INSTANCE_ID).await?;

    let mut client =
        BigTableClient::new_local(emulator.host().to_string(), INSTANCE_ID.to_string())
            .await
            .context("Failed to create BigTable client")?;

    // Create two random object IDs and ensure B < A lexicographically.
    let mut id_a = ObjectID::random();
    let mut id_b = ObjectID::random();
    if id_a < id_b {
        std::mem::swap(&mut id_a, &mut id_b);
    }
    assert!(id_b < id_a);

    let obj_b = Object::immutable_with_id_for_testing(id_b);
    let key_b = ObjectKey(id_b, obj_b.version());

    // Write object B to the objects table, but do NOT write A.
    let cells = tables::objects::encode(&obj_b)?;
    let entry = tables::make_entry(tables::objects::encode_key(&key_b), cells, None);
    client
        .write_entries(
            tables::objects::NAME,
            vec![entry],
            CheckpointSpan::single(0),
        )
        .await?;

    // Query for the latest version of A (which does not exist).
    // The prefix boundary of our fix should prevent the scan from bleeding backward into B's keys!
    let latest_a = client.get_latest_object(&id_a).await?;
    assert!(
        latest_a.is_none(),
        "Querying missing object A returned a value! (likely bled into B)"
    );

    // Query for the latest version of B (which does exist).
    let latest_b = client.get_latest_object(&id_b).await?;
    assert!(
        latest_b.is_some(),
        "Querying existing object B returned None!"
    );
    let found_obj = latest_b.unwrap();
    assert_eq!(found_obj.id(), id_b);

    Ok(())
}

/// A checkpoint-index lookup reads only its own object's rows: a missing object resolves to
/// nothing at every checkpoint, though its neighbours on both sides have rows at the first and
/// last checkpoints, and an object with rows resolves to its newest row at or below the
/// checkpoint.
#[tokio::test]
async fn object_version_at_checkpoint_stays_within_its_object() -> Result<()> {
    require_bigtable_emulator();
    let emulator = tokio::task::spawn_blocking(BigTableEmulator::start)
        .await
        .context("spawn_blocking panicked")??;
    create_tables(emulator.host(), INSTANCE_ID).await?;

    let mut client =
        BigTableClient::new_local(emulator.host().to_string(), INSTANCE_ID.to_string())
            .await
            .context("Failed to create BigTable client")?;

    let missing = ObjectID::from_single_byte(0x42);
    let neighbours = [
        ObjectID::from_single_byte(0x41),
        ObjectID::from_single_byte(0x43),
    ];
    // Each neighbour's version at each checkpoint is the checkpoint plus one, so a result names
    // the row it came from.
    let checkpoints = [0, 10, u64::MAX];
    let mut entries = vec![];
    for id in neighbours {
        for checkpoint in checkpoints {
            entries.push(tables::make_entry(
                tables::object_versions_by_checkpoint::encode_key(&id, checkpoint),
                tables::object_versions_by_checkpoint::encode(
                    SequenceNumber::from_u64(checkpoint.saturating_add(1)),
                    ObjectStatus::Live,
                ),
                None,
            ));
        }
    }
    client
        .write_entries(
            tables::object_versions_by_checkpoint::NAME,
            entries,
            CheckpointSpan::single(0),
        )
        .await?;

    for checkpoint in [0, 1, 10, 11, u64::MAX - 1, u64::MAX] {
        assert_eq!(
            client
                .get_object_version_at_checkpoint(missing, checkpoint)
                .await?,
            None,
            "index read at checkpoint {checkpoint} bled into a neighbour",
        );
        let newest = checkpoints
            .into_iter()
            .filter(|&c| c <= checkpoint)
            .max()
            .expect("a neighbour row at checkpoint 0");
        for id in neighbours {
            assert_eq!(
                client
                    .get_object_version_at_checkpoint(id, checkpoint)
                    .await?,
                Some((
                    SequenceNumber::from_u64(newest.saturating_add(1)),
                    ObjectStatus::Live,
                )),
                "{id} at checkpoint {checkpoint}",
            );
        }
    }

    Ok(())
}
