// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Verification tests for the Missing Blocks OOM fix

use std::sync::Arc;

use consensus_config::AuthorityIndex;
use consensus_types::block::{BlockDigest, BlockRef};
use parking_lot::RwLock;

use crate::{
    block::{TestBlock, VerifiedBlock},
    block_manager::{BlockManager, MAX_MISSING_BLOCKS},
    context::Context,
    dag_state::DagState,
    storage::mem_store::MemStore,
};

/// Creates an attack block referencing a fake parent
fn create_attack_block(round: u32, fake_parent_id: u32) -> VerifiedBlock {
    let mut fake_digest = [0u8; 32];
    fake_digest[0] = (fake_parent_id & 0xFF) as u8;
    fake_digest[1] = ((fake_parent_id >> 8) & 0xFF) as u8;
    fake_digest[2] = ((fake_parent_id >> 16) & 0xFF) as u8;
    fake_digest[3] = ((fake_parent_id >> 24) & 0xFF) as u8;

    let fake_ancestor = BlockRef::new(
        round - 1,
        AuthorityIndex::new_for_test(0),
        BlockDigest(fake_digest),
    );

    VerifiedBlock::new_for_test(
        TestBlock::new(round, 0)
            .set_ancestors(vec![fake_ancestor])
            .build(),
    )
}

/// Test: Missing blocks should be bounded at MAX_MISSING_BLOCKS after fix
#[tokio::test]
async fn test_fix_missing_blocks_bounded() {
    let (context, _key_pairs) = Context::new_for_test(4);
    let context = Arc::new(context);
    let store = Arc::new(MemStore::new());
    let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));

    let mut block_manager = BlockManager::new(context.clone(), dag_state.clone());

    println!("=== Missing Blocks OOM Fix Verification ===\n");
    println!("MAX_MISSING_BLOCKS = {}\n", MAX_MISSING_BLOCKS);

    // Try to inject more attack blocks than the limit
    let attack_count = MAX_MISSING_BLOCKS + 50_000;
    
    println!("Injecting {} attack blocks (exceeding limit by {})\n", attack_count, attack_count - MAX_MISSING_BLOCKS);

    for i in 0..attack_count {
        let block = create_attack_block(10 + (i / 1000) as u32, i as u32);
        let _ = block_manager.try_accept_blocks(vec![block]);

        if (i + 1) % 25_000 == 0 {
            let current_missing = block_manager.missing_blocks().len();
            println!(
                "Injected: {:6} | Missing: {:6} | Limit: {} | Status: {}",
                i + 1,
                current_missing,
                MAX_MISSING_BLOCKS,
                if current_missing <= MAX_MISSING_BLOCKS {
                    "OK - within limit"
                } else {
                    "FAIL - exceeded limit!"
                }
            );
        }
    }

    let final_missing = block_manager.missing_blocks().len();
    println!("\n=== Final Results ===");
    println!("Total attacks: {}", attack_count);
    println!("Final missing blocks: {}", final_missing);
    println!("Limit: {}", MAX_MISSING_BLOCKS);

    // Verify the fix works
    assert!(
        final_missing <= MAX_MISSING_BLOCKS,
        "Fix failed! Missing blocks ({}) exceeded limit ({})",
        final_missing,
        MAX_MISSING_BLOCKS
    );

    println!("\nFix verified! Missing blocks are correctly bounded.");
}

/// Test: GC cleanup mechanism
#[tokio::test]
async fn test_fix_gc_cleanup() {
    let (context, _key_pairs) = Context::new_for_test(4);
    let context = Arc::new(context);
    let store = Arc::new(MemStore::new());
    let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));

    let mut block_manager = BlockManager::new(context.clone(), dag_state.clone());

    println!("=== GC Cleanup Mechanism Test ===\n");

    // First inject some low round attack blocks
    println!("Injecting low round (5-10) attack blocks...");
    for i in 0..1000 {
        let block = create_attack_block(5 + (i % 5) as u32, i as u32);
        let _ = block_manager.try_accept_blocks(vec![block]);
    }

    let before_gc = block_manager.missing_blocks().len();
    println!("Missing blocks before GC: {}", before_gc);

    // Advance GC round (simulate commit)
    // Note: In real scenarios, gc_round advances via DagState
    // Here we inject high round blocks to trigger GC
    println!("\nInjecting high round (100+) attack blocks to trigger GC...");
    for i in 0..1000 {
        let block = create_attack_block(100 + (i / 100) as u32, 10000 + i as u32);
        let _ = block_manager.try_accept_blocks(vec![block]);
    }

    let after_gc = block_manager.missing_blocks().len();
    println!("Missing blocks after GC: {}", after_gc);

    println!("\nGC cleanup test completed.");
}

/// Test: Normal blocks are not affected by the fix
#[tokio::test]
async fn test_fix_normal_blocks_unaffected() {
    use crate::test_dag_builder::DagBuilder;

    let (context, _key_pairs) = Context::new_for_test(4);
    let context = Arc::new(context);
    let store = Arc::new(MemStore::new());
    let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

    let mut block_manager = BlockManager::new(context.clone(), dag_state.clone());

    println!("=== Normal Block Processing Test ===\n");

    // First fill up to near capacity
    println!("Filling missing blocks near capacity...");
    for i in 0..MAX_MISSING_BLOCKS - 100 {
        let block = create_attack_block(10 + (i / 1000) as u32, i as u32);
        let _ = block_manager.try_accept_blocks(vec![block]);
    }
    
    let before_normal = block_manager.missing_blocks().len();
    println!("Missing blocks after attack: {}", before_normal);

    // Build normal DAG - genesis blocks are already in dag_state
    let mut dag_builder = DagBuilder::new(context.clone());
    dag_builder.layers(1..=3).build();
    
    // Persist normal blocks to dag_state (simulate they have been received)
    dag_builder.persist_all_blocks(dag_state.clone());

    // Get round 4 blocks as test subjects
    dag_builder.layer(4).build();
    let normal_blocks = dag_builder.blocks(4..=4);

    println!("\nInjecting {} normal blocks...", normal_blocks.len());
    let (accepted, _) = block_manager.try_accept_blocks(normal_blocks.clone());

    println!("Accepted normal blocks: {}", accepted.len());
    println!("Expected: {}", normal_blocks.len());

    assert_eq!(
        accepted.len(),
        normal_blocks.len(),
        "All normal blocks should be accepted!"
    );

    println!("\nNormal block processing is not affected by missing blocks limit.");
}
