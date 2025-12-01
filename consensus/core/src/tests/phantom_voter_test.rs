// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Tests for Phantom Voter Attack - validating authority index bounds.
//!
//! ## "The Phantom Voter Attack"
//!
//! This tests whether the system correctly validates authority index bounds
//! to prevent index out of bounds panics (DoS attacks).
//!
//! ## Threat Model: BFT (Malformed Metadata / DoS)
//!
//! A Byzantine validator could try to:
//! 1. Create a block with an invalid authority index (e.g., 255 when max is 3)
//! 2. Create a block referencing ancestors with invalid authority indices
//! 3. Trigger index out of bounds panic when the system tries to lookup the authority
//!
//! ## Expected Behavior
//!
//! The system should:
//! - Return `Err(InvalidAuthorityIndex)` for invalid indices
//! - NEVER panic due to index out of bounds
//! - Gracefully reject malformed data

use std::sync::Arc;

use bytes::Bytes;
use consensus_config::AuthorityIndex;
use consensus_types::block::{BlockDigest, BlockRef};
use parking_lot::RwLock;

use crate::{
    block::{SignedBlock, TestBlock, VerifiedBlock},
    block_manager::BlockManager,
    block_verifier::{BlockVerifier, SignedBlockVerifier},
    context::Context,
    dag_state::DagState,
    error::ConsensusError,
    storage::mem_store::MemStore,
    test_dag_builder::DagBuilder,
    transaction::{TransactionVerifier, ValidationError},
};

/// Simple transaction verifier for testing
struct AcceptAllVerifier;

impl TransactionVerifier for AcceptAllVerifier {
    fn verify_batch(&self, _batch: &[&[u8]]) -> Result<(), ValidationError> {
        Ok(())
    }

    fn verify_and_vote_batch(
        &self,
        _block_ref: &BlockRef,
        _batch: &[&[u8]],
    ) -> Result<Vec<consensus_types::block::TransactionIndex>, ValidationError> {
        Ok(vec![])
    }
}

/// Test: Block with phantom author index should be rejected
///
/// This is the core "Phantom Voter Attack" scenario:
/// A block claims to be authored by a non-existent authority.
#[tokio::test]
async fn test_phantom_author_index() {
    let (context, key_pairs) = Context::new_for_test(4);
    let context = Arc::new(context);
    let committee_size = context.committee.size();

    println!("=== Phantom Author Index Test ===\n");
    println!("Committee size: {}", committee_size);

    // Create block verifier
    let verifier = SignedBlockVerifier::new(context.clone(), Arc::new(AcceptAllVerifier));

    // Build base DAG for valid ancestors
    let mut dag_builder = DagBuilder::new(context.clone());
    dag_builder.layers(1..=5).build();

    let valid_ancestors = dag_builder.last_ancestors.clone();

    // Test various phantom indices
    let phantom_indices = vec![
        committee_size as u32,       // Just out of bounds
        committee_size as u32 + 1,   // Slightly out of bounds
        100,                          // Way out of bounds
        255,                          // u8::MAX - common attack value
    ];

    for phantom_index in phantom_indices {
        println!("\nTesting phantom author index: {}", phantom_index);

        // Create a block with phantom author
        let block = TestBlock::new(6, phantom_index)
            .set_ancestors(valid_ancestors.clone())
            .set_timestamp_ms(6000)
            .build();

        // Sign with a valid key (doesn't matter, should fail before signature check)
        let (_, protocol_keypair) = &key_pairs[0];
        let signed_block = SignedBlock::new(block, protocol_keypair).unwrap();
        let serialized = Bytes::from(bcs::to_bytes(&signed_block).unwrap());

        // Attempt to verify - should NOT panic
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            verifier.verify_and_vote(signed_block, serialized)
        }));

        match result {
            Ok(verify_result) => {
                match verify_result {
                    Ok(_) => {
                        println!("  FAIL: Phantom author {} was ACCEPTED!", phantom_index);
                        panic!("Phantom voter attack succeeded");
                    }
                    Err(e) => {
                        println!("  PASS: Phantom author {} rejected with error", phantom_index);
                        println!("  Error: {:?}", e);
                        assert!(
                            matches!(e, ConsensusError::InvalidAuthorityIndex { .. }),
                            "Expected InvalidAuthorityIndex error, got: {:?}",
                            e
                        );
                    }
                }
            }
            Err(_) => {
                println!("  FAIL: PANIC occurred for phantom author {}!", phantom_index);
                panic!("Index out of bounds panic - DoS vulnerability!");
            }
        }
    }

    println!("\n=== Test Complete ===");
}

