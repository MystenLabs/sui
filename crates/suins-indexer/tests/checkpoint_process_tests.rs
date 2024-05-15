// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_storage::blob::Blob;
use sui_types::full_checkpoint_content::CheckpointData;
use suins_indexer::indexer::SuinsIndexer;

/// Test ids.
const TEST_REGISTRY_TABLE_ID: &str =
    "0xb120c0d55432630fce61f7854795a3463deb6e3b443cc4ae72e1282073ff56e4";
const TEST_NAME_RECORD_TYPE: &str = "0x2::dynamic_field::Field<0x22fa05f21b1ad71442491220bb9338f7b7095fe35000ef88d5400d28523bdd93::domain::Domain,0x22fa05f21b1ad71442491220bb9338f7b7095fe35000ef88d5400d28523bdd93::name_record::NameRecord>";
const TEST_SUBDOMAIN_REGISTRATION_TYPE: &str = "0x22fa05f21b1ad71442491220bb9338f7b7095fe35000ef88d5400d28523bdd93::subdomain_registration::SubDomainRegistration";

/// For our test policy, we have a few checkpoints that contain some data additions, deletions, replacements
///
/// Checkpoint 22279187: Adds 3 different names (1 SLD, 1 leaf, 1 node). Deletes none.
/// Checkpoint 22279365: Removes 1 leaf name. Adds 1 leaf name.
/// Checkpoint 22279496: Replaces the name added on `22279365` (new.test.sui) by removing it and then adding it as a node name.
/// Checkpoint 22279944: Adds `remove.test.sui`.
/// Checkpoint 22280030: Adds `remove.test.sui` as a replacement (the previous one expired!).
///                      [This was only simulated using a dummy contract and cannot happen in realistic scenarios.]
///
#[test]
fn process_22279187_checkpoint() {
    let checkpoint = read_checkpoint_from_file(include_bytes!("data/22279187.chk"));
    let indexer = get_test_indexer();

    let (updates, removals) = indexer.process_checkpoint(&checkpoint);

    // This checkpoint has no removals and adds 3 names.
    assert_eq!(removals.len(), 0);
    assert_eq!(updates.len(), 3);

    let names: Vec<_> = updates.iter().map(|n| n.name.clone()).collect();

    assert!(names.contains(&"leaf.test.sui".to_string()));
    assert!(names.contains(&"node.test.sui".to_string()));
    assert!(names.contains(&"test.sui".to_string()));
}

#[test]
fn process_22279365_checkpoint() {
    let checkpoint = read_checkpoint_from_file(include_bytes!("data/22279365.chk"));
    let indexer = get_test_indexer();
    let (updates, removals) = indexer.process_checkpoint(&checkpoint);

    // This checkpoint has 1 removal and 1 addition.
    assert_eq!(removals.len(), 1);
    assert_eq!(updates.len(), 1);

    let addition = updates.first().unwrap();
    assert_eq!(addition.name, "new.test.sui".to_string());
    assert_eq!(addition.parent, "test.sui".to_string());
    assert_eq!(
        addition.nft_id,
        "0xa4891f3754b203ef230a5e2a08822c835c808eab71e2bc6ca33a73cec9728376".to_string()
    );
    assert_eq!(addition.expiration_timestamp_ms, 0);
    assert_eq!(addition.subdomain_wrapper_id, None);
}

#[test]
fn process_22279496_checkpoint() {
    let checkpoint = read_checkpoint_from_file(include_bytes!("data/22279496.chk"));
    let indexer = get_test_indexer();
    let (updates, removals) = indexer.process_checkpoint(&checkpoint);

    assert_eq!(removals.len(), 0);
    assert_eq!(updates.len(), 1);

    let addition = updates.first().unwrap();
    assert_eq!(addition.name, "new.test.sui".to_string());
    assert_eq!(addition.parent, "test.sui".to_string());
    assert_eq!(
        addition.nft_id,
        "0x87f04a4ffa1713e0a7e3a9e5ebf56f0ab24ce0bba87b17eb11a7532cb381bd58".to_string()
    );
    assert_eq!(addition.expiration_timestamp_ms, 1706213544456);
    assert_eq!(
        addition.subdomain_wrapper_id,
        Some("0xa308d8b800a2b65f4b2282bd0bbf11edf2435705905119c45257c21914bff032".to_string())
    );
}

#[test]
fn process_22279944_checkpoint() {
    let checkpoint = read_checkpoint_from_file(include_bytes!("data/22279944.chk"));
    let indexer = get_test_indexer();
    let (updates, removals) = indexer.process_checkpoint(&checkpoint);

    assert_eq!(removals.len(), 0);
    assert_eq!(updates.len(), 1);

    let addition = updates.first().unwrap();
    assert_eq!(addition.name, "remove.test.sui".to_string());
    assert_eq!(
        addition.nft_id,
        "0x7c230e1a4cd7b708232a713a138f4c950e7f579b61d01b988f06d7dc53e99211".to_string()
    );
    assert_eq!(
        addition.subdomain_wrapper_id,
        Some("0x9ca93181d093598b55787e82f69296819e9f779f25f1cc5226d2cd4d07126790".to_string())
    );
    assert_eq!(
        addition.field_id,
        "0x79b123c73d073ba73c9e6f0817e63270d716db3c7945ecde477b22df7d026e43".to_string()
    )
}

#[test]
fn process_22280030_checkpoint() {
    let checkpoint = read_checkpoint_from_file(include_bytes!("data/22280030.chk"));
    let indexer = get_test_indexer();
    let (updates, removals) = indexer.process_checkpoint(&checkpoint);

    assert_eq!(removals.len(), 0);
    assert_eq!(updates.len(), 1);

    let addition = updates.first().unwrap();
    assert_eq!(addition.name, "remove.test.sui".to_string());
    assert_eq!(
        addition.nft_id,
        "0xdd513860269b0768c6ed77ddaf48cd579ba0c2995e793eab182d6ab861818250".to_string()
    );
    assert_eq!(
        addition.subdomain_wrapper_id,
        Some("0x48de1a7eef5956c4f3478849654abd94dcf5b206c631328c50518091b0eee9b0".to_string())
    );

    assert_eq!(
        addition.field_id,
        "0x79b123c73d073ba73c9e6f0817e63270d716db3c7945ecde477b22df7d026e43".to_string()
    )
}

/// Reads a checkpoint from a given file in the `/tests/data` directory.
fn read_checkpoint_from_file(file: &[u8]) -> CheckpointData {
    Blob::from_bytes::<CheckpointData>(file).unwrap()
}

/// Return a suins_indexer instance for testing.
/// Uses the testnet demo we used to extract the checkpoints being processed.
fn get_test_indexer() -> SuinsIndexer {
    SuinsIndexer::new(
        TEST_REGISTRY_TABLE_ID.to_string(),
        TEST_SUBDOMAIN_REGISTRATION_TYPE.to_string(),
        TEST_NAME_RECORD_TYPE.to_string(),
    )
}
