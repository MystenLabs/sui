// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_authenticated_events_client::types::{EventCommitment, derive_event_stream_head_object_id};
use sui_macros::sim_test;
use sui_sdk_types::Address;
use sui_sdk_types::Digest;
use sui_types::accumulator_root;
use sui_types::base_types::SuiAddress;

#[sim_test]
async fn test_derive_event_stream_head_object_id_compat() {
    let test_addresses = [
        SuiAddress::ZERO,
        SuiAddress::random_for_testing_only(),
        SuiAddress::random_for_testing_only(),
    ];

    for sui_addr in &test_addresses {
        let sdk_addr = Address::new(sui_addr.to_inner());

        let sui_types_id = accumulator_root::derive_event_stream_head_object_id(*sui_addr)
            .expect("sui-types derivation failed");

        let sdk_id = derive_event_stream_head_object_id(sdk_addr).expect("sdk derivation failed");

        assert_eq!(
            sui_types_id.to_vec(),
            sdk_id.as_bytes().to_vec(),
            "Object ID mismatch for address {:?}",
            sui_addr,
        );
    }
}

#[sim_test]
async fn test_event_commitment_bcs_compat() {
    let sui_types_commitment = accumulator_root::EventCommitment::new(
        42,
        7,
        3,
        sui_types::digests::Digest::new([0xab; 32]),
    );

    let sdk_commitment = EventCommitment::new(42, 7, 3, Digest::new([0xab; 32]));

    let sui_types_bcs =
        bcs::to_bytes(&sui_types_commitment).expect("sui-types BCS serialization failed");
    let sdk_bcs = bcs::to_bytes(&sdk_commitment).expect("sdk BCS serialization failed");

    assert_eq!(sui_types_bcs, sdk_bcs, "EventCommitment BCS mismatch");

    let roundtrip: EventCommitment =
        bcs::from_bytes(&sui_types_bcs).expect("Failed to deserialize sui-types BCS as sdk type");
    assert_eq!(roundtrip, sdk_commitment);
}

#[sim_test]
async fn test_event_stream_head_bcs_compat() {
    let sui_types_head = accumulator_root::EventStreamHead {
        mmr: vec![100u128.into(), 200u128.into()],
        checkpoint_seq: 999,
        num_events: 42,
    };

    let sui_types_bcs = bcs::to_bytes(&sui_types_head).expect("sui-types BCS serialization failed");

    let sdk_head: sui_authenticated_events_client::types::EventStreamHead =
        bcs::from_bytes(&sui_types_bcs).expect("Failed to deserialize sui-types BCS as sdk type");

    assert_eq!(sdk_head.checkpoint_seq, 999);
    assert_eq!(sdk_head.num_events, 42);
    assert_eq!(sdk_head.mmr.len(), 2);
}

#[sim_test]
async fn test_build_event_merkle_root_compat() {
    let commitments_sui = vec![
        accumulator_root::EventCommitment::new(1, 0, 0, sui_types::digests::Digest::new([1; 32])),
        accumulator_root::EventCommitment::new(1, 0, 1, sui_types::digests::Digest::new([2; 32])),
        accumulator_root::EventCommitment::new(2, 0, 0, sui_types::digests::Digest::new([3; 32])),
    ];

    let commitments_sdk = vec![
        EventCommitment::new(1, 0, 0, Digest::new([1; 32])),
        EventCommitment::new(1, 0, 1, Digest::new([2; 32])),
        EventCommitment::new(2, 0, 0, Digest::new([3; 32])),
    ];

    let sui_root = accumulator_root::build_event_merkle_root(&commitments_sui);
    let sdk_root =
        sui_authenticated_events_client::types::build_event_merkle_root(&commitments_sdk);

    assert_eq!(
        sui_root.inner().to_vec(),
        sdk_root.as_bytes().to_vec(),
        "Event merkle root mismatch"
    );
}