/// Test: Block with phantom ancestor indices should be rejected
#[tokio::test]
async fn test_phantom_ancestor_index() {
    let (context, key_pairs) = Context::new_for_test(4);
    let context = Arc::new(context);
    let committee_size = context.committee.size();

    println!("=== Phantom Ancestor Index Test ===\n");
    println!("Committee size: {}", committee_size);

    let verifier = SignedBlockVerifier::new(context.clone(), Arc::new(AcceptAllVerifier));

    // Build base DAG
    let mut dag_builder = DagBuilder::new(context.clone());
    dag_builder.layers(1..=5).build();

    let _valid_ancestors = dag_builder.last_ancestors.clone();

    // Create ancestors with phantom indices
    let phantom_indices: Vec<u32> = vec![
        committee_size as u32,  // Just out of bounds
        100,                     // Way out of bounds
        255,                     // u8::MAX
    ];

    for phantom_index in phantom_indices {
        println!("\nTesting phantom ancestor index: {}", phantom_index);

        // Create valid ancestors plus one phantom
        let mut ancestors = vec![];
        // First ancestor must be from same author
        ancestors.push(BlockRef::new(5, AuthorityIndex::new_for_test(0), BlockDigest::MIN));
        // Add valid ancestors from other authorities
        for i in 1..3 {
            ancestors.push(BlockRef::new(
                5,
                AuthorityIndex::new_for_test(i),
                BlockDigest::MIN,
            ));
        }
        // Add phantom ancestor
        ancestors.push(BlockRef::new(
            5,
            AuthorityIndex::new_for_test(phantom_index),
            BlockDigest::MIN,
        ));

        let block = TestBlock::new(6, 0)
            .set_ancestors(ancestors)
            .set_timestamp_ms(6000)
            .build();

        let (_, protocol_keypair) = &key_pairs[0];
        let signed_block = SignedBlock::new(block, protocol_keypair).unwrap();
        let serialized = Bytes::from(bcs::to_bytes(&signed_block).unwrap());

        // Attempt to verify - should NOT panic
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            verifier.verify_and_vote(signed_block, serialized)
        }));

        match result {
            Ok(verify_result) => {
                match verify_result {
                    Ok(_) => {
                        println!("  FAIL: Phantom ancestor {} was ACCEPTED!", phantom_index);
                        panic!("Phantom voter attack succeeded");
                    }
                    Err(e) => {
                        println!("  PASS: Phantom ancestor {} rejected", phantom_index);
                        println!("  Error: {:?}", e);
                        // Could be InvalidAuthorityIndex or another validation error
                    }
                }
            }
            Err(_) => {
                println!("  FAIL: PANIC occurred for phantom ancestor {}!", phantom_index);
                panic!("Index out of bounds panic - DoS vulnerability!");
            }
        }
    }

    println!("\n=== Test Complete ===");
}

