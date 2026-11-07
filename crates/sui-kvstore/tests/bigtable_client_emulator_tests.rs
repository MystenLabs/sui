// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use sui_kvstore::CheckpointSpan;
use sui_kvstore::ObjectSchema;
use sui_kvstore::ObjectStatus;
use sui_kvstore::testing::{
    ASCENDING_OBJECT_VERSIONS_NEWEST_RANGE, BigTableEmulator, INSTANCE_ID, create_tables,
    require_bigtable_emulator,
};
use sui_kvstore::{BigTableClient, KeyValueStoreReader, tables};
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::object::Object;
use sui_types::storage::ObjectKey;

/// Reads of a missing object never return the rows of the objects on either side of it, in
/// either objects schema or in the checkpoint index, whichever direction the read scans.
#[tokio::test]
async fn object_reads_stay_within_their_object() -> Result<()> {
    require_bigtable_emulator();
    let emulator = tokio::task::spawn_blocking(BigTableEmulator::start)
        .await
        .context("spawn_blocking panicked")??;
    create_tables(emulator.host(), INSTANCE_ID).await?;

    let client = BigTableClient::new_local(emulator.host().to_string(), INSTANCE_ID.to_string())
        .await
        .context("Failed to create BigTable client")?;

    let missing = ObjectID::from_single_byte(0x42);
    let neighbours = [
        ObjectID::from_single_byte(0x41),
        ObjectID::from_single_byte(0x43),
    ];
    // Versions at both ends of the key space, so a scan past either bound of `missing` finds one.
    let versions = [0, 1, SequenceNumber::MAX.value(), u64::MAX - 1];

    for schema in [ObjectSchema::V1, ObjectSchema::V2] {
        let mut client = client.clone().with_object_schema(schema);
        let mut entries = vec![];
        for id in neighbours {
            for version in versions {
                let version = SequenceNumber::from_u64(version);
                let object = Object::with_id_owner_version_for_testing(
                    id,
                    version,
                    sui_types::object::Owner::AddressOwner(Default::default()),
                );
                entries.push(tables::make_entry(
                    tables::objects::encode_key(schema, &ObjectKey(id, version)),
                    tables::objects::encode(&object)?,
                    None,
                ));
            }
        }
        client
            .write_entries(
                tables::objects::table_name(schema),
                entries,
                CheckpointSpan::single(0),
            )
            .await?;

        assert!(
            client.get_latest_object(&missing).await?.is_none(),
            "{schema:?}: latest read of a missing object bled into a neighbour",
        );
        for version in versions {
            let version = SequenceNumber::from_u64(version);
            assert!(
                client
                    .get_object_lt_or_eq_version(missing, version)
                    .await?
                    .is_none(),
                "{schema:?}: read at or below {version} bled into a neighbour",
            );
        }
        for descending in [false, true] {
            let page = client
                .get_live_object_versions(
                    missing,
                    SequenceNumber::MIN..=SequenceNumber::from_u64(u64::MAX),
                    10,
                    descending,
                )
                .await?;
            assert!(
                page.is_empty(),
                "{schema:?}: page (descending: {descending}) bled into a neighbour",
            );
        }
        for id in neighbours {
            let latest = client.get_latest_object(&id).await?;
            assert_eq!(
                latest.map(|o| (o.id(), o.version().value())),
                Some((id, u64::MAX - 1)),
                "{schema:?}",
            );
        }
    }

    // The checkpoint index, with neighbour rows at the first and last checkpoints.
    let mut client = client;
    let mut entries = vec![];
    for id in neighbours {
        for checkpoint in [0, u64::MAX] {
            entries.push(tables::make_entry(
                tables::object_versions_by_checkpoint::encode_key(&id, checkpoint),
                tables::object_versions_by_checkpoint::encode(
                    SequenceNumber::from_u64(1),
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
    for checkpoint in [0, 1, u64::MAX] {
        assert_eq!(
            client
                .get_object_version_at_checkpoint(missing, checkpoint)
                .await?,
            None,
            "index read at checkpoint {checkpoint} bled into a neighbour",
        );
    }

    Ok(())
}

/// An ascending [`ObjectSchema::V2`] page over more than
/// [`ASCENDING_OBJECT_VERSIONS_NEWEST_RANGE`] versions is one reversed read of two ranges.
/// BigTable returns the live versions of both ranges in ascending order, skips the tombstones in
/// each without counting them toward the limit, stops at the limit in either range, and never
/// returns a neighbouring object's rows.
#[tokio::test]
async fn ascending_object_versions_pages_span_both_ranges() -> Result<()> {
    require_bigtable_emulator();
    let emulator = tokio::task::spawn_blocking(BigTableEmulator::start)
        .await
        .context("spawn_blocking panicked")??;
    create_tables(emulator.host(), INSTANCE_ID).await?;
    let mut client =
        BigTableClient::new_local(emulator.host().to_string(), INSTANCE_ID.to_string())
            .await
            .context("Failed to create BigTable client")?
            .with_object_schema(ObjectSchema::V2);

    let width = ASCENDING_OBJECT_VERSIONS_NEWEST_RANGE;
    let newest = 3 * width + 5;
    // A page from version 0 to `newest` reads `[0, split]`, then `(split, newest]`.
    let split = newest - width;
    let object = ObjectID::from_single_byte(0x42);
    let live = [1, split - 2, split, split + 1, split + 3, newest];
    let tombstones = [split - 1, split + 2];
    let neighbours = [
        ObjectID::from_single_byte(0x41),
        ObjectID::from_single_byte(0x43),
    ];

    let live_entry = |id, version| -> Result<_> {
        let version = SequenceNumber::from_u64(version);
        let object = Object::with_id_owner_version_for_testing(
            id,
            version,
            sui_types::object::Owner::AddressOwner(Default::default()),
        );
        Ok(tables::make_entry(
            tables::objects::encode_key(ObjectSchema::V2, &ObjectKey(id, version)),
            tables::objects::encode(&object)?,
            None,
        ))
    };
    let mut entries = vec![];
    for version in live {
        entries.push(live_entry(object, version)?);
    }
    for version in tombstones {
        entries.push(tables::make_entry(
            tables::objects::encode_key(
                ObjectSchema::V2,
                &ObjectKey(object, SequenceNumber::from_u64(version)),
            ),
            tables::objects::encode_tombstone(ObjectStatus::Wrapped)?,
            None,
        ));
    }
    for id in neighbours {
        for version in [0, split - 1, split + 2, newest, u64::MAX - 1] {
            entries.push(live_entry(id, version)?);
        }
    }
    client
        .write_entries(tables::objects::NAME_V2, entries, CheckpointSpan::single(0))
        .await?;

    let page = |lo: u64, limit: usize| {
        let mut client = client.clone();
        async move {
            let objects = client
                .get_live_object_versions(
                    object,
                    SequenceNumber::from_u64(lo)..=SequenceNumber::from_u64(newest),
                    limit,
                    false,
                )
                .await?;
            assert!(objects.iter().all(|o| o.id() == object), "read a neighbour");
            anyhow::Ok(
                objects
                    .iter()
                    .map(|o| o.version().value())
                    .collect::<Vec<_>>(),
            )
        }
    };

    // From version 0, every limit stops in the range where it is reached.
    for limit in 1..=live.len() + 1 {
        let expected: Vec<u64> = live.iter().copied().take(limit).collect();
        assert_eq!(page(0, limit).await?, expected, "limit {limit}");
    }

    // Pages of two, each starting after the previous one, list every live version once. The
    // second page starts at a tombstone and ends across the split.
    let mut listed: Vec<u64> = vec![];
    let mut lo = 0;
    loop {
        let versions = page(lo, 2).await?;
        listed.extend(&versions);
        match versions.last() {
            Some(&last) if versions.len() == 2 => lo = last + 1,
            _ => break,
        }
    }
    assert_eq!(listed, live);

    Ok(())
}
