// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::Deserialize;
use serde::Serialize;
use std::collections::HashSet;
use std::fmt::Debug;
use typed_store::rocks::DBMap;
use typed_store::traits::DBMapTableUtil;
use typed_store::traits::Map;
use typed_store_macros::DBMapUtils;

fn temp_dir() -> std::path::PathBuf {
    tempfile::tempdir()
        .expect("Failed to open temporary directory")
        .into_path()
}
/// This struct is used to illustrate how the utility works
#[derive(DBMapUtils)]
struct Tables {
    /// A comment
    #[options(optimization = "point_lookup", cache_capacity = 100000)]
    table1: DBMap<String, String>,
    #[options(optimization = "point_lookup")]
    table2: DBMap<i32, String>,
    /// A comment
    table3: DBMap<i32, String>,
    #[options()]
    table4: DBMap<i32, String>,
}

/// The existence of this struct is to prove that multiple structs can be defined in same file with no issues
#[derive(DBMapUtils)]
struct Tables2 {
    #[options(optimization = "point_lookup", cache_capacity = 100000)]
    table1: DBMap<String, String>,
    #[options(optimization = "point_lookup")]
    table2: DBMap<i32, String>,
    table3: DBMap<i32, String>,
    #[options()]
    table4: DBMap<i32, String>,
}

// Check that generics work
#[derive(DBMapUtils)]
struct Tables3<Q, W> {
    table1: DBMap<String, String>,
    table2: DBMap<u32, Generic<Q, W>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Generic<T, V> {
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
    #[options(optimization = "point_lookup", cache_capacity = 100000)]
    table1: DBMap<String, String>,
}

#[tokio::test]
async fn macro_test() {
    let primary_path = temp_dir();
    let tbls_primary = Tables::open_tables_read_write(primary_path.clone(), None);

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
    let tbls_secondary = Tables::open_tables_read_only(primary_path.clone(), None, None);

    // Check all the tables can be listed
    let table_names = Tables::list_tables(primary_path).unwrap();
    let exp: HashSet<String> = HashSet::from_iter(
        vec!["table1", "table2", "table3", "table4"]
            .into_iter()
            .map(|s| s.to_owned()),
    );
    assert_eq!(HashSet::from_iter(table_names), exp);

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
