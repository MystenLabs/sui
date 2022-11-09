// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use once_cell::sync::Lazy;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashSet;
use std::fmt::Debug;
use std::sync::Mutex;
use std::time::Duration;
use typed_store::metrics::SamplingInterval;
use typed_store::rocks::list_tables;
use typed_store::rocks::DBMap;
use typed_store::traits::Map;
use typed_store::traits::TypedStoreDebug;
use typed_store::Store;
use typed_store_derive::DBMapUtils;

fn temp_dir() -> std::path::PathBuf {
    tempfile::tempdir()
        .expect("Failed to open temporary directory")
        .into_path()
}
/// This struct is used to illustrate how the utility works
#[derive(DBMapUtils)]
struct Tables {
    table1: DBMap<String, String>,
    table2: DBMap<i32, String>,
}

// Check that generics work
#[derive(DBMapUtils)]
struct TablesGenerics<Q, W> {
    table1: DBMap<String, String>,
    table2: DBMap<u32, Generic<Q, W>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Generic<T, V> {
    field1: T,
    field2: V,
}

impl<
        T: Eq + Debug + Serialize + for<'de> Deserialize<'de>,
        V: Eq + Debug + Serialize + for<'de> Deserialize<'de>,
    > Generic<T, V>
{
}

/// This struct shows that single elem structs work
#[derive(DBMapUtils)]
struct TablesSingle {
    table1: DBMap<String, String>,
}

#[tokio::test]
async fn macro_test() {
    let primary_path = temp_dir();
    let tbls_primary = Tables::open_tables_read_write(primary_path.clone(), None, None);

    // Write to both tables
    let keys_vals_1 = (1..10).map(|i| (i.to_string(), i.to_string()));
    tbls_primary
        .table1
        .multi_insert(keys_vals_1.clone())
        .expect("Failed to multi-insert");

    let keys_vals_2 = (3..10).map(|i| (i, i.to_string()));
    tbls_primary
        .table2
        .multi_insert(keys_vals_2.clone())
        .expect("Failed to multi-insert");

    // Open in secondary mode
    let tbls_secondary = Tables::get_read_only_handle(primary_path.clone(), None, None);

    // Check all the tables can be listed
    let actual_table_names: HashSet<_> = list_tables(primary_path).unwrap().into_iter().collect();
    let observed_table_names: HashSet<_> = Tables::describe_tables()
        .iter()
        .map(|q| q.0.clone())
        .collect();

    let exp: HashSet<String> =
        HashSet::from_iter(vec!["table1", "table2"].into_iter().map(|s| s.to_owned()));
    assert_eq!(HashSet::from_iter(actual_table_names), exp);
    assert_eq!(HashSet::from_iter(observed_table_names), exp);

    // Check the counts
    assert_eq!(9, tbls_secondary.count_keys("table1").unwrap());
    assert_eq!(7, tbls_secondary.count_keys("table2").unwrap());

    // Test all entries
    let m = tbls_secondary.dump("table1", 100, 0).unwrap();
    for (k, v) in keys_vals_1 {
        assert_eq!(format!("\"{v}\""), *m.get(&format!("\"{k}\"")).unwrap());
    }

    let m = tbls_secondary.dump("table2", 100, 0).unwrap();
    for (k, v) in keys_vals_2 {
        assert_eq!(format!("\"{v}\""), *m.get(&k.to_string()).unwrap());
    }

    // Check that catchup logic works
    let keys_vals_1 = (100..110).map(|i| (i.to_string(), i.to_string()));
    tbls_primary
        .table1
        .multi_insert(keys_vals_1)
        .expect("Failed to multi-insert");
    // New entries should be present in secondary
    assert_eq!(19, tbls_secondary.count_keys("table1").unwrap());

    // Test pagination
    let m = tbls_secondary.dump("table1", 2, 0).unwrap();
    assert_eq!(2, m.len());
    assert_eq!(format!("\"1\""), *m.get(&"\"1\"".to_string()).unwrap());
    assert_eq!(format!("\"2\""), *m.get(&"\"2\"".to_string()).unwrap());

    let m = tbls_secondary.dump("table1", 3, 2).unwrap();
    assert_eq!(3, m.len());
    assert_eq!(format!("\"7\""), *m.get(&"\"7\"".to_string()).unwrap());
    assert_eq!(format!("\"8\""), *m.get(&"\"8\"".to_string()).unwrap());
}

/// We show that custom functions can be applied
#[derive(DBMapUtils)]
struct TablesCustomOptions {
    #[default_options_override_fn = "another_custom_fn_name"]
    table1: DBMap<String, String>,
    table2: DBMap<i32, String>,
    #[default_options_override_fn = "custom_fn_name"]
    table3: DBMap<i32, String>,
    #[default_options_override_fn = "another_custom_fn_name"]
    table4: DBMap<i32, String>,
}

static TABLE1_OPTIONS_SET_FLAG: Lazy<Mutex<Vec<bool>>> = Lazy::new(|| Mutex::new(vec![]));
static TABLE2_OPTIONS_SET_FLAG: Lazy<Mutex<Vec<bool>>> = Lazy::new(|| Mutex::new(vec![]));

fn custom_fn_name() -> typed_store::rocks::DBOptions {
    TABLE1_OPTIONS_SET_FLAG.lock().unwrap().push(false);
    typed_store::rocks::DBOptions::default()
}

fn another_custom_fn_name() -> typed_store::rocks::DBOptions {
    TABLE2_OPTIONS_SET_FLAG.lock().unwrap().push(false);
    TABLE2_OPTIONS_SET_FLAG.lock().unwrap().push(false);
    TABLE2_OPTIONS_SET_FLAG.lock().unwrap().push(false);
    typed_store::rocks::DBOptions::default()
}

#[tokio::test]
async fn macro_test_configure() {
    let primary_path = temp_dir();

    // Get a configurator for this table
    let mut config = Tables::configurator();
    // Config table 1
    config.table1 = typed_store::rocks::DBOptions::default();
    config.table1.options.create_if_missing(true);
    config.table1.options.set_write_buffer_size(123456);

    // Config table 2
    config.table2 = config.table1.clone();

    config.table2.options.create_if_missing(false);

    // Build and open with new config
    let _ = Tables::open_tables_read_write(primary_path, None, Some(config.build()));

    // Test the static config options
    let primary_path = temp_dir();

    assert_eq!(TABLE1_OPTIONS_SET_FLAG.lock().unwrap().len(), 0);

    let _ = TablesCustomOptions::open_tables_read_write(primary_path, None, None);

    // Ensures that the function to set options was called
    assert_eq!(TABLE1_OPTIONS_SET_FLAG.lock().unwrap().len(), 1);

    // `another_custom_fn_name` is called twice, so 6 items in vec
    assert_eq!(TABLE2_OPTIONS_SET_FLAG.lock().unwrap().len(), 6);
}

/// We show that custom functions can be applied
#[derive(DBMapUtils)]
struct TablesMemUsage {
    table1: DBMap<String, String>,
    table2: DBMap<i32, String>,
    table3: DBMap<i32, String>,
    table4: DBMap<i32, String>,
}

#[tokio::test]
async fn macro_test_get_memory_usage() {
    let primary_path = temp_dir();
    let tables = TablesMemUsage::open_tables_read_write(primary_path, None, None);

    let keys_vals_1 = (1..1000).map(|i| (i.to_string(), i.to_string()));
    tables
        .table1
        .multi_insert(keys_vals_1)
        .expect("Failed to multi-insert");

    let (mem_table, _) = tables.get_memory_usage().unwrap();
    assert!(mem_table > 0);
}

#[derive(DBMapUtils)]
struct StoreTables {
    table1: Store<Vec<u8>, Vec<u8>>,
    table2: Store<i32, String>,
}
#[tokio::test]
async fn store_iter_and_filter_successfully() {
    // Use constom configurator
    let mut config = StoreTables::configurator();
    // Config table 1
    config.table1 = typed_store::rocks::DBOptions::default();
    config.table1.options.create_if_missing(true);
    config.table1.options.set_write_buffer_size(123456);

    // Config table 2
    config.table2 = config.table1.clone();

    config.table2.options.create_if_missing(false);
    let path = temp_dir();
    let str = StoreTables::open_tables_read_write(path.clone(), None, Some(config.build()));

    // AND key-values to store.
    let key_values = vec![
        (vec![0u8, 1u8], vec![4u8, 4u8]),
        (vec![0u8, 2u8], vec![4u8, 5u8]),
        (vec![0u8, 3u8], vec![4u8, 6u8]),
        (vec![0u8, 4u8], vec![4u8, 7u8]),
        (vec![0u8, 5u8], vec![4u8, 0u8]),
        (vec![0u8, 6u8], vec![4u8, 1u8]),
    ];

    let result = str.table1.sync_write_all(key_values.clone()).await;
    assert!(result.is_ok());

    // Iter through the keys
    let output = str
        .table1
        .iter(Some(Box::new(|(k, _v)| {
            u16::from_le_bytes(k[..2].try_into().unwrap()) % 2 == 0
        })))
        .await;
    for (k, v) in &key_values {
        let int = u16::from_le_bytes(k[..2].try_into().unwrap());
        if int % 2 == 0 {
            let v1 = output.get(k).unwrap();
            assert_eq!(v1.first(), v.first());
            assert_eq!(v1.last(), v.last());
        } else {
            assert!(output.get(k).is_none());
        }
    }
    assert_eq!(output.len(), key_values.len());
}

#[tokio::test]
async fn test_sampling() {
    let sampling_interval = SamplingInterval::new(Duration::ZERO, 10);
    for _i in 0..10 {
        assert!(!sampling_interval.sample());
    }
    assert!(sampling_interval.sample());
    for _i in 0..10 {
        assert!(!sampling_interval.sample());
    }
    assert!(sampling_interval.sample());
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_sampling_time() {
    let sampling_interval = SamplingInterval::new(Duration::from_secs(1), 10);
    for _i in 0..10 {
        assert!(!sampling_interval.sample());
    }
    assert!(!sampling_interval.sample());
    tokio::time::advance(Duration::from_secs(1)).await;
    tokio::task::yield_now().await;
    assert!(sampling_interval.sample());
    for _i in 0..10 {
        assert!(!sampling_interval.sample());
    }
    assert!(!sampling_interval.sample());
    tokio::time::advance(Duration::from_secs(1)).await;
    tokio::task::yield_now().await;
    assert!(sampling_interval.sample());
}
