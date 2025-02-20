// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use once_cell::sync::Lazy;
use serde::Deserialize;
use serde::Serialize;
use std::borrow::Borrow;
use std::collections::HashSet;
use std::fmt::Debug;
use std::sync::Mutex;
use std::time::Duration;
use typed_store::metrics::SamplingInterval;
use typed_store::rocks::list_tables;
use typed_store::rocks::DBMap;
use typed_store::rocks::{be_fix_int_ser, MetricConf};
use typed_store::traits::Map;
use typed_store::traits::TableSummary;
use typed_store::traits::TypedStoreDebug;
use typed_store::DBMapUtils;

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

#[derive(DBMapUtils)]
struct RenameTables1 {
    table: DBMap<String, String>,
}

#[derive(DBMapUtils)]
struct RenameTables2 {
    #[rename = "table"]
    renamed_table: DBMap<String, String>,
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
    let tbls_primary =
        Tables::open_tables_read_write(primary_path.clone(), MetricConf::default(), None, None);

    // Write to both tables
    let mut raw_key_bytes1 = 0;
    let mut raw_value_bytes1 = 0;
    let kv_range = 1..10;
    for i in kv_range.clone() {
        let key = i.to_string();
        let value = i.to_string();
        let k_buf = be_fix_int_ser::<String>(&key).unwrap();
        let value_buf = bcs::to_bytes::<String>(&value).unwrap();
        raw_key_bytes1 += k_buf.len();
        raw_value_bytes1 += value_buf.len();
    }
    let keys_vals_1 = kv_range.map(|i| (i.to_string(), i.to_string()));
    tbls_primary
        .table1
        .multi_insert(keys_vals_1.clone())
        .expect("Failed to multi-insert");

    let mut raw_key_bytes2 = 0;
    let mut raw_value_bytes2 = 0;
    let kv_range = 3..10;
    for i in kv_range.clone() {
        let key = i;
        let value = i.to_string();
        let k_buf = be_fix_int_ser(key.borrow()).unwrap();
        let value_buf = bcs::to_bytes::<String>(&value).unwrap();
        raw_key_bytes2 += k_buf.len();
        raw_value_bytes2 += value_buf.len();
    }
    let keys_vals_2 = kv_range.map(|i| (i, i.to_string()));
    tbls_primary
        .table2
        .multi_insert(keys_vals_2.clone())
        .expect("Failed to multi-insert");

    // Open in secondary mode
    let tbls_secondary =
        Tables::get_read_only_handle(primary_path.clone(), None, None, MetricConf::default());

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

    // check raw byte sizes of key and values
    let summary1 = tbls_secondary.table_summary("table1").unwrap();
    assert_eq!(9, summary1.num_keys);
    assert_eq!(raw_key_bytes1, summary1.key_bytes_total);
    assert_eq!(raw_value_bytes1, summary1.value_bytes_total);
    let summary2 = tbls_secondary.table_summary("table2").unwrap();
    assert_eq!(7, summary2.num_keys);
    assert_eq!(raw_key_bytes2, summary2.key_bytes_total);
    assert_eq!(raw_value_bytes2, summary2.value_bytes_total);

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
    assert_eq!(format!("\"1\""), *m.get("\"1\"").unwrap());
    assert_eq!(format!("\"2\""), *m.get("\"2\"").unwrap());

    let m = tbls_secondary.dump("table1", 3, 2).unwrap();
    assert_eq!(3, m.len());
    assert_eq!(format!("\"7\""), *m.get("\"7\"").unwrap());
    assert_eq!(format!("\"8\""), *m.get("\"8\"").unwrap());
}

#[tokio::test]
async fn rename_test() {
    let dbdir = temp_dir();

    let key = "key".to_string();
    let value = "value".to_string();
    {
        let original_db =
            RenameTables1::open_tables_read_write(dbdir.clone(), MetricConf::default(), None, None);
        original_db.table.insert(&key, &value).unwrap();
    }

    // sleep for 1 second
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;

    {
        let renamed_db =
            RenameTables2::open_tables_read_write(dbdir.clone(), MetricConf::default(), None, None);
        assert_eq!(renamed_db.renamed_table.get(&key), Ok(Some(value)));
    }
}

#[derive(DBMapUtils)]
struct DeprecatedTables {
    table1: DBMap<String, String>,
    #[deprecated]
    table2: DBMap<i32, String>,
}

#[tokio::test]
async fn deprecate_test() {
    let dbdir = temp_dir();
    let key = "key".to_string();
    let value = "value".to_string();
    {
        let original_db =
            Tables::open_tables_read_write(dbdir.clone(), MetricConf::default(), None, None);
        original_db.table1.insert(&key, &value).unwrap();
        original_db.table2.insert(&0, &value).unwrap();
    }
    for _ in 0..2 {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        let db = DeprecatedTables::open_tables_read_write_with_deprecation_option(
            dbdir.clone(),
            MetricConf::default(),
            None,
            None,
            true,
        );
        assert_eq!(db.table1.get(&key), Ok(Some(value.clone())));
    }
}

#[tokio::test]
async fn macro_transactional_test() {
    let key = "key".to_string();
    let primary_path = temp_dir();
    let tables = Tables::open_tables_transactional(primary_path, MetricConf::default(), None, None);
    let mut transaction = tables
        .table1
        .transaction()
        .expect("failed to init transaction");
    transaction
        .insert_batch(&tables.table1, vec![(key.to_string(), "1".to_string())])
        .unwrap();
    transaction
        .commit()
        .expect("failed to commit first transaction");
    assert_eq!(tables.table1.get(&key), Ok(Some("1".to_string())));
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
    let _ = Tables::open_tables_read_write(
        primary_path,
        MetricConf::default(),
        None,
        Some(config.build()),
    );

    // Test the static config options
    let primary_path = temp_dir();

    assert_eq!(TABLE1_OPTIONS_SET_FLAG.lock().unwrap().len(), 0);

    let _ = TablesCustomOptions::open_tables_read_write(
        primary_path,
        MetricConf::default(),
        None,
        None,
    );

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
