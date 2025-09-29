use fastcrypto::hash::{Blake2b256, HashFunction};
use fastcrypto::merkle::MerkleTree;
use move_core_types::u256::U256;
use serde::Serialize;
use sui_types::digests::Digest;

const U256_ZERO: U256 = U256::zero();

#[derive(Debug, Serialize, Clone)]
pub struct EventCommitment {
    checkpoint_seq: u64,
    transaction_idx: u64,
    event_idx: u64,
    digest: Digest,
}

impl EventCommitment {
    fn new(checkpoint_seq: u64, transaction_idx: u64, event_idx: u64, digest: Digest) -> Self {
        Self {
            checkpoint_seq,
            transaction_idx,
            event_idx,
            digest,
        }
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct StreamHead {
    mmr_digest: Vec<U256>,
    checkpoint_seq: u64,
    num_events: u64,
}

impl StreamHead {
    pub fn new() -> Self {
        Self {
            mmr_digest: vec![],
            checkpoint_seq: 0,
            num_events: 0,
        }
    }
}

fn hash_two_to_one_u256(left: U256, right: U256) -> U256 {
    let mut concatenated = bcs::to_bytes(&left).expect("Failed to serialize left U256");
    concatenated.extend_from_slice(&bcs::to_bytes(&right).expect("Failed to serialize right U256"));
    let hash = Blake2b256::digest(&concatenated);
    U256::from_le_bytes(&hash.digest)
}

// Build the Merkle root of all events in a single checkpoint
fn build_event_merkle_root(events: &[EventCommitment]) -> U256 {
    // Debug assertion to ensure events are ordered by the natural order of EventCommitment
    debug_assert!(
        events.windows(2).all(|pair| {
            let (a, b) = (&pair[0], &pair[1]);
            (a.checkpoint_seq, a.transaction_idx, a.event_idx)
                <= (b.checkpoint_seq, b.transaction_idx, b.event_idx)
        }),
        "Events must be ordered by (checkpoint_seq, transaction_idx, event_idx)"
    );

    let merkle_tree = MerkleTree::<Blake2b256>::build_from_unserialized(events.to_vec())
        .expect("failed to serialize event commitments for merkle root");
    let root_node = merkle_tree.root();
    let root_digest = root_node.bytes();
    U256::from_le_bytes(&root_digest)
}

fn add_to_stream(mmr_digest: &mut Vec<U256>, new_val: U256) {
    let mut i = 0;
    let mut cur = new_val;

    while i < mmr_digest.len() {
        let r = &mut mmr_digest[i];
        if *r == U256_ZERO {
            *r = cur;
            return;
        } else {
            cur = hash_two_to_one_u256(*r, cur);
            *r = U256_ZERO;
        }
        i = i + 1;
    }

    // Vector length insufficient. Increase by 1.
    mmr_digest.push(cur);
}

// Returns the new stream head after applying the updates.
// - head: the stream head to update.
// - events: a list of events for each checkpoint.
pub fn apply_stream_updates(head: &StreamHead, events: Vec<Vec<EventCommitment>>) -> StreamHead {
    let mut new_head = head.clone();
    let mut old_checkpoint_seq = head.checkpoint_seq;
    for cp_events in events {
        // Verify that there are events in the checkpoint.
        debug_assert!(cp_events.len() > 0);
        let cur_checkpoint_seq = cp_events[0].checkpoint_seq;
        // Verify that the checkpoint number is same for each group of events.
        debug_assert!(cp_events
            .iter()
            .all(|event| event.checkpoint_seq == cur_checkpoint_seq));
        // Verify that the checkpoint number is monotonically increasing.
        debug_assert!(old_checkpoint_seq < cur_checkpoint_seq);

        let merkle_root = build_event_merkle_root(&cp_events);
        add_to_stream(&mut new_head.mmr_digest, merkle_root);
        new_head.num_events += cp_events.len() as u64;
        new_head.checkpoint_seq = cur_checkpoint_seq;
        old_checkpoint_seq = cur_checkpoint_seq;
    }
    new_head
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn test_basic() {
        let mut stream_head = StreamHead::new();
        let new_val = U256::from(1u64);
        add_to_stream(&mut stream_head.mmr_digest, new_val);
        assert_eq!(stream_head.mmr_digest, vec![U256::from(1u64)]);
    }

    #[test]
    fn test_compat_with_framework() {
        let mut stream_head = StreamHead::new();

        for i in 0..8 {
            let new_val = U256::from(50u64 + i);
            add_to_stream(&mut stream_head.mmr_digest, new_val);
        }

        // This should match the Move test_mmr_digest_compat_with_rust result
        assert_eq!(
            stream_head.mmr_digest,
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
        let old_head = StreamHead::new();
        let events = vec![vec![EventCommitment::new(1, 0, 0, Digest::new([1; 32]))]];
        let new_head = apply_stream_updates(&old_head, events);
        println!("{:?}", new_head);
    }
}
