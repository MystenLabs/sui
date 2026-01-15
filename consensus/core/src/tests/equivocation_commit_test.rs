// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Tests for equivocation handling in the Linearizer.
//!
//! ## The Bug
//!
//! Before the fix, `linearize_sub_dag()` used `is_committed(BlockRef)` which checks
//! the full BlockRef including Digest. Two equivocating blocks with the same
//! (Round, Author) but different Digests were treated as different blocks.
//!
//! This could lead to double-commit: if Block A is committed, Block B (same slot,
//! different digest) could still be committed later via a different DAG path.
//!
//! ## The Fix
//!
//! Added `is_any_block_at_slot_committed(Slot)` check to prevent committing any
//! block if another block at the same slot has already been committed.

use std::sync::Arc;

use consensus_config::AuthorityIndex;
use consensus_types::block::BlockRef;
use parking_lot::RwLock;

use crate::{
    block::{BlockAPI, Slot, TestBlock, Transaction, VerifiedBlock},
    context::Context,
    dag_state::DagState,
    leader_schedule::{LeaderSchedule, LeaderSwapTable},
    linearizer::Linearizer,
    storage::mem_store::MemStore,
    universal_committer::universal_committer_builder::UniversalCommitterBuilder,
};

/// Test that equivocating blocks at the same slot cannot be double-committed.
///
/// Scenario:
/// 1. Create two equivocating blocks A and B (same Round, Author; different Digest)
/// 2. Build main chain referencing Block A and commit it
/// 3. Insert Block B into DAG
/// 4. Build side chain referencing Block B
/// 5. Commit side chain leader
/// 6. Assert: Block B should NOT be in the committed blocks
#[tokio::test]
async fn test_equivocation_double_commit_prevented() {
    // Use production gc_depth = 60 (from sui-protocol-config)
    const PRODUCTION_GC_DEPTH: u32 = 60;

    let (mut context, _key_pairs) = Context::new_for_test(4);
    context
        .protocol_config
        .set_consensus_gc_depth_for_testing(PRODUCTION_GC_DEPTH);
    let context = Arc::new(context);
    let store = Arc::new(MemStore::new());
    let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

    let leader_schedule = Arc::new(LeaderSchedule::new(
        context.clone(),
        LeaderSwapTable::default(),
    ));

    let mut linearizer = Linearizer::new(context.clone(), dag_state.clone());
    let committer = UniversalCommitterBuilder::new(
        context.clone(),
        leader_schedule.clone(),
        dag_state.clone(),
    )
    .with_number_of_leaders(1)
    .build();

    // Equivocator is Authority 1 (not own_index=0)
    let equivocator: u32 = 1;
    let equivocation_round: u32 = 1;

    // Get genesis refs
    let genesis_refs: Vec<BlockRef> = dag_state
        .read()
        .get_last_cached_block_per_authority(1)
        .iter()
        .map(|(b, _)| b.reference())
        .collect();

    // Create equivocating Block A
    let block_a = VerifiedBlock::new_for_test(
        TestBlock::new(equivocation_round, equivocator)
            .set_ancestors(genesis_refs.clone())
            .set_transactions(vec![Transaction::new(b"Block A".to_vec())])
            .set_timestamp_ms(1000)
            .build(),
    );
    let ref_a = block_a.reference();
    dag_state.write().accept_block(block_a.clone());

    // Create other Round 1 blocks
    for author in 0..4u32 {
        if author == equivocator {
            continue;
        }
        let mut auth_ancestors = genesis_refs.clone();
        if let Some(pos) = auth_ancestors
            .iter()
            .position(|a| a.author.value() == author as usize)
        {
            auth_ancestors.swap(0, pos);
        }
        let block = VerifiedBlock::new_for_test(
            TestBlock::new(1, author)
                .set_ancestors(auth_ancestors)
                .set_transactions(vec![Transaction::new(vec![author as u8])])
                .set_timestamp_ms(1000 + author as u64)
                .build(),
        );
        dag_state.write().accept_block(block);
    }

    // Build main chain Rounds 2-6
    for round in 2..=6u32 {
        let prev_round_refs: Vec<BlockRef> = {
            let dag = dag_state.read();
            let mut refs = vec![];
            for author in 0..4u32 {
                let slot = Slot::new(round - 1, AuthorityIndex::new_for_test(author));
                let blocks = dag.get_uncommitted_blocks_at_slot(slot);
                if let Some(b) = blocks.first() {
                    refs.push(b.reference());
                }
            }
            refs
        };

        for author in 0..4u32 {
            let mut auth_ancestors = prev_round_refs.clone();
            if let Some(pos) = auth_ancestors
                .iter()
                .position(|a| a.author.value() == author as usize)
            {
                auth_ancestors.swap(0, pos);
            }
            let block = VerifiedBlock::new_for_test(
                TestBlock::new(round, author)
                    .set_ancestors(auth_ancestors)
                    .set_transactions(vec![Transaction::new(vec![round as u8, author as u8])])
                    .set_timestamp_ms(round as u64 * 1000 + author as u64)
                    .build(),
            );
            dag_state.write().accept_block(block);
        }
    }

    // Commit main chain (includes Block A)
    let last_decided = Slot::new(0, AuthorityIndex::new_for_test(0));
    let decided_leaders = committer.try_decide(last_decided);

    let mut first_leader_slot = Slot::new(0, AuthorityIndex::new_for_test(0));
    let mut block_a_committed = false;

    if let Some(decided) = decided_leaders.first() {
        if let Some(leader_block) = decided.clone().into_committed_block() {
            first_leader_slot = Slot::new(leader_block.round(), leader_block.author());
            let subdags = linearizer.handle_commit(vec![leader_block.clone()]);

            if let Some(subdag) = subdags.first() {
                block_a_committed = subdag.blocks.iter().any(|b| b.reference() == ref_a);
            }
        }
    }

    assert!(block_a_committed, "Block A should be committed in main chain");

    // Create equivocating Block B
    let block_b = VerifiedBlock::new_for_test(
        TestBlock::new(equivocation_round, equivocator)
            .set_ancestors(genesis_refs.clone())
            .set_transactions(vec![Transaction::new(b"Block B".to_vec())])
            .set_timestamp_ms(1001)
            .build(),
    );
    let ref_b = block_b.reference();
    dag_state.write().accept_block(block_b.clone());

    // Verify Block A and B are at the same slot with different digests
    assert_eq!(ref_a.round, ref_b.round);
    assert_eq!(ref_a.author, ref_b.author);
    assert_ne!(ref_a.digest, ref_b.digest);

    // Get Round 6 refs for side chain
    let round_6_refs: Vec<BlockRef> = {
        let dag = dag_state.read();
        let mut refs = vec![];
        for author in 0..4u32 {
            let slot = Slot::new(6, AuthorityIndex::new_for_test(author));
            let blocks = dag.get_uncommitted_blocks_at_slot(slot);
            if let Some(b) = blocks.first() {
                refs.push(b.reference());
            }
        }
        refs
    };

    // Build side chain Round 7 with one block referencing Block B
    for author in 0..4u32 {
        let mut auth_ancestors = round_6_refs.clone();
        if author == 2 {
            // Authority 2 references Block B
            auth_ancestors.push(ref_b);
        }
        if let Some(pos) = auth_ancestors
            .iter()
            .position(|a| a.author.value() == author as usize)
        {
            auth_ancestors.swap(0, pos);
        }
        let block = VerifiedBlock::new_for_test(
            TestBlock::new(7, author)
                .set_ancestors(auth_ancestors)
                .set_transactions(vec![Transaction::new(vec![7u8, author as u8])])
                .set_timestamp_ms(7000 + author as u64)
                .build(),
        );
        dag_state.write().accept_block(block);
    }

    // Build Rounds 8-12
    for round in 8..=12u32 {
        let prev_round_refs: Vec<BlockRef> = {
            let dag = dag_state.read();
            let mut refs = vec![];
            for author in 0..4u32 {
                let slot = Slot::new(round - 1, AuthorityIndex::new_for_test(author));
                let blocks = dag.get_uncommitted_blocks_at_slot(slot);
                if let Some(b) = blocks.first() {
                    refs.push(b.reference());
                }
            }
            refs
        };

        for author in 0..4u32 {
            let mut auth_ancestors = prev_round_refs.clone();
            if let Some(pos) = auth_ancestors
                .iter()
                .position(|a| a.author.value() == author as usize)
            {
                auth_ancestors.swap(0, pos);
            }
            let block = VerifiedBlock::new_for_test(
                TestBlock::new(round, author)
                    .set_ancestors(auth_ancestors)
                    .set_transactions(vec![Transaction::new(vec![round as u8, author as u8])])
                    .set_timestamp_ms(round as u64 * 1000 + author as u64)
                    .build(),
            );
            dag_state.write().accept_block(block);
        }
    }

    // Commit side chain
    let decided_leaders_2 = committer.try_decide(first_leader_slot);

    let mut block_b_committed = false;
    for decided in &decided_leaders_2 {
        if let Some(leader_block) = decided.clone().into_committed_block() {
            let subdags = linearizer.handle_commit(vec![leader_block.clone()]);
            for subdag in &subdags {
                if subdag.blocks.iter().any(|b| b.reference() == ref_b) {
                    block_b_committed = true;
                }
            }
        }
    }

    // THE FIX: Block B should NOT be committed because Block A (same slot) was already committed
    assert!(
        !block_b_committed,
        "Block B should NOT be committed - same slot as Block A which was already committed"
    );
}

