// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use rstest::rstest;

fn temp_dir() -> std::path::PathBuf {
    tempfile::tempdir()
        .expect("Failed to open temporary directory")
        .keep()
}

fn get_iter<K, V>(db: &DBMap<K, V>) -> impl Iterator<Item = (K, V)> + use<'_, K, V>
where
    K: Serialize + DeserializeOwned,
    V: Serialize + DeserializeOwned,
{
    db.safe_iter().map(|item| item.unwrap())
}

fn get_reverse_iter<K, V>(
    db: &DBMap<K, V>,
    lower_bound: Option<K>,
    upper_bound: Option<K>,
) -> impl Iterator<Item = Result<(K, V), TypedStoreError>> + use<'_, K, V>
where
    K: Serialize + DeserializeOwned,
    V: Serialize + DeserializeOwned,
{
    db.reversed_safe_iter_with_bounds(lower_bound, upper_bound)
        .unwrap()
}

fn get_iter_with_bounds<K, V>(
    db: &DBMap<K, V>,
    lower_bound: Option<K>,
    upper_bound: Option<K>,
) -> impl Iterator<Item = (K, V)> + use<'_, K, V>
where
    K: Serialize + DeserializeOwned,
    V: Serialize + DeserializeOwned,
{
    db.safe_iter_with_bounds(lower_bound, upper_bound)
        .map(|item| item.unwrap())
}

fn get_range_iter<'a, K, V>(
    db: &'a DBMap<K, V>,
    range: impl RangeBounds<K> + 'a,
) -> impl Iterator<Item = (K, V)> + 'a
where
    K: Serialize + DeserializeOwned,
    V: Serialize + DeserializeOwned,
{
    db.safe_range_iter(range).map(|item| item.unwrap())
}

#[tokio::test]
async fn test_open() {
    let _db = open_map::<_, u32, String>(temp_dir(), None);
}

#[tokio::test]
async fn test_reopen() {
    let arc = {
        let db = open_map::<_, u32, String>(temp_dir(), None);
        db.insert(&123456789, &"123456789".to_string())
            .expect("Failed to insert");
        db
    };
    let db = DBMap::<u32, String>::reopen(&arc.db, None, &ReadWriteOptions::default(), false)
        .expect("Failed to re-open storage");
    assert!(
        db.contains_key(&123456789)
            .expect("Failed to retrieve item in storage")
    );
}

#[tokio::test]
async fn test_contains_key() {
    let db = open_map(temp_dir(), None);

    db.insert(&123456789, &"123456789".to_string())
        .expect("Failed to insert");
    assert!(
        db.contains_key(&123456789)
            .expect("Failed to call contains key")
    );
    assert!(
        !db.contains_key(&000000000)
            .expect("Failed to call contains key")
    );
}

#[tokio::test]
async fn test_safe_drop_db() {
    let root_path = temp_dir();

    let tmp_path = root_path.join("test-0");
    {
        let db: DBMap<i32, String> = open_map(tmp_path.clone(), Some("table-0"));
        db.insert(&777, &"123".to_string()).unwrap();
    }
    safe_drop_db(tmp_path, Duration::from_secs(30))
        .await
        .unwrap();
}

#[tokio::test]
async fn test_multi_contain() {
    let db = open_map(temp_dir(), None);

    db.insert(&123, &"123".to_string())
        .expect("Failed to insert");
    db.insert(&456, &"456".to_string())
        .expect("Failed to insert");
    db.insert(&789, &"789".to_string())
        .expect("Failed to insert");

    let result = db
        .multi_contains_keys([123, 456])
        .expect("Failed to check multi keys existence");

    assert_eq!(result.len(), 2);
    assert!(result[0]);
    assert!(result[1]);

    let result = db
        .multi_contains_keys([123, 987, 789])
        .expect("Failed to check multi keys existence");

    assert_eq!(result.len(), 3);
    assert!(result[0]);
    assert!(!result[1]);
    assert!(result[2]);
}

#[tokio::test]
async fn test_get() {
    let db = open_map(temp_dir(), None);

    db.insert(&123456789, &"123456789".to_string())
        .expect("Failed to insert");
    assert_eq!(
        Some("123456789".to_string()),
        db.get(&123456789).expect("Failed to get")
    );
    assert_eq!(None, db.get(&000000000).expect("Failed to get"));
}

