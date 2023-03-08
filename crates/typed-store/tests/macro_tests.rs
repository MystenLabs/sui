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
use typed_store::rocks::be_fix_int_ser;
use typed_store::rocks::list_tables;
use typed_store::rocks::DBMap;
use typed_store::rocks::RocksDBAccessType;
use typed_store::sally::SallyColumn;
use typed_store::sally::SallyDBOptions;
use typed_store::sally::SallyReadOnlyDBOptions;
use typed_store::traits::Map;
use typed_store::traits::TableSummary;
use typed_store::traits::TypedStoreDebug;
use typed_store_derive::DBMapUtils;
use typed_store_derive::SallyDB;

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
    assert_eq!(format!("\"1\""), *m.get(&"\"1\"".to_string()).unwrap());
    assert_eq!(format!("\"2\""), *m.get(&"\"2\"".to_string()).unwrap());

    let m = tbls_secondary.dump("table1", 3, 2).unwrap();
    assert_eq!(3, m.len());
    assert_eq!(format!("\"7\""), *m.get(&"\"7\"".to_string()).unwrap());
    assert_eq!(format!("\"8\""), *m.get(&"\"8\"".to_string()).unwrap());
}

#[derive(SallyDB)]
pub struct SallyDBExample {
    col1: SallyColumn<String, String>,
    col2: SallyColumn<i32, String>,
}

#[tokio::test]
async fn test_sallydb() {
    let primary_path = temp_dir();
    let example_db = SallyDBExample::init(SallyDBOptions::RocksDB((
        primary_path.clone(),
        RocksDBAccessType::Primary,
        None,
        None,
    )));

    // Write to both columns
    let keys_vals_1 = (1..10).map(|i| (i.to_string(), i.to_string()));
    let mut wb = example_db.col1.batch();
    wb.insert_batch(&example_db.col1, keys_vals_1.clone())
        .expect("Failed to insert");

    let keys_vals_2 = (3..10).map(|i| (i, i.to_string()));
    wb.insert_batch(&example_db.col2, keys_vals_2.clone())
        .expect("Failed to insert");

    wb.write().await.expect("Failed to commit write batch");

    // Open in secondary mode
    let example_db_secondary = SallyDBExample::get_read_only_handle(
        SallyReadOnlyDBOptions::RocksDB(Box::new((primary_path.clone(), None, None))),
    );

    // Check all the tables can be listed
    let actual_table_names: HashSet<_> = list_tables(primary_path).unwrap().into_iter().collect();
    let observed_table_names: HashSet<_> = SallyDBExample::describe_tables()
        .iter()
        .map(|q| q.0.clone())
        .collect();

    let exp: HashSet<String> =
        HashSet::from_iter(vec!["col1", "col2"].into_iter().map(|s| s.to_owned()));
    assert_eq!(HashSet::from_iter(actual_table_names), exp);
    assert_eq!(HashSet::from_iter(observed_table_names), exp);

    // Check the counts
    assert_eq!(9, example_db_secondary.count_keys("col1").unwrap());
    assert_eq!(7, example_db_secondary.count_keys("col2").unwrap());

    // Test all entries
    let m = example_db_secondary.dump("col1", 100, 0).unwrap();
    for (k, v) in keys_vals_1 {
        assert_eq!(format!("\"{v}\""), *m.get(&format!("\"{k}\"")).unwrap());
    }

    let m = example_db_secondary.dump("col2", 100, 0).unwrap();
    for (k, v) in keys_vals_2 {
        assert_eq!(format!("\"{v}\""), *m.get(&k.to_string()).unwrap());
    }

    // Check that catchup logic works
    let keys_vals_1 = (100..110).map(|i| (i.to_string(), i.to_string()));
    let mut wb = example_db.col1.batch();
    wb.insert_batch(&example_db.col1, keys_vals_1.clone())
        .expect("Failed to insert");
    wb.write().await.expect("Failed to commit write batch");

    // New entries should be present in secondary
    assert_eq!(19, example_db_secondary.count_keys("col1").unwrap());

    // Test pagination
    let m = example_db_secondary.dump("col1", 2, 0).unwrap();
    assert_eq!(2, m.len());
    assert_eq!(format!("\"1\""), *m.get(&"\"1\"".to_string()).unwrap());
    assert_eq!(format!("\"2\""), *m.get(&"\"2\"".to_string()).unwrap());

    let m = example_db_secondary.dump("col1", 3, 2).unwrap();
    assert_eq!(3, m.len());
    assert_eq!(format!("\"7\""), *m.get(&"\"7\"".to_string()).unwrap());
    assert_eq!(format!("\"8\""), *m.get(&"\"8\"".to_string()).unwrap());
}

#[tokio::test]
async fn macro_transactional_test() {
    let key = "key".to_string();
    let primary_path = temp_dir();
    let tables = Tables::open_tables_transactional(primary_path, None, None);
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
