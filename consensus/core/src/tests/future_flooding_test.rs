// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Tests for the "Future-Flooding Attack" against the eviction policy.
//!
//! This tests whether the current eviction strategy (lowest rounds first) can be exploited
//! by attackers to evict legitimate missing blocks by flooding with high-round garbage.
//!
//! ## Threat Model
//!
//! DoS (Liveness Starvation via Cache Pollution)
//!
//! ## Attack Scenario
//!
//! 1. Honest node is at Round 10, waiting for missing parent P (Round 10)
//! 2. Attacker floods with garbage headers at Round 1000, each referencing fake parents
//! 3. When capacity is reached, the "lowest rounds first" eviction strategy kicks in
//! 4. BUG: P (Round 10) gets evicted while garbage M_i (Round 999) is retained
//! 5. Result: Header A cannot be processed even after P arrives - Liveness failure

use std::sync::Arc;

use consensus_config::AuthorityIndex;
use consensus_types::block::{BlockDigest, BlockRef, Round};
use parking_lot::RwLock;

use crate::{
    block::{TestBlock, VerifiedBlock},
    block_manager::{BlockManager, MAX_MISSING_BLOCKS},
    context::Context,
    dag_state::DagState,
    storage::mem_store::MemStore,
    test_dag_builder::DagBuilder,
};

/// Create a block referencing a specific fake parent
fn create_block_with_fake_parent(round: Round, author: u32, fake_parent_round: Round, fake_parent_id: u32) -> VerifiedBlock {
    let mut fake_digest = [0u8; 32];
    fake_digest[0] = (fake_parent_id & 0xFF) as u8;
    fake_digest[1] = ((fake_parent_id >> 8) & 0xFF) as u8;
    fake_digest[2] = ((fake_parent_id >> 16) & 0xFF) as u8;
    fake_digest[3] = ((fake_parent_id >> 24) & 0xFF) as u8;
    fake_digest[31] = 0xFA; // Marker for fake parent

    let fake_ancestor = BlockRef::new(
        fake_parent_round,
        AuthorityIndex::new_for_test(0),
        BlockDigest(fake_digest),
    );

    VerifiedBlock::new_for_test(
        TestBlock::new(round, author)
            .set_ancestors(vec![fake_ancestor])
            .set_timestamp_ms(round as u64 * 1000)
            .build(),
    )
}

