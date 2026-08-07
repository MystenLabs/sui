// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use sui_kvstore::testing::{
    BigTableEmulator, INSTANCE_ID, create_tables, require_bigtable_emulator,
};
use sui_kvstore::{BigTableClient, KeyValueStoreReader, tables};
use sui_types::base_types::ObjectID;
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
        .write_entries(tables::objects::NAME, vec![entry])
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

#[tokio::test]
async fn test_get_package_latest_pages_past_row_cap() -> Result<()> {
    require_bigtable_emulator();
    let emulator = tokio::task::spawn_blocking(BigTableEmulator::start)
        .await
        .context("spawn_blocking panicked")??;
    create_tables(emulator.host(), INSTANCE_ID).await?;

    let mut client =
        BigTableClient::new_local(emulator.host().to_string(), INSTANCE_ID.to_string())
            .await
            .context("Failed to create BigTable client")?;

    // Versions 1..=60, version v published at checkpoint 100 + v. Queried at the bound of
    // version 1, 59 later versions must be scanned past — more than one 50-row page.
    let original_id = ObjectID::random();
    let entries = (1..=60u64)
        .map(|version| {
            tables::make_entry(
                tables::packages::encode_key(original_id.as_ref(), version),
                tables::packages::encode(100 + version, original_id.as_ref(), false),
                None,
            )
        })
        .collect::<Vec<_>>();
    client
        .write_entries(tables::packages::NAME, entries)
        .await?;

    let latest = client.get_package_latest(original_id, 101).await?;
    let pkg = latest.expect("version 1 is visible at checkpoint 101");
    assert_eq!(pkg.package_version, 1);
    assert_eq!(pkg.cp_sequence_number, 101);

    // Bound before any version exists.
    assert!(client.get_package_latest(original_id, 100).await?.is_none());

    // Unbounded-in-practice lookup resolves the newest version.
    let latest = client.get_package_latest(original_id, u64::MAX).await?;
    assert_eq!(latest.expect("package exists").package_version, 60);

    Ok(())
}

#[tokio::test]
async fn test_get_package_versions_pages_past_fetch_cap() -> Result<()> {
    require_bigtable_emulator();
    let emulator = tokio::task::spawn_blocking(BigTableEmulator::start)
        .await
        .context("spawn_blocking panicked")??;
    create_tables(emulator.host(), INSTANCE_ID).await?;

    let mut client =
        BigTableClient::new_local(emulator.host().to_string(), INSTANCE_ID.to_string())
            .await
            .context("Failed to create BigTable client")?;

    // Versions 1..=250, version v published at checkpoint 100 + v.
    let original_id = ObjectID::random();
    let entries = (1..=250u64)
        .map(|version| {
            tables::make_entry(
                tables::packages::encode_key(original_id.as_ref(), version),
                tables::packages::encode(100 + version, original_id.as_ref(), false),
                None,
            )
        })
        .collect::<Vec<_>>();
    client
        .write_entries(tables::packages::NAME, entries)
        .await?;

    // A page larger than the old fixed 200-row fetch cap comes back complete.
    let versions = client
        .get_package_versions(original_id, u64::MAX, None, None, 250, false)
        .await?;
    assert_eq!(versions.len(), 250);
    assert_eq!(versions.first().map(|pkg| pkg.package_version), Some(1));
    assert_eq!(versions.last().map(|pkg| pkg.package_version), Some(250));

    // Descending at a bound that filters out the newest 150 versions: the scan must page past
    // all of them to fill the page with versions 100..=51.
    let versions = client
        .get_package_versions(original_id, 200, None, None, 50, true)
        .await?;
    assert_eq!(versions.first().map(|pkg| pkg.package_version), Some(100));
    assert_eq!(versions.last().map(|pkg| pkg.package_version), Some(51));
    assert_eq!(versions.len(), 50);

    // Version bounds still apply on top of paging.
    let versions = client
        .get_package_versions(original_id, u64::MAX, Some(10), Some(21), 100, false)
        .await?;
    assert_eq!(versions.len(), 10);
    assert_eq!(versions.first().map(|pkg| pkg.package_version), Some(11));
    assert_eq!(versions.last().map(|pkg| pkg.package_version), Some(20));

    Ok(())
}

#[tokio::test]
async fn test_get_system_packages_fills_page_past_filtered_rows() -> Result<()> {
    require_bigtable_emulator();
    let emulator = tokio::task::spawn_blocking(BigTableEmulator::start)
        .await
        .context("spawn_blocking panicked")??;
    create_tables(emulator.host(), INSTANCE_ID).await?;

    let mut client =
        BigTableClient::new_local(emulator.host().to_string(), INSTANCE_ID.to_string())
            .await
            .context("Failed to create BigTable client")?;

    // Six system packages in key order: the first three first appear after the queried bound,
    // the last three before it. A raw-row limit of 3 sees only the ineligible rows.
    let mut ids: Vec<ObjectID> = (0..6).map(|_| ObjectID::random()).collect();
    ids.sort();
    let (ineligible, eligible) = ids.split_at(3);

    let mut system_entries = Vec::new();
    let mut package_entries = Vec::new();
    for id in ineligible {
        system_entries.push(tables::make_entry(
            tables::system_packages::encode_key(id.as_ref()),
            tables::system_packages::encode(100),
            None,
        ));
        package_entries.push(tables::make_entry(
            tables::packages::encode_key(id.as_ref(), 1),
            tables::packages::encode(100, id.as_ref(), true),
            None,
        ));
    }
    for id in eligible {
        system_entries.push(tables::make_entry(
            tables::system_packages::encode_key(id.as_ref()),
            tables::system_packages::encode(1),
            None,
        ));
        package_entries.push(tables::make_entry(
            tables::packages::encode_key(id.as_ref(), 1),
            tables::packages::encode(1, id.as_ref(), true),
            None,
        ));
    }
    client
        .write_entries(tables::system_packages::NAME, system_entries)
        .await?;
    client
        .write_entries(tables::packages::NAME, package_entries)
        .await?;

    let results = client.get_system_packages(10, None, 3).await?;
    let found: Vec<&[u8]> = results
        .iter()
        .map(|pkg| pkg.original_id.as_slice())
        .collect();
    let expected: Vec<&[u8]> = eligible.iter().map(|id| id.as_ref()).collect();
    assert_eq!(found, expected);

    // Cursoring past the first eligible package returns the remaining two.
    let results = client.get_system_packages(10, Some(eligible[0]), 3).await?;
    assert_eq!(results.len(), 2);

    Ok(())
}
