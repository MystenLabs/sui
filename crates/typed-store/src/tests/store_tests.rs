// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;

fn temp_dir() -> std::path::PathBuf {
    tempfile::tempdir()
        .expect("Failed to open temporary directory")
        .into_path()
}

#[tokio::test]
async fn create_store() {
    // Create new store.
    let db = rocks::DBMap::<usize, String>::open(
        temp_dir(),
        None,
        None,
        &rocks::ReadWriteOptions::default(),
    )
    .unwrap();
    let _ = Store::<usize, String>::new(db);
}

#[tokio::test]
async fn read_async_write_value() {
    // Create new store.
    let db = rocks::DBMap::<Vec<u8>, Vec<u8>>::open(
        temp_dir(),
        None,
        None,
        &rocks::ReadWriteOptions::default(),
    )
    .unwrap();
    let store = Store::new(db);

    // Write value to the store.
    let key = vec![0u8, 1u8, 2u8, 3u8];
    let value = vec![4u8, 5u8, 6u8, 7u8];
    store.async_write(key.clone(), value.clone()).await;

    // Read value.
    let result = store.read(key).await;
    assert!(result.is_ok());
    let read_value = result.unwrap();
    assert!(read_value.is_some());
    assert_eq!(read_value.unwrap(), value);
}

#[tokio::test]
async fn read_sync_write_value() {
    // Create new store.
    let db = rocks::DBMap::<Vec<u8>, Vec<u8>>::open(
        temp_dir(),
        None,
        None,
        &rocks::ReadWriteOptions::default(),
    )
    .unwrap();
    let store = Store::new(db);

    // Write value to the store.
    let key = vec![0u8, 1u8, 2u8, 3u8];
    let value = vec![4u8, 5u8, 6u8, 7u8];
    store.sync_write(key.clone(), value.clone()).await.unwrap();

    // Read value.
    let result = store.read(key).await;
    assert!(result.is_ok());
    let read_value = result.unwrap();
    assert!(read_value.is_some());
    assert_eq!(read_value.unwrap(), value);
}

#[tokio::test]
async fn read_raw_write_value() {
    // Create new store.
    let db = rocks::DBMap::<Vec<u8>, String>::open(
        temp_dir(),
        None,
        None,
        &rocks::ReadWriteOptions::default(),
    )
    .unwrap();
    let store = Store::new(db);

    // Write value to the store.
    let key = vec![0u8, 1u8, 2u8, 3u8];
    let value = "123456".to_string();
    store.async_write(key.clone(), value.clone()).await;

    // Read value.
    let result = store.read_raw_bytes(key).await;
    assert!(result.is_ok());
    let read_value = result.unwrap();
    assert!(read_value.is_some());
    assert_eq!(read_value, Some(bincode::serialize(&value).unwrap()));
}