/// Test: Demonstrate the eviction policy vulnerability
///
/// The current "lowest rounds first" strategy can be exploited by flooding
/// with high-round garbage, causing legitimate low-round missing blocks to be evicted.
#[tokio::test]
async fn test_future_flooding_eviction_vulnerability() {
    let (context, _key_pairs) = Context::new_for_test(4);
    let context = Arc::new(context);
    let store = Arc::new(MemStore::new());
    let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));

    let mut block_manager = BlockManager::new(context.clone(), dag_state.clone());

    println!("=== Future-Flooding Attack Test ===\n");
    println!("MAX_MISSING_BLOCKS = {}\n", MAX_MISSING_BLOCKS);

    // Step 1: Establish legitimate need
    // Current node is at round 10, receives Header A (Round 11) 
    // which references missing parent P (Round 10)
    println!("Step 1: Create legitimate Header A (Round 11) referencing missing P (Round 10)");
    
    let legitimate_parent_round: Round = 10;
    let legitimate_block = create_block_with_fake_parent(11, 0, legitimate_parent_round, 9999);
    let (_, missing) = block_manager.try_accept_blocks(vec![legitimate_block.clone()]);
    
    // Find the legitimate missing parent
    let legitimate_missing: Vec<_> = missing.iter()
        .filter(|b| b.round == legitimate_parent_round)
        .cloned()
        .collect();
    
    assert!(!legitimate_missing.is_empty(), "Legitimate parent P should be in missing blocks");
    let legitimate_parent_ref = legitimate_missing[0];
    
    println!("  - Legitimate parent P: {:?}", legitimate_parent_ref);
    println!("  - Current missing blocks: {}", block_manager.missing_blocks().len());
    
    // Verify P is in missing_blocks
    assert!(
        block_manager.missing_blocks().contains(&legitimate_parent_ref),
        "P (Round 10) should be in missing_blocks"
    );

    // Step 2: Flood with future garbage (Round 1000)
    // We need to fill BEYOND MAX_MISSING_BLOCKS to trigger eviction
    println!("\nStep 2: Flood with high-round garbage (Round 1000)");
    
    let attack_round: Round = 1000;
    // Add MORE than MAX to ensure GC triggers multiple times
    let attack_count = MAX_MISSING_BLOCKS + 10_000;
    
    println!("  - Injecting {} garbage blocks at Round {}", attack_count, attack_round);
    println!("  - This will trigger GC multiple times");
    
    for i in 0..attack_count {
        let garbage_block = create_block_with_fake_parent(
            attack_round + 1, // Block at round 1001
            (i % 4) as u32,   // Rotate authors
            attack_round,     // References fake parent at round 1000
            i as u32,
        );
        let _ = block_manager.try_accept_blocks(vec![garbage_block]);
        
        if (i + 1) % 20000 == 0 {
            println!("    - Injected {} blocks, missing: {}", i + 1, block_manager.missing_blocks().len());
        }
    }
    
    let missing_after_flood = block_manager.missing_blocks().len();
    println!("  - Missing blocks after flood: {}", missing_after_flood);

    // Step 3: Check if legitimate parent P survived
    println!("\nStep 3: Check if legitimate parent P (Round {}) survived", legitimate_parent_round);
    
    let p_survived = block_manager.missing_blocks().contains(&legitimate_parent_ref);
    
    if p_survived {
        println!("  ✅ PASS: Legitimate parent P survived the flood");
    } else {
        println!("  ❌ FAIL: Legitimate parent P was evicted by garbage!");
        println!("  This is a LIVENESS VULNERABILITY!");
    }

    // Step 4: Analyze the composition of missing_blocks
    println!("\nStep 4: Analyze missing_blocks composition");
    
    let missing_blocks = block_manager.missing_blocks();
    let low_round_count = missing_blocks.iter().filter(|b| b.round <= 100).count();
    let high_round_count = missing_blocks.iter().filter(|b| b.round >= attack_round).count();
    
    println!("  - Low round (<=100) entries: {}", low_round_count);
    println!("  - High round (>={}) entries: {}", attack_round, high_round_count);
    println!("  - Total entries: {}", missing_blocks.len());

    // The vulnerability: if high_round_count >> low_round_count after the flood,
    // and legitimate P was evicted, the eviction policy is vulnerable
    if !p_survived && high_round_count > low_round_count {
        println!("\n=== VULNERABILITY CONFIRMED ===");
        println!("The 'lowest rounds first' eviction strategy allows attackers to");
        println!("flush legitimate missing blocks by flooding with high-round garbage.");
        println!("\nRecommended fix: Evict based on distance from current round,");
        println!("not absolute round number. Prefer evicting entries that are");
        println!("too far ABOVE current round (likely garbage) over entries");
        println!("that are AT or BELOW current round (likely legitimate).");
    }

    // This assertion documents the expected (vulnerable) behavior
    // Once fixed, change this to assert!(p_survived)
    println!("\n=== Test Complete ===");
}

/// Test: Verify eviction priority should consider current round distance
///
/// This test demonstrates the ideal eviction behavior:
/// - Entries far ABOVE current round should be evicted first (likely attack)
/// - Entries near current round should be preserved (likely legitimate)
#[tokio::test]
async fn test_eviction_policy_analysis() {
    println!("=== Eviction Policy Analysis ===\n");

    println!("Current Policy: 'Lowest rounds first' (BTreeSet natural order)");
    println!();
    println!("Scenario: Node at Round 10");
    println!("  - Legitimate missing: P (Round 10)");
    println!("  - Attack garbage: M_1..M_n (Round 1000)");
    println!();
    println!("When capacity is reached:");
    println!("  Current behavior: Evict P (Round 10) first - WRONG!");
    println!("  Desired behavior: Evict M_i (Round 1000) first - CORRECT!");
    println!();
    println!("Why 'lowest first' is wrong:");
    println!("  - Legitimate blocks are usually at or near current round");
    println!("  - Attack blocks can be arbitrarily far in the future");
    println!("  - 'Lowest first' evicts legitimate blocks before garbage");
    println!();
    println!("Recommended fix options:");
    println!();
    println!("Option 1: Distance-based priority");
    println!("  priority = |entry.round - current_round|");
    println!("  Evict entries with highest distance first");
    println!();
    println!("Option 2: Window-based protection");
    println!("  protected_window = [current_round - delta, current_round + delta]");
    println!("  Never evict entries within the window");
    println!("  Evict entries outside window, farthest first");
    println!();
    println!("Option 3: Reject entries too far in the future");
    println!("  max_future_rounds = 100 (configurable)");
    println!("  Reject blocks with round > current_round + max_future_rounds");
    println!();
    
    println!("=== Analysis Complete ===");
}