/// Test that is_any_block_at_slot_committed correctly detects committed blocks at a slot.
#[tokio::test]
async fn test_is_any_block_at_slot_committed() {
    let (context, _) = Context::new_for_test(4);
    let context = Arc::new(context);
    let store = Arc::new(MemStore::new());
    let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

    // Use Authority 1 (not own_index=0) to avoid own-equivocation panic
    let equivocator: u32 = 1;

    let genesis_refs: Vec<BlockRef> = dag_state
        .read()
        .get_last_cached_block_per_authority(1)
        .iter()
        .map(|(b, _)| b.reference())
        .collect();

    // Create two equivocating blocks
    let block_a = VerifiedBlock::new_for_test(
        TestBlock::new(1, equivocator)
            .set_ancestors(genesis_refs.clone())
            .set_transactions(vec![Transaction::new(vec![0xAA])])
            .set_timestamp_ms(1000)
            .build(),
    );

    let block_b = VerifiedBlock::new_for_test(
        TestBlock::new(1, equivocator)
            .set_ancestors(genesis_refs.clone())
            .set_transactions(vec![Transaction::new(vec![0xBB])])
            .set_timestamp_ms(1001)
            .build(),
    );

    let ref_a = block_a.reference();
    let ref_b = block_b.reference();
    let slot = Slot::new(1, AuthorityIndex::new_for_test(equivocator));

    dag_state.write().accept_block(block_a);
    dag_state.write().accept_block(block_b);

    // Initially, no block at slot is committed
    assert!(
        !dag_state.read().is_any_block_at_slot_committed(slot),
        "No block should be committed yet"
    );

    // Commit Block A
    dag_state.write().set_committed(&ref_a);

    // Now the slot should have a committed block
    assert!(
        dag_state.read().is_any_block_at_slot_committed(slot),
        "Slot should have committed block after committing Block A"
    );

    // Block B's is_committed should still be false (different digest)
    assert!(
        !dag_state.read().is_committed(&ref_b),
        "Block B should not be marked as committed (different digest)"
    );

    // But is_any_block_at_slot_committed should return true
    assert!(
        dag_state.read().is_any_block_at_slot_committed(slot),
        "Slot check should detect Block A is committed"
    );
}