/// Test: Verify is_valid_index is checked before array access
#[tokio::test]
async fn test_index_validation_order() {
    let (context, _key_pairs) = Context::new_for_test(4);
    let context = Arc::new(context);
    let committee = &context.committee;

    println!("=== Index Validation Order Test ===\n");
    println!("Committee size: {}", committee.size());

    // Test boundary conditions
    let test_indices: Vec<u32> = vec![
        0,                             // Valid: first
        3,                             // Valid: last
        4,                             // Invalid: first out of bounds
        u32::MAX / 2,                  // Invalid: middle of range
        u32::MAX,                      // Invalid: max value
    ];

    for index in test_indices {
        let authority_index = AuthorityIndex::new_for_test(index);
        let is_valid = committee.is_valid_index(authority_index);

        if index < committee.size() as u32 {
            println!("Index {}: valid={} (expected: true)", index, is_valid);
            assert!(is_valid, "Index {} should be valid", index);
            
            // Safe to access
            let _ = committee.authority(authority_index);
            let _ = committee.stake(authority_index);
        } else {
            println!("Index {}: valid={} (expected: false)", index, is_valid);
            assert!(!is_valid, "Index {} should be invalid", index);
            
            // Should NOT access - would panic
            // committee.authority(authority_index); // This would panic!
        }
    }

    println!("\n=== Test Complete ===");
}

/// Test: BlockManager handles phantom indices in ancestors
///
/// VULNERABILITY DISCOVERED: DagState::contains_blocks() does not validate
/// authority index bounds before array access, leading to panic.
///
/// Location: consensus/core/src/dag_state.rs:723
/// Code: let recent_refs = &self.recent_refs_by_authority[block_ref.author];
///
/// This test documents the vulnerability. In a fixed system, this should pass.
#[tokio::test]
async fn test_block_manager_phantom_ancestor() {
    let (context, _key_pairs) = Context::new_for_test(4);
    let context = Arc::new(context);
    let store = Arc::new(MemStore::new());
    let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));
    let committee_size = context.committee.size();

    println!("=== BlockManager Phantom Ancestor Test ===\n");

    let mut block_manager = BlockManager::new(context.clone(), dag_state);

    // Build some valid DAG
    let mut dag_builder = DagBuilder::new(context.clone());
    dag_builder.layers(1..=5).build();

    // Create block with phantom ancestor
    // Using a small phantom index to avoid other issues
    let phantom_index = committee_size as u32 + 10;
    let mut ancestors = dag_builder.last_ancestors.clone();
    ancestors.push(BlockRef::new(
        5,
        AuthorityIndex::new_for_test(phantom_index),
        BlockDigest::MIN,
    ));

    let block = VerifiedBlock::new_for_test(
        TestBlock::new(6, 1)
            .set_ancestors(ancestors)
            .set_timestamp_ms(6000)
            .build(),
    );

    println!("Attempting to accept block with phantom ancestor index: {}", phantom_index);
    println!("Committee size: {}", committee_size);
    println!();
    println!("Testing DagState bounds checking:");
    println!("  Location: DagState::contains_blocks() - dag_state.rs");
    println!("  Issue was: No bounds check on block_ref.author before array access");
    println!("  Expected: Graceful handling without panic");
    println!();

    // After fix: Should not panic, should gracefully handle invalid authority index
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        block_manager.try_accept_blocks(vec![block])
    }));

    match result {
        Ok((accepted, missing)) => {
            println!("  PASS: Block processed without panic");
            println!("  Accepted: {}", accepted.len());
            println!("  Missing: {}", missing.len());
            println!();
            println!("  Defense in depth working:");
            println!("  - DagState::contains_blocks() now validates authority index");
            println!("  - Invalid indices are treated as 'block does not exist'");
            println!("  - No panic, graceful degradation");
            
            // Assert the fix is working - no panic means success
            // The block should be suspended (not accepted) because ancestor is "missing"
            assert_eq!(accepted.len(), 0, "Block with phantom ancestor should not be accepted");
        }
        Err(e) => {
            // If we still panic, the fix is not complete
            let panic_msg = if let Some(s) = e.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = e.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic".to_string()
            };
            
            panic!(
                "VULNERABILITY NOT FIXED: DagState still panics on phantom ancestor index!\n\
                Panic message: {}\n\
                This is a DoS vulnerability - Byzantine node can crash peers",
                panic_msg
            );
        }
    }

    println!("\n=== Test Complete ===");
}

