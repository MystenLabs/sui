// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::hash::{Blake2b256, HashFunction};
use move_core_types::u256::U256;
use sui_types::accumulator_root::{EventCommitment, EventStreamHead, build_event_merkle_root};

const U256_ZERO: U256 = U256::zero();

fn hash_two_to_one_u256(left: U256, right: U256) -> U256 {
    let mut concatenated = bcs::to_bytes(&left).expect("Failed to serialize left U256");
    concatenated.extend_from_slice(&bcs::to_bytes(&right).expect("Failed to serialize right U256"));
    let hash = Blake2b256::digest(&concatenated);
    U256::from_le_bytes(&hash.digest)
}

fn add_to_stream(mmr: &mut Vec<U256>, new_val: U256) {
    let mut i = 0;
    let mut cur = new_val;

    while i < mmr.len() {
        let r = &mut mmr[i];
        if *r == U256_ZERO {
            *r = cur;
            return;
        } else {
            cur = hash_two_to_one_u256(*r, cur);
            *r = U256_ZERO;
        }
        i += 1;
    }

    // Vector length insufficient. Increase by 1.
    mmr.push(cur);
}

// Returns the new stream head after applying the updates.
// - head: the stream head to update.
// - events: a list of events for each checkpoint.
pub fn apply_stream_updates(
    head: &EventStreamHead,
    events: Vec<Vec<EventCommitment>>,
) -> EventStreamHead {
    let mut new_head = head.clone();
    for cp_events in events {
        // Verify that there are events in the checkpoint.
        debug_assert!(!cp_events.is_empty());

        // TODO: checkpoint_seq in EventCommitment is always 0, so we don't validate it

        let digest = build_event_merkle_root(&cp_events);
        let merkle_root = U256::from_le_bytes(&digest.into_inner());
        add_to_stream(&mut new_head.mmr, merkle_root);
        new_head.num_events += cp_events.len() as u64;
    }
    new_head
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use sui_types::digests::Digest;

    use super::*;

    #[test]
    fn test_basic() {
        let mut stream_head = EventStreamHead::new();
        let new_val = U256::from(1u64);
        add_to_stream(&mut stream_head.mmr, new_val);
        assert_eq!(stream_head.mmr, vec![U256::from(1u64)]);
    }

    #[test]
    fn test_compat_with_framework() {
        let mut stream_head = EventStreamHead::new();

        for i in 0..8 {
            let new_val = U256::from(50u64 + i);
            add_to_stream(&mut stream_head.mmr, new_val);
        }

        // This should match the Move test_mmr_digest_compat_with_rust result
        assert_eq!(
            stream_head.mmr,
            vec![
                U256::from(0u64),
                U256::from(0u64),
                U256::from(0u64),
                U256::from_str(
                    "69725770072863840208899320192042305265295220676851872214494910464384102654361"
                )
                .unwrap()
            ]
        );
    }

    #[test]
    fn test_verify_stream_head_update() {
        let old_head = EventStreamHead::new();
        let events = vec![vec![EventCommitment::new(0, 0, 0, Digest::new([1; 32]))]];
        let new_head = apply_stream_updates(&old_head, events);
        println!("{:?}", new_head);
    }
}