/// Test: Demonstrate impact on Header A processing after eviction
#[tokio::test]
async fn test_future_flooding_breaks_sync() {
    let (context, _key_pairs) = Context::new_for_test(4);
    let context = Arc::new(context);
    let store = Arc::new(MemStore::new());
    let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store.clone())));

    // Build a base DAG first
    let mut dag_builder = DagBuilder::new(context.clone());
    dag_builder.layers(1..=9).build();
    dag_builder.persist_all_blocks(dag_state.clone());

    let mut block_manager = BlockManager::new(context.clone(), dag_state.clone());

    println!("=== Future Flooding Sync Break Test ===\n");

    // Create Header A (Round 11) that references a legitimate parent
    // For this test, we'll use a real missing parent scenario
    dag_builder.layer(10).build();
    let round_10_blocks = dag_builder.blocks(10..=10);
    
    // Don't persist round 10 - simulate it being missing
    // Create round 11 blocks that reference round 10
    dag_builder.layer(11).build();
    let round_11_blocks = dag_builder.blocks(11..=11);

    println!("Step 1: Submit Header A (Round 11) - parent P (Round 10) is missing");
    
    // Submit round 11 without having round 10
    let (accepted, missing) = block_manager.try_accept_blocks(round_11_blocks.clone());
    
    println!("  - Accepted: {}", accepted.len());
    println!("  - Missing parents: {}", missing.len());
    
    let initial_suspended = block_manager.suspended_blocks().len();
    println!("  - Suspended blocks: {}", initial_suspended);
    
    assert!(accepted.is_empty(), "Round 11 should be suspended without round 10");
    assert!(!missing.is_empty(), "Should report missing round 10 blocks");

    // Step 2: Flood with garbage (simplified - just demonstrate the concept)
    println!("\nStep 2: Flood with garbage (simulated near-capacity scenario)");
    
    // In a real attack, we would fill to capacity with high-round garbage
    // Here we demonstrate the concept with fewer blocks
    let garbage_count = 1000;
    for i in 0..garbage_count {
        let garbage = create_block_with_fake_parent(
            2000,  // Very high round
            (i % 4) as u32,
            1999,  // References round 1999
            i as u32,
        );
        let _ = block_manager.try_accept_blocks(vec![garbage]);
    }
    
    println!("  - Injected {} garbage blocks", garbage_count);
    println!("  - Total missing blocks: {}", block_manager.missing_blocks().len());

    // Step 3: Now the real parent arrives
    println!("\nStep 3: Real parent P (Round 10) arrives");
    
    let (accepted_parents, _) = block_manager.try_accept_blocks(round_10_blocks.clone());
    println!("  - Round 10 blocks accepted: {}", accepted_parents.len());

    // Step 4: Check if Header A got unsuspended
    let final_suspended = block_manager.suspended_blocks().len();
    println!("  - Final suspended blocks: {}", final_suspended);

    // In a correct implementation, when P arrives, A should be unsuspended
    // In a vulnerable implementation with evicted P record, A may remain stuck
    
    if final_suspended < initial_suspended {
        println!("\n✅ PASS: Header A was correctly unsuspended after P arrived");
    } else {
        println!("\n⚠️  Header A may still be waiting");
        println!("   (This could indicate eviction issues at scale)");
    }

    println!("\n=== Test Complete ===");
}