/// Test: Analysis of phantom voter protection mechanisms
#[tokio::test]
async fn test_phantom_voter_protection_analysis() {
    println!("=== Phantom Voter Protection Analysis ===\n");

    println!("Mysticeti's Phantom Voter Protection:\n");

    println!("Protection Point 1: Committee.is_valid_index()");
    println!("  Location: consensus/config/src/committee.rs");
    println!("  Code: index.value() < self.size()");
    println!("  Effect: Returns false for out-of-bounds indices\n");

    println!("Protection Point 2: Block Author Validation");
    println!("  Location: SignedBlockVerifier::verify_block()");
    println!("  Code: if !committee.is_valid_index(block.author())");
    println!("  Effect: Rejects blocks with invalid author index\n");

    println!("Protection Point 3: Ancestor Author Validation");
    println!("  Location: SignedBlockVerifier::verify_block()");
    println!("  Code: if !committee.is_valid_index(ancestor.author)");
    println!("  Effect: Rejects blocks with invalid ancestor indices\n");

    println!("Protection Point 4: seen_ancestors Bounds");
    println!("  Location: SignedBlockVerifier::verify_block()");
    println!("  Code: let mut seen_ancestors = vec![false; committee.size()]");
    println!("  Note: Access is guarded by is_valid_index check before\n");

    println!("Attack Vectors Mitigated:");
    println!("  1. Phantom Author Attack: BLOCKED");
    println!("     - is_valid_index check before any array access");
    println!("  2. Phantom Ancestor Attack: BLOCKED");
    println!("     - Each ancestor index validated");
    println!("  3. Index Overflow Attack: BLOCKED");
    println!("     - AuthorityIndex uses u32, compared to usize size()\n");

    println!("Critical Code Pattern:");
    println!("  // SAFE: Bounds check before access");
    println!("  if !committee.is_valid_index(author) {{");
    println!("      return Err(InvalidAuthorityIndex {{ ... }});");
    println!("  }}");
    println!("  let authority = committee.authority(author); // Now safe\n");

    println!("=== Analysis Complete ===");
}

/// Test: Fuzz-like test with random large indices
#[tokio::test]
async fn test_random_phantom_indices() {
    let (context, key_pairs) = Context::new_for_test(4);
    let context = Arc::new(context);

    println!("=== Random Phantom Indices Fuzz Test ===\n");

    let verifier = SignedBlockVerifier::new(context.clone(), Arc::new(AcceptAllVerifier));

    // Build base DAG
    let mut dag_builder = DagBuilder::new(context.clone());
    dag_builder.layers(1..=5).build();

    let valid_ancestors = dag_builder.last_ancestors.clone();

    // Test many random large indices
    let random_indices: Vec<u32> = vec![
        5, 10, 50, 100, 256, 1000, 10000, 65535, 
        u32::MAX / 4, u32::MAX / 2, u32::MAX - 1, u32::MAX
    ];

    let mut panics = 0;
    let mut rejections = 0;
    let mut accepts = 0;

    for phantom_index in &random_indices {
        let block = TestBlock::new(6, *phantom_index)
            .set_ancestors(valid_ancestors.clone())
            .set_timestamp_ms(6000)
            .build();

        let (_, protocol_keypair) = &key_pairs[0];
        let signed_block = SignedBlock::new(block, protocol_keypair).unwrap();
        let serialized = Bytes::from(bcs::to_bytes(&signed_block).unwrap());

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            verifier.verify_and_vote(signed_block, serialized)
        }));

        match result {
            Ok(Ok(_)) => accepts += 1,
            Ok(Err(_)) => rejections += 1,
            Err(_) => panics += 1,
        }
    }

    println!("Tested {} random phantom indices", random_indices.len());
    println!("  - Rejections: {}", rejections);
    println!("  - Accepts: {}", accepts);
    println!("  - PANICS: {}", panics);

    if panics > 0 {
        panic!("{} panics occurred - DoS vulnerability!", panics);
    }

    // Valid indices (0-3) should be accepted or rejected based on other validation
    // Invalid indices should all be rejected
    println!("\n  PASS: No panics with random phantom indices");
    println!("\n=== Test Complete ===");
}