#[tokio::test]
async fn test_multi_get() {
    let db = open_map(temp_dir(), None);

    db.insert(&123, &"123".to_string())
        .expect("Failed to insert");
    db.insert(&456, &"456".to_string())
        .expect("Failed to insert");

    let result = db.multi_get([123, 456, 789]).expect("Failed to multi get");

    assert_eq!(result.len(), 3);
    assert_eq!(result[0], Some("123".to_string()));
    assert_eq!(result[1], Some("456".to_string()));
    assert_eq!(result[2], None);
}

#[tokio::test]
async fn test_skip() {
    let db = open_map(temp_dir(), None);

    db.insert(&123, &"123".to_string())
        .expect("Failed to insert");
    db.insert(&456, &"456".to_string())
        .expect("Failed to insert");
    db.insert(&789, &"789".to_string())
        .expect("Failed to insert");

    // Skip all smaller
    let key_vals: Vec<_> = get_iter_with_bounds(&db, Some(456), None).collect();
    assert_eq!(key_vals.len(), 2);
    assert_eq!(key_vals[0], (456, "456".to_string()));
    assert_eq!(key_vals[1], (789, "789".to_string()));

    // Skip to the end
    assert_eq!(get_iter_with_bounds(&db, Some(999), None).count(), 0);

    // Skip to last
    assert_eq!(
        get_reverse_iter(&db, None, None).next(),
        Some(Ok((789, "789".to_string()))),
    );

    // Skip to successor of first value
    assert_eq!(get_iter_with_bounds(&db, Some(000), None).count(), 3);
    assert_eq!(get_iter_with_bounds(&db, Some(000), None).count(), 3);
}

#[tokio::test]
async fn test_reverse_iter_with_bounds() {
    let db = open_map(temp_dir(), None);
    db.insert(&123, &"123".to_string())
        .expect("Failed to insert");
    db.insert(&456, &"456".to_string())
        .expect("Failed to insert");
    db.insert(&789, &"789".to_string())
        .expect("Failed to insert");

    let mut iter = get_reverse_iter(&db, None, Some(999));
    assert_eq!(iter.next().unwrap(), Ok((789, "789".to_string())));

    db.insert(&999, &"999".to_string())
        .expect("Failed to insert");
    let mut iter = get_reverse_iter(&db, None, Some(999));
    assert_eq!(iter.next().unwrap(), Ok((999, "999".to_string())));

    let mut iter = get_reverse_iter(&db, None, None);
    assert_eq!(iter.next().unwrap(), Ok((999, "999".to_string())));
}

#[tokio::test]
async fn test_remove() {
    let db = open_map(temp_dir(), None);

    db.insert(&123456789, &"123456789".to_string())
        .expect("Failed to insert");
    assert!(db.get(&123456789).expect("Failed to get").is_some());

    db.remove(&123456789).expect("Failed to remove");
    assert!(db.get(&123456789).expect("Failed to get").is_none());
}

#[tokio::test]
async fn test_iter() {
    let db = open_map(temp_dir(), None);
    db.insert(&123456789, &"123456789".to_string())
        .expect("Failed to insert");
    db.insert(&987654321, &"987654321".to_string())
        .expect("Failed to insert");

    let mut iter = get_iter(&db);

    assert_eq!(Some((123456789, "123456789".to_string())), iter.next());
    assert_eq!(Some((987654321, "987654321".to_string())), iter.next());
    assert_eq!(None, iter.next());
}

#[tokio::test]
async fn test_iter_reverse() {
    let db = open_map(temp_dir(), None);

    db.insert(&1, &"1".to_string()).expect("Failed to insert");
    db.insert(&2, &"2".to_string()).expect("Failed to insert");
    db.insert(&3, &"3".to_string()).expect("Failed to insert");

    let mut iter = get_reverse_iter(&db, None, None);
    assert_eq!(Some(Ok((3, "3".to_string()))), iter.next());
    assert_eq!(Some(Ok((2, "2".to_string()))), iter.next());
    assert_eq!(Some(Ok((1, "1".to_string()))), iter.next());
    assert_eq!(None, iter.next());

    let mut iter = get_iter_with_bounds(&db, Some(1), None);
    assert_eq!(Some((1, "1".to_string())), iter.next());
    assert_eq!(Some((2, "2".to_string())), iter.next());
}

