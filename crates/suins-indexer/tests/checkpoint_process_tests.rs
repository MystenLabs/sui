// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{env, fs};

use sui_storage::blob::Blob;
use sui_types::full_checkpoint_content::CheckpointData;
use suins_indexer::indexer::SuinsIndexer;

/// Test ids.
const TEST_REGISTRY_TABLE_ID: &str =
    "0xb120c0d55432630fce61f7854795a3463deb6e3b443cc4ae72e1282073ff56e4";
const TEST_SUBDOMAIN_REGISTRATION_TYPE: &str = "0x22fa05f21b1ad71442491220bb9338f7b7095fe35000ef88d5400d28523bdd93::subdomain_registration::SubDomainRegistration";

/// For our test policy, we have a few checkpoints that contain some data additions, deletions, replacements
///
/// Checkpoint 22279187: Adds 3 different names. Deletes none.
/// Checkpoint 22279365: Removes 1 name. Adds 1 name
///
/// TODO: Finish the tests.
#[test]
fn process_initial_checkpoint() {
    let checkpoint = read_checkpoint_from_file("22279187");
    let indexer = get_test_indexer();

    let (updates, removals) = indexer.process_checkpoint(checkpoint.clone());

    // This checkpoint has no removals and adds 3 names.
    assert_eq!(removals.len(), 0);
    assert_eq!(updates.len(), 3);

    let names: Vec<_> = updates.iter().map(|n| n.name.clone()).collect();

    assert!(names.contains(&"leaf.test.sui".to_string()));
    assert!(names.contains(&"node.test.sui".to_string()));
    assert!(names.contains(&"test.sui".to_string()));
}

/// Reads a checkpoint from a given file in the `/tests/data` directory.
fn read_checkpoint_from_file(file_name: &str) -> CheckpointData {
    let file = fs::read(format!(
        "{}/tests/data/{}.chk",
        get_current_working_dir(),
        file_name
    ))
    .unwrap();

    Blob::from_bytes::<CheckpointData>(&file).unwrap()
}

fn get_current_working_dir() -> String {
    env::current_dir().unwrap().to_str().unwrap().to_string()
}

/// Return a suins_indexer instance for testing.
/// Uses the testnet demo we used to extract the checkpoints being processed.
fn get_test_indexer() -> SuinsIndexer {
    SuinsIndexer::new(
        TEST_REGISTRY_TABLE_ID.to_string(),
        TEST_SUBDOMAIN_REGISTRATION_TYPE.to_string(),
    )
}