#[tokio::test]
async fn read_unknown_key() {
    // Create new store.
    let db = rocks::DBMap::<Vec<u8>, Vec<u8>>::open(
        temp_dir(),
        None,
        None,
        &rocks::ReadWriteOptions::default(),
    )
    .unwrap();
    let store = Store::new(db);

    // Try to read unknown key.
    let key = vec![0u8, 1u8, 2u8, 3u8];
    let result = store.read(key).await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[tokio::test]
async fn read_notify() {
    // Create new store.
    let db = rocks::DBMap::<Vec<u8>, Vec<u8>>::open(
        temp_dir(),
        None,
        None,
        &rocks::ReadWriteOptions::default(),
    )
    .unwrap();
    let store = Store::new(db);

    // Try to read a kew that does not yet exist. Then write a value
    // for that key and check that notify read returns the result.
    let key = vec![0u8, 1u8, 2u8, 3u8];
    let value = vec![4u8, 5u8, 6u8, 7u8];

    // Try to read a missing value.
    let store_copy = store.clone();
    let key_copy = key.clone();
    let value_copy = value.clone();
    let handle = tokio::spawn(async move {
        match store_copy.notify_read(key_copy).await {
            Ok(Some(v)) => assert_eq!(v, value_copy),
            _ => panic!("Failed to read from store"),
        }
    });

    // Write the missing value and ensure the handle terminates correctly.
    store.async_write(key, value).await;
    assert!(handle.await.is_ok());
}

#[tokio::test]
async fn remove_all_successfully() {
    // GIVEN Create new store.
    let db = rocks::DBMap::<Vec<u8>, Vec<u8>>::open(
        temp_dir(),
        None,
        None,
        &rocks::ReadWriteOptions::default(),
    )
    .unwrap();
    let store = Store::new(db);

    // AND Write values to the store.
    let keys = vec![
        vec![0u8, 1u8, 2u8, 1u8],
        vec![0u8, 1u8, 2u8, 2u8],
        vec![0u8, 1u8, 2u8, 3u8],
    ];
    let value = vec![4u8, 5u8, 6u8, 7u8];

    for key in keys.clone() {
        store.async_write(key.clone(), value.clone()).await;
    }

    // WHEN multi remove values
    let result = store.remove_all(keys.clone().into_iter()).await;

    // THEN
    assert!(result.is_ok());

    // AND values doesn't exist any more
    for key in keys {
        let result = store.read(key).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }
}

#[tokio::test]
async fn write_and_read_all_successfully() {
    // GIVEN Create new store.
    let db = rocks::DBMap::<Vec<u8>, Vec<u8>>::open(
        temp_dir(),
        None,
        None,
        &rocks::ReadWriteOptions::default(),
    )
    .unwrap();
    let store = Store::new(db);

    // AND key-values to store.
    let key_values = vec![
        (vec![0u8, 1u8, 2u8, 1u8], vec![4u8, 5u8, 6u8, 7u8]),
        (vec![0u8, 1u8, 2u8, 2u8], vec![4u8, 5u8, 6u8, 7u8]),
        (vec![0u8, 1u8, 2u8, 3u8], vec![4u8, 5u8, 6u8, 7u8]),
    ];

    // WHEN
    let result = store.sync_write_all(key_values.clone()).await;

    // THEN
    assert!(result.is_ok());

    // AND read_all to ensure that values have been written
    let keys: Vec<Vec<u8>> = key_values.clone().into_iter().map(|(key, _)| key).collect();
    let result = store.read_all(keys).await;

    assert!(result.is_ok());
    assert_eq!(result.as_ref().unwrap().len(), 3);

    for (i, value) in result.unwrap().into_iter().enumerate() {
        assert!(value.is_some());
        assert_eq!(value.unwrap(), key_values[i].1);
    }
}

#[tokio::test]
async fn iter_successfully() {
    // GIVEN Create new store.
    let db = rocks::DBMap::<Vec<u8>, Vec<u8>>::open(
        temp_dir(),
        None,
        None,
        &rocks::ReadWriteOptions::default(),
    )
    .unwrap();
    let store = Store::new(db);

    // AND key-values to store.
    let key_values = vec![
        (vec![0u8, 1u8], vec![4u8, 4u8]),
        (vec![0u8, 2u8], vec![4u8, 5u8]),
        (vec![0u8, 3u8], vec![4u8, 6u8]),
    ];

    let result = store.sync_write_all(key_values.clone()).await;
    assert!(result.is_ok());

    // Iter through the keys
    let output = store.iter(None).await;
    for (k, v) in &key_values {
        let v1 = output.get(k).unwrap();
        assert_eq!(v1.first(), v.first());
        assert_eq!(v1.last(), v.last());
    }
    assert_eq!(output.len(), key_values.len());
}

#[tokio::test]
async fn iter_and_filter_successfully() {
    // GIVEN Create new store.
    let db = rocks::DBMap::<Vec<u8>, Vec<u8>>::open(
        temp_dir(),
        None,
        None,
        &rocks::ReadWriteOptions::default(),
    )
    .unwrap();
    let store = Store::new(db);

    // AND key-values to store.
    let key_values = vec![
        (vec![0u8, 1u8], vec![4u8, 4u8]),
        (vec![0u8, 2u8], vec![4u8, 5u8]),
        (vec![0u8, 3u8], vec![4u8, 6u8]),
        (vec![0u8, 4u8], vec![4u8, 7u8]),
        (vec![0u8, 5u8], vec![4u8, 0u8]),
        (vec![0u8, 6u8], vec![4u8, 1u8]),
    ];

    let result = store.sync_write_all(key_values.clone()).await;
    assert!(result.is_ok());

    // Iter through the keys
    let output = store
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