#[tokio::test]
async fn test_insert_batch() {
    let db = open_map(temp_dir(), None);
    let keys_vals = (1..100).map(|i| (i, i.to_string()));
    let mut insert_batch = db.batch();
    insert_batch
        .insert_batch(&db, keys_vals.clone())
        .expect("Failed to batch insert");
    insert_batch.write().expect("Failed to execute batch");
    for (k, v) in keys_vals {
        let val = db.get(&k).expect("Failed to get inserted key");
        assert_eq!(Some(v), val);
    }
}

#[tokio::test]
async fn test_insert_batch_across_cf() {
    let rocks = open_rocksdb(temp_dir(), &["First_CF", "Second_CF"]);

    let db_cf_1 = DBMap::reopen(
        &rocks,
        Some("First_CF"),
        &ReadWriteOptions::default(),
        false,
    )
    .expect("Failed to open storage");
    let keys_vals_1 = (1..100).map(|i| (i, i.to_string()));

    let db_cf_2 = DBMap::reopen(
        &rocks,
        Some("Second_CF"),
        &ReadWriteOptions::default(),
        false,
    )
    .expect("Failed to open storage");
    let keys_vals_2 = (1000..1100).map(|i| (i, i.to_string()));

    let mut batch = db_cf_1.batch();
    batch
        .insert_batch(&db_cf_1, keys_vals_1.clone())
        .expect("Failed to batch insert")
        .insert_batch(&db_cf_2, keys_vals_2.clone())
        .expect("Failed to batch insert");

    batch.write().expect("Failed to execute batch");
    for (k, v) in keys_vals_1 {
        let val = db_cf_1.get(&k).expect("Failed to get inserted key");
        assert_eq!(Some(v), val);
    }

    for (k, v) in keys_vals_2 {
        let val = db_cf_2.get(&k).expect("Failed to get inserted key");
        assert_eq!(Some(v), val);
    }
}

#[tokio::test]
async fn test_insert_batch_across_different_db() {
    let rocks = open_rocksdb(temp_dir(), &["First_CF", "Second_CF"]);
    let rocks2 = open_rocksdb(temp_dir(), &["First_CF", "Second_CF"]);

    let db_cf_1: DBMap<i32, String> = DBMap::reopen(
        &rocks,
        Some("First_CF"),
        &ReadWriteOptions::default(),
        false,
    )
    .expect("Failed to open storage");
    let keys_vals_1 = (1..100).map(|i| (i, i.to_string()));

    let db_cf_2: DBMap<i32, String> = DBMap::reopen(
        &rocks2,
        Some("Second_CF"),
        &ReadWriteOptions::default(),
        false,
    )
    .expect("Failed to open storage");
    let keys_vals_2 = (1000..1100).map(|i| (i, i.to_string()));

    assert!(
        db_cf_1
            .batch()
            .insert_batch(&db_cf_1, keys_vals_1)
            .expect("Failed to batch insert")
            .insert_batch(&db_cf_2, keys_vals_2)
            .is_err()
    );
}

#[tokio::test]
async fn test_delete_batch() {
    let db = open_map::<_, u32, String>(temp_dir(), None);

    let keys_vals = (1..100).map(|i| (i, i.to_string()));
    let mut batch = db.batch();
    batch
        .insert_batch(&db, keys_vals)
        .expect("Failed to batch insert");

    // delete the odd-index keys
    let deletion_keys = (1..100).step_by(2);
    batch
        .delete_batch(&db, deletion_keys)
        .expect("Failed to batch delete");

    batch.write().expect("Failed to execute batch");

    for (k, _) in get_iter(&db) {
        assert_eq!(k % 2, 0);
    }
}

#[tokio::test]
async fn test_delete_range() {
    let options = ReadWriteOptions::default().set_ignore_range_deletions(false);
    let db: DBMap<i32, String> = DBMap::reopen(
        &open_rocksdb(temp_dir(), &[rocksdb::DEFAULT_COLUMN_FAMILY_NAME]),
        None,
        &options,
        false,
    )
    .unwrap();

    // Note that the last element is (100, "100".to_owned()) here
    let keys_vals = (0..101).map(|i| (i, i.to_string()));
    let mut batch = db.batch();
    batch
        .insert_batch(&db, keys_vals)
        .expect("Failed to batch insert");

    batch
        .schedule_delete_range(&db, &50, &100)
        .expect("Failed to delete range");

    batch.write().expect("Failed to execute batch");

    for k in 0..50 {
        assert!(db.contains_key(&k).expect("Failed to query legal key"),);
    }
    for k in 50..100 {
        assert!(!db.contains_key(&k).expect("Failed to query legal key"));
    }

    // range operator is not inclusive of to
    assert!(db.contains_key(&100).expect("Failed to query legal key"));
}