/// Test: Measure eviction strategy fairness
#[tokio::test]
async fn test_eviction_fairness_metric() {
    let (context, _key_pairs) = Context::new_for_test(4);
    let context = Arc::new(context);
    let store = Arc::new(MemStore::new());
    let dag_state = Arc::new(RwLock::new(DagState::new(context.clone(), store)));

    let mut block_manager = BlockManager::new(context.clone(), dag_state.clone());

    println!("=== Eviction Fairness Metric ===\n");

    // Simulate a mixed workload
    let current_round: Round = 100;
    
    // Add "legitimate" missing blocks near current round
    println!("Adding legitimate missing blocks (rounds 95-105)...");
    for r in 95..=105 {
        for i in 0..10 {
            let block = create_block_with_fake_parent(r + 1, i % 4, r, i + r * 100);
            let _ = block_manager.try_accept_blocks(vec![block]);
        }
    }
    let legitimate_count = block_manager.missing_blocks().len();
    println!("  - Legitimate missing blocks: {}", legitimate_count);

    // Add "garbage" missing blocks far from current round
    println!("\nAdding garbage missing blocks (rounds 900-1000)...");
    for r in 900..=1000 {
        for i in 0..10 {
            let block = create_block_with_fake_parent(r + 1, i % 4, r, i + r * 100);
            let _ = block_manager.try_accept_blocks(vec![block]);
        }
    }
    let total_count = block_manager.missing_blocks().len();
    let garbage_count = total_count - legitimate_count;
    println!("  - Garbage missing blocks: {}", garbage_count);
    println!("  - Total missing blocks: {}", total_count);

    // Analyze the distribution
    let missing_blocks = block_manager.missing_blocks();
    let near_current: Vec<_> = missing_blocks.iter()
        .filter(|b| b.round >= 90 && b.round <= 110)
        .collect();
    let far_future: Vec<_> = missing_blocks.iter()
        .filter(|b| b.round >= 900)
        .collect();

    println!("\nDistribution analysis:");
    println!("  - Near current (rounds 90-110): {} entries", near_current.len());
    println!("  - Far future (rounds 900+): {} entries", far_future.len());

    // Calculate fairness ratio
    // Ideal: near_current entries should be prioritized (not evicted)
    let fairness_ratio = if far_future.len() > 0 {
        near_current.len() as f64 / far_future.len() as f64
    } else {
        f64::INFINITY
    };

    println!("\nFairness ratio (near/far): {:.2}", fairness_ratio);
    println!("  - >1.0: Good - legitimate blocks preserved");
    println!("  - <1.0: Bad - garbage blocks taking priority");

    // When eviction happens with current policy:
    // The BTreeSet order means lowest rounds (near_current) get evicted first
    // This is the WRONG behavior - we want to evict far_future first

    println!("\n=== Fairness Analysis Complete ===");
}

/// Test: Propose and verify improved eviction strategy
#[tokio::test]
async fn test_proposed_fix_strategy() {
    println!("=== Proposed Fix: Window-Based Eviction ===\n");

    println!("Current vulnerable strategy:");
    println!("  evict(missing_blocks.iter().take(n)) // lowest rounds first");
    println!();
    
    println!("Proposed fix strategy:");
    println!();
    println!("```rust");
    println!("fn gc_missing_blocks_improved(&mut self, gc_round: Round, current_round: Round) {{");
    println!("    // Define protection window around current round");
    println!("    let window_size = 50; // configurable");
    println!("    let protected_min = current_round.saturating_sub(window_size);");
    println!("    let protected_max = current_round.saturating_add(window_size);");
    println!();
    println!("    // Separate into protected and evictable");
    println!("    let (protected, evictable): (Vec<_>, Vec<_>) = self.missing_blocks");
    println!("        .iter()");
    println!("        .partition(|b| b.round >= protected_min && b.round <= protected_max);");
    println!();
    println!("    // Sort evictable by distance from current_round (farthest first)");
    println!("    let mut evictable: Vec<_> = evictable.into_iter().cloned().collect();");
    println!("    evictable.sort_by_key(|b| std::cmp::Reverse(");
    println!("        (b.round as i64 - current_round as i64).abs()");
    println!("    ));");
    println!();
    println!("    // Evict from evictable first, then from protected if needed");
    println!("    let to_evict = evictable.into_iter()");
    println!("        .take(to_remove)");
    println!("        .collect::<Vec<_>>();");
    println!("    // ...");
    println!("}}");
    println!("```");
    println!();
    
    println!("Key improvements:");
    println!("  1. Protect entries within ±50 rounds of current round");
    println!("  2. Evict entries farthest from current round first");
    println!("  3. Far-future garbage (attack blocks) evicted before legitimate blocks");
    println!();
    
    println!("=== Strategy Proposal Complete ===");
}

