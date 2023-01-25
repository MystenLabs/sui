// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::epoch::data_removal;
use std::fs;
use std::path::PathBuf;

#[tokio::test]
async fn test_remove_old_epoch_data() {
    // Create the storage paths
    let base_path_string = narwhal_test_utils::temp_dir().to_str().unwrap().to_owned();

    let mut base_path = PathBuf::new();
    base_path.push(base_path_string.clone());

    let mut path_other = PathBuf::new();
    path_other.push(base_path_string.clone() + "/other");
    let mut path_98 = base_path.clone();
    path_98.push(base_path_string.clone() + "/98");
    let mut path_99 = base_path.clone();
    path_99.push(base_path_string.clone() + "/99");
    let mut path_100 = base_path.clone();
    path_100.push(base_path_string.clone() + "/100");

    // Remove the directories created next in case it wasn't cleaned up before the last test run terminated
    _ = fs::remove_dir_all(base_path.clone());

    // Create some epoch directories
    fs::create_dir(base_path.clone()).unwrap();
    fs::create_dir(path_other.clone()).unwrap();
    fs::create_dir(path_98.clone()).unwrap();
    fs::create_dir(path_99.clone()).unwrap();
    fs::create_dir(path_100.clone()).unwrap();

    // With the current epoch of 100, remove old epochs
    data_removal::remove_old_epoch_data(base_path.clone(), 100);

    // Now ensure the epoch directories older than 100 were removed
    let files = fs::read_dir(base_path_string).unwrap();

    let mut epochs_left = Vec::new();
    for file_res in files {
        let file_epoch_string = file_res.unwrap().file_name().to_str().unwrap().to_owned();
        if let Ok(file_epoch) = file_epoch_string.parse::<u64>() {
            epochs_left.push(file_epoch);
        }
    }

    // Remove the directories we created before the test possibly terminates
    _ = fs::remove_dir_all(base_path);

    assert_eq!(epochs_left.len(), 1);
    assert_eq!(epochs_left[0], 100);
}