#[tokio::test]
async fn test_iter_with_bounds() {
    let db = open_map(temp_dir(), None);

    // Add [1, 50) and (50, 100) in the db
    for i in 1..100 {
        if i != 50 {
            db.insert(&i, &i.to_string()).unwrap();
        }
    }

    // Tests basic bounded scan.
    let db_iter = get_iter_with_bounds(&db, Some(20), Some(90));
    assert_eq!(
        (20..50)
            .chain(51..90)
            .map(|i| (i, i.to_string()))
            .collect::<Vec<_>>(),
        db_iter.collect::<Vec<_>>()
    );

    // Don't specify upper bound.
    let db_iter = get_iter_with_bounds(&db, Some(20), None);
    assert_eq!(
        (20..50)
            .chain(51..100)
            .map(|i| (i, i.to_string()))
            .collect::<Vec<_>>(),
        db_iter.collect::<Vec<_>>()
    );

    // Don't specify lower bound.
    let db_iter = get_iter_with_bounds(&db, None, Some(90));
    assert_eq!(
        (1..50)
            .chain(51..90)
            .map(|i| (i, i.to_string()))
            .collect::<Vec<_>>(),
        db_iter.collect::<Vec<_>>()
    );

    // Don't specify any bounds.
    let db_iter = get_iter_with_bounds(&db, None, None);
    assert_eq!(
        (1..50)
            .chain(51..100)
            .map(|i| (i, i.to_string()))
            .collect::<Vec<_>>(),
        db_iter.collect::<Vec<_>>()
    );

    // Specify a bound outside of dataset.
    let db_iter = db.safe_iter_with_bounds(Some(200), Some(300));
    assert!(db_iter.collect::<Vec<_>>().is_empty());

    // Skip to first key in the bound (bound is [1, 50))
    let db_iter = get_iter_with_bounds(&db, Some(1), Some(50));
    assert_eq!(
        (1..50).map(|i| (i, i.to_string())).collect::<Vec<_>>(),
        db_iter.collect::<Vec<_>>()
    );
}

#[rstest]
#[tokio::test]
async fn test_range_iter() {
    let db = open_map(temp_dir(), None);

    // Add [1, 50) and (50, 100) in the db
    for i in 1..100 {
        if i != 50 {
            db.insert(&i, &i.to_string()).unwrap();
        }
    }

    // Tests basic range iterating with inclusive end.
    let db_iter = get_range_iter(&db, 10..=20);
    assert_eq!(
        (10..21).map(|i| (i, i.to_string())).collect::<Vec<_>>(),
        db_iter.collect::<Vec<_>>()
    );

    // Tests range with min start and exclusive end.
    let db_iter = get_range_iter(&db, ..20);
    assert_eq!(
        (1..20).map(|i| (i, i.to_string())).collect::<Vec<_>>(),
        db_iter.collect::<Vec<_>>()
    );

    // Tests range with max end.
    let db_iter = get_range_iter(&db, 60..);
    assert_eq!(
        (60..100).map(|i| (i, i.to_string())).collect::<Vec<_>>(),
        db_iter.collect::<Vec<_>>()
    );

    // Skip to first key in the bound (bound is [1, 49))
    let db_iter = get_range_iter(&db, 1..49);
    assert_eq!(
        (1..49).map(|i| (i, i.to_string())).collect::<Vec<_>>(),
        db_iter.collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn test_is_empty() {
    let db: DBMap<i32, String> = open_map(temp_dir(), Some("table"));
    // Test empty map is truly empty
    assert!(db.is_empty());

    let keys_vals = (0..101).map(|i| (i, i.to_string()));
    let mut insert_batch = db.batch();
    insert_batch
        .insert_batch(&db, keys_vals)
        .expect("Failed to batch insert");

    insert_batch.write().expect("Failed to execute batch");

    // Check we have multiple entries and not empty
    assert!(db.safe_iter().count() > 1);
    assert!(!db.is_empty());
}

#[tokio::test]
async fn test_multi_insert() {
    // Init a DB
    let db: DBMap<i32, String> = open_map(temp_dir(), Some("table"));
    // Create kv pairs
    let keys_vals = (0..101).map(|i| (i, i.to_string()));

    db.multi_insert(keys_vals.clone())
        .expect("Failed to multi-insert");

    for (k, v) in keys_vals {
        let val = db.get(&k).expect("Failed to get inserted key");
        assert_eq!(Some(v), val);
    }
}

#[tokio::test]
async fn test_checkpoint() {
    let path_prefix = temp_dir();
    let db_path = path_prefix.join("db");
    let db: DBMap<i32, String> = open_map(db_path, Some("table"));
    // Create kv pairs
    let keys_vals = (0..101).map(|i| (i, i.to_string()));

    db.multi_insert(keys_vals.clone())
        .expect("Failed to multi-insert");
    let checkpointed_path = path_prefix.join("checkpointed_db");
    db.db
        .checkpoint(&checkpointed_path)
        .expect("Failed to create db checkpoint");
    // Create more kv pairs
    let new_keys_vals = (101..201).map(|i| (i, i.to_string()));
    db.multi_insert(new_keys_vals.clone())
        .expect("Failed to multi-insert");
    // Verify checkpoint
    let checkpointed_db: DBMap<i32, String> = open_map(checkpointed_path, Some("table"));
    // Ensure keys inserted before checkpoint are present in original and checkpointed db
    for (k, v) in keys_vals {
        let val = db.get(&k).expect("Failed to get inserted key");
        assert_eq!(Some(v.clone()), val);
        let val = checkpointed_db.get(&k).expect("Failed to get inserted key");
        assert_eq!(Some(v), val);
    }
    // Ensure keys inserted after checkpoint are only present in original db but not in checkpointed db
    for (k, v) in new_keys_vals {
        let val = db.get(&k).expect("Failed to get inserted key");
        assert_eq!(Some(v.clone()), val);
        let val = checkpointed_db.get(&k).expect("Failed to get inserted key");
        assert_eq!(None, val);
    }
}

#[tokio::test]
async fn test_multi_remove() {
    // Init a DB
    let db: DBMap<i32, String> = open_map(temp_dir(), Some("table"));

    // Create kv pairs
    let keys_vals = (0..101).map(|i| (i, i.to_string()));

    db.multi_insert(keys_vals.clone())
        .expect("Failed to multi-insert");

    // Check insertion
    for (k, v) in keys_vals.clone() {
        let val = db.get(&k).expect("Failed to get inserted key");
        assert_eq!(Some(v), val);
    }

    // Remove 50 items
    db.multi_remove(keys_vals.clone().map(|kv| kv.0).take(50))
        .expect("Failed to multi-remove");
    assert_eq!(db.safe_iter().count(), 101 - 50);

    // Check that the remaining are present
    for (k, v) in keys_vals.skip(50) {
        let val = db.get(&k).expect("Failed to get inserted key");
        assert_eq!(Some(v), val);
    }
}

#[tokio::test]
async fn open_as_secondary_test() {
    let primary_path = temp_dir();

    // Init a DB
    let primary_db: DBMap<i32, String> = open_map(primary_path.clone(), Some("table"));
    // Create kv pairs
    let keys_vals = (0..101).map(|i| (i, i.to_string()));

    primary_db
        .multi_insert(keys_vals.clone())
        .expect("Failed to multi-insert");

    let opt = rocksdb::Options::default();
    let secondary_store = open_cf_opts_secondary(
        primary_path.clone(),
        None,
        None,
        MetricConf::default(),
        &[("table", opt.clone())],
    )
    .unwrap();
    let secondary_db = DBMap::<i32, String>::reopen(
        &secondary_store,
        Some("table"),
        &ReadWriteOptions::default(),
        false,
    )
    .unwrap();

    secondary_db.try_catch_up_with_primary().unwrap();
    // Check secondary
    for (k, v) in keys_vals {
        assert_eq!(secondary_db.get(&k).unwrap(), Some(v));
    }

    // Update the value from 0 to 10
    primary_db.insert(&0, &"10".to_string()).unwrap();

    // This should still be stale since secondary is behind
    assert_eq!(secondary_db.get(&0).unwrap(), Some("0".to_string()));

    // Try force catchup
    secondary_db.try_catch_up_with_primary().unwrap();

    // New value should be present
    assert_eq!(secondary_db.get(&0).unwrap(), Some("10".to_string()));
}

fn open_map<P: AsRef<Path>, K, V>(path: P, opt_cf: Option<&str>) -> DBMap<K, V> {
    let cf_key = opt_cf.unwrap_or(rocksdb::DEFAULT_COLUMN_FAMILY_NAME);
    DBMap::<K, V>::reopen(
        &open_rocksdb(path, &[cf_key]),
        opt_cf,
        &ReadWriteOptions::default(),
        false,
    )
    .expect("failed to open rocksdb")
}

fn open_rocksdb<P: AsRef<Path>>(path: P, opt_cfs: &[&str]) -> Arc<Database> {
    let opts = rocksdb::Options::default();
    open_cf_opts(
        path,
        None,
        MetricConf::default(),
        &opt_cfs
            .iter()
            .map(|cf| (*cf, opts.clone()))
            .collect::<Vec<_>>(),
    )
    .expect("failed to open rocksdb")
}

/// Two types that have different BCS serialization formats
mod type_mismatch_types {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct OriginalValue {
        pub field_a: u64,
        pub field_b: String,
        pub field_c: Vec<u8>,
    }

    /// A different type with an incompatible layout - BCS is position-based,
    /// so even with the same field names, different types will fail to deserialize
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct IncompatibleValue {
        pub field_a: String, // Changed from u64 to String - will fail to deserialize
        pub field_b: u64,    // Changed from String to u64
    }
}

#[tokio::test]
async fn test_safe_iter_silently_ignores_deserialization_errors() {
    use type_mismatch_types::{IncompatibleValue, OriginalValue};

    let path = temp_dir();
    let cf_name = "test_cf";

    // Step 1: Create a database and insert values with OriginalValue type
    let rocks = open_rocksdb(path.clone(), &[cf_name]);

    let db_original: DBMap<i32, OriginalValue> =
        DBMap::reopen(&rocks, Some(cf_name), &ReadWriteOptions::default(), false)
            .expect("Failed to open storage");

    // Insert multiple values
    let original_values = vec![
        (
            1,
            OriginalValue {
                field_a: 100,
                field_b: "hello".to_string(),
                field_c: vec![1, 2, 3],
            },
        ),
        (
            2,
            OriginalValue {
                field_a: 200,
                field_b: "world".to_string(),
                field_c: vec![4, 5, 6],
            },
        ),
        (
            3,
            OriginalValue {
                field_a: 300,
                field_b: "test".to_string(),
                field_c: vec![7, 8, 9],
            },
        ),
    ];

    for (key, value) in &original_values {
        db_original
            .insert(key, value)
            .expect("Failed to insert value");
    }

    // Verify the values were inserted correctly with the original type
    let original_count = db_original.safe_iter().count();
    assert_eq!(
        original_count, 3,
        "Should have 3 values when reading with correct type"
    );

    // Step 2: Reopen the same column family with a different, incompatible value type
    let db_incompatible: DBMap<i32, IncompatibleValue> =
        DBMap::reopen(&rocks, Some(cf_name), &ReadWriteOptions::default(), false)
            .expect("Failed to reopen storage with different type");

    // Step 3: Iterate using safe_iter - this demonstrates the bug
    // The values should fail to deserialize, but currently safe_iter silently ignores them
    let results: Vec<_> = db_incompatible.safe_iter().collect();

    // BUG DEMONSTRATION:
    // - We inserted 3 values into the database
    // - When we iterate with an incompatible type, deserialization should fail
    // - Currently, safe_iter silently returns None for failed deserializations
    // - This means we get NO values and NO errors - the data appears to not exist

    // Count how many Ok results we got
    let ok_count = results.iter().filter(|r| r.is_ok()).count();
    // Count how many Err results we got
    let err_count = results.iter().filter(|r| r.is_err()).count();

    // This assertion documents the current buggy behavior:
    // We expect 0 values because they all fail to deserialize
    assert_eq!(
        ok_count, 0,
        "BUG: safe_iter returns no values when deserialization fails"
    );
    // We also expect 0 errors - the failures are silent!
    assert_eq!(
        err_count, 0,
        "BUG: safe_iter emits no errors when deserialization fails"
    );
    // The total number of items yielded by the iterator is 0
    assert_eq!(
        results.len(),
        0,
        "BUG: safe_iter yields nothing - failures are completely silent"
    );

    // Verify the data is still there by reading with the correct type
    let verification_count = db_original.safe_iter().count();
    assert_eq!(
        verification_count, 3,
        "Data should still exist in the database"
    );
}
