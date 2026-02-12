// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Tests for `advance_epoch` focusing on `storage_fund::advance_epoch` balance splits.
///
/// The storage fund has a `total_object_storage_rebates` balance that must satisfy:
///   non_refundable_storage_fee_amount + storage_rebate_amount
///       <= total_object_storage_rebates + storage_charges
///
/// These tests verify the function succeeds at boundaries and aborts when violated.
#[test_only]
module sui_system::advance_epoch_storage_fund_tests;

use std::unit_test::assert_eq;
use sui::balance;
use sui::sui::SUI;
use sui_system::test_runner;

const MIST_PER_SUI: u64 = 1_000_000_000;

// ========================================================================
// Positive tests: advance_epoch succeeds
// ========================================================================

#[test]
/// Basic flow: storage_charge covers both rebate and non-refundable fee.
fun test_storage_fund_basic_flow() {
    let mut runner = test_runner::new().build();

    // Epoch 1: charge 100 SUI of storage, rebate 50 SUI worth, non-refundable 10 SUI worth.
    let opts = runner.advance_epoch_opts()
        .storage_charge(100)
        .computation_charge(100)
        .storage_rebate(50 * MIST_PER_SUI)
        .non_refundable_storage_fee(10 * MIST_PER_SUI)
        .epoch_start_time(42);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    // After epoch: total_object_storage_rebates = 100 SUI - 50 SUI - 10 SUI = 40 SUI
    runner.system_tx!(|system, _| {
        assert_eq!(system.get_storage_fund_object_rebates(), 40 * MIST_PER_SUI);
    });

    runner.finish();
}

#[test]
/// Exact drain: rebate + non_refundable exactly equals storage_charge.
/// total_object_storage_rebates starts at 0, so everything comes from current charge.
fun test_storage_fund_exact_balance_drain() {
    let mut runner = test_runner::new().build();

    let opts = runner.advance_epoch_opts()
        .storage_charge(100)
        .computation_charge(100)
        .storage_rebate(60 * MIST_PER_SUI)
        .non_refundable_storage_fee(40 * MIST_PER_SUI)
        .epoch_start_time(42);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    runner.system_tx!(|system, _| {
        assert_eq!(system.get_storage_fund_object_rebates(), 0);
    });

    runner.finish();
}

#[test]
/// Multi-epoch accumulation: build up total_object_storage_rebates over several
/// epochs, then drain a large portion in one epoch.
fun test_storage_fund_multi_epoch_accumulation_then_drain() {
    let mut runner = test_runner::new().build();

    // Epoch 1: charge 200 SUI, rebate 0 → accumulates 200 SUI in rebate pool
    let opts = runner.advance_epoch_opts()
        .storage_charge(200)
        .computation_charge(100)
        .epoch_start_time(42);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    runner.system_tx!(|system, _| {
        assert_eq!(system.get_storage_fund_object_rebates(), 200 * MIST_PER_SUI);
    });

    // Epoch 2: charge 100 SUI, rebate 50 SUI → pool = 200 + 100 - 50 = 250 SUI
    let opts = runner.advance_epoch_opts()
        .storage_charge(100)
        .computation_charge(100)
        .storage_rebate(50 * MIST_PER_SUI)
        .epoch_start_time(84);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    runner.system_tx!(|system, _| {
        assert_eq!(system.get_storage_fund_object_rebates(), 250 * MIST_PER_SUI);
    });

    // Epoch 3: charge 0 SUI, rebate 250 SUI → drains everything from accumulated pool
    let opts = runner.advance_epoch_opts()
        .computation_charge(100)
        .storage_rebate(250 * MIST_PER_SUI)
        .epoch_start_time(126);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    runner.system_tx!(|system, _| {
        assert_eq!(system.get_storage_fund_object_rebates(), 0);
    });

    runner.finish();
}

#[test]
/// Multi-epoch accumulation: non-refundable fees can also drain against accumulated pool.
fun test_storage_fund_accumulated_then_non_refundable_drain() {
    let mut runner = test_runner::new().build();

    // Epoch 1: charge 300 SUI, no rebate
    let opts = runner.advance_epoch_opts()
        .storage_charge(300)
        .computation_charge(100)
        .epoch_start_time(42);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    // Epoch 2: charge 0, non_refundable = 300 SUI (drains from accumulated)
    let opts = runner.advance_epoch_opts()
        .computation_charge(100)
        .non_refundable_storage_fee(300 * MIST_PER_SUI)
        .epoch_start_time(84);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    runner.system_tx!(|system, _| {
        assert_eq!(system.get_storage_fund_object_rebates(), 0);
    });

    runner.finish();
}

#[test]
/// Safe mode accumulation: gas fees stashed during safe mode are folded into
/// the next successful advance_epoch. The storage_charge from safe mode adds
/// to total_object_storage_rebates, so rebates referencing the combined total
/// should succeed.
fun test_storage_fund_safe_mode_accumulation() {
    let mut runner = test_runner::new().build();

    // Epoch 1: charge 100 SUI to build up the pool
    let opts = runner.advance_epoch_opts()
        .storage_charge(100)
        .computation_charge(100)
        .epoch_start_time(42);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();
    // pool = 100 SUI

    // Simulate a safe mode epoch: bump epoch, stash 50 SUI storage rewards
    // and 30 SUI worth of storage rebates into the safe mode fields.
    runner.advance_epoch_safe_mode();
    runner.system_tx!(|system, _| {
        system.set_safe_mode_gas_for_testing(
            balance::create_for_testing<SUI>(50 * MIST_PER_SUI), // storage rewards
            balance::create_for_testing<SUI>(0),                  // computation rewards
            30 * MIST_PER_SUI,                                    // storage rebates
            10 * MIST_PER_SUI,                                    // non-refundable fee
        );
    });

    // Epoch 3 (recovery): charge 0 new storage, but the safe mode storage rewards
    // (50 SUI) get folded in. Pool becomes 100 + 50 = 150 SUI before splits.
    // Total splits: rebate = 30 SUI (safe mode) + non_refundable = 10 SUI (safe mode) = 40 SUI.
    // 40 <= 150, so this should succeed.
    let opts = runner.advance_epoch_opts()
        .computation_charge(100)
        .epoch_start_time(126);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    // pool = 100 + 50 (safe mode storage rewards) - 30 (rebate) - 10 (non-refundable) = 110 SUI
    runner.system_tx!(|system, _| {
        assert_eq!(system.get_storage_fund_object_rebates(), 110 * MIST_PER_SUI);
    });

    runner.finish();
}

#[test]
/// Edge case: zero storage charge and zero rebates. This is the simplest possible
/// epoch advance (no storage activity at all).
fun test_storage_fund_zero_charge_zero_rebate() {
    let mut runner = test_runner::new().build();

    let opts = runner.advance_epoch_opts()
        .computation_charge(100)
        .epoch_start_time(42);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    runner.system_tx!(|system, _| {
        assert_eq!(system.get_storage_fund_object_rebates(), 0);
    });

    runner.finish();
}

#[test]
/// Edge case: rebate of exactly 1 MIST with storage_charge of exactly 1 MIST.
fun test_storage_fund_dust_amounts() {
    let mut runner = test_runner::new().build();

    runner.system_tx!(|system, ctx| {
        let new_epoch = ctx.epoch() + 1;
        system.advance_epoch_for_testing(
            new_epoch,
            1,      // protocol version
            1,      // 1 MIST storage charge
            100 * MIST_PER_SUI, // computation charge
            1,      // 1 MIST storage rebate
            0,      // non-refundable
            0,      // reinvest rate
            0,      // slashing rate
            42,     // epoch start time
            ctx,
        ).destroy_for_testing();
    });
    runner.scenario_mut().next_epoch(@0);

    runner.system_tx!(|system, _| {
        assert_eq!(system.get_storage_fund_object_rebates(), 0);
    });

    runner.finish();
}

// ========================================================================
// Negative tests: advance_epoch aborts
// ========================================================================

#[test, expected_failure(abort_code = sui::balance::ENotEnough)]
/// Abort: storage_rebate exceeds total_object_storage_rebates + storage_charge
/// on the very first epoch (pool starts at 0).
fun test_storage_fund_rebate_exceeds_balance_first_epoch() {
    let mut runner = test_runner::new().build();

    // charge 100 SUI but rebate 101 SUI → 1 SUI over
    let opts = runner.advance_epoch_opts()
        .storage_charge(100)
        .computation_charge(100)
        .storage_rebate(100 * MIST_PER_SUI + 1)
        .epoch_start_time(42);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    runner.finish();
}

#[test, expected_failure(abort_code = sui::balance::ENotEnough)]
/// Abort: non_refundable_storage_fee exceeds total_object_storage_rebates + storage_charge.
fun test_storage_fund_non_refundable_exceeds_balance() {
    let mut runner = test_runner::new().build();

    let opts = runner.advance_epoch_opts()
        .storage_charge(100)
        .computation_charge(100)
        .non_refundable_storage_fee(100 * MIST_PER_SUI + 1)
        .epoch_start_time(42);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    runner.finish();
}

#[test, expected_failure(abort_code = sui::balance::ENotEnough)]
/// Abort: individually both fit, but combined they exceed the balance.
/// storage_charge = 100 SUI. non_refundable = 60 SUI, rebate = 60 SUI.
/// After non_refundable split, only 40 SUI remains for rebate → fails.
fun test_storage_fund_combined_exceeds_balance() {
    let mut runner = test_runner::new().build();

    let opts = runner.advance_epoch_opts()
        .storage_charge(100)
        .computation_charge(100)
        .storage_rebate(60 * MIST_PER_SUI)
        .non_refundable_storage_fee(60 * MIST_PER_SUI)
        .epoch_start_time(42);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    runner.finish();
}

#[test, expected_failure(abort_code = sui::balance::ENotEnough)]
/// Abort: zero storage_charge with positive rebate on first epoch.
/// total_object_storage_rebates is 0, so any rebate > 0 will fail.
fun test_storage_fund_zero_charge_nonzero_rebate_first_epoch() {
    let mut runner = test_runner::new().build();

    let opts = runner.advance_epoch_opts()
        .computation_charge(100)
        .storage_rebate(1) // just 1 MIST
        .epoch_start_time(42);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    runner.finish();
}

#[test, expected_failure(abort_code = sui::balance::ENotEnough)]
/// Abort: accumulated pool is drained over two epochs.
/// Epoch 1 builds up 100 SUI. Epoch 2 tries to rebate 101 SUI with no new charge.
fun test_storage_fund_accumulated_then_over_drain() {
    let mut runner = test_runner::new().build();

    // Epoch 1: charge 100 SUI, no rebate → pool = 100 SUI
    let opts = runner.advance_epoch_opts()
        .storage_charge(100)
        .computation_charge(100)
        .epoch_start_time(42);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    // Epoch 2: no charge, rebate 100 SUI + 1 MIST → exceeds pool
    let opts = runner.advance_epoch_opts()
        .computation_charge(100)
        .storage_rebate(100 * MIST_PER_SUI + 1)
        .epoch_start_time(84);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    runner.finish();
}

#[test, expected_failure(abort_code = sui::balance::ENotEnough)]
/// Abort: safe mode stashes rebates that, combined with epoch rebates,
/// exceed the pool balance.
fun test_storage_fund_safe_mode_rebate_overflow() {
    let mut runner = test_runner::new().build();

    // Epoch 1: charge 100 SUI → pool = 100 SUI
    let opts = runner.advance_epoch_opts()
        .storage_charge(100)
        .computation_charge(100)
        .epoch_start_time(42);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    // Safe mode: stash 60 SUI rebate (no new storage rewards to balance it)
    runner.advance_epoch_safe_mode();
    runner.system_tx!(|system, _| {
        system.set_safe_mode_gas_for_testing(
            balance::create_for_testing<SUI>(0),
            balance::create_for_testing<SUI>(0),
            60 * MIST_PER_SUI, // storage rebates
            0,
        );
    });

    // Recovery epoch: charge 0, rebate 50 SUI. Total rebate = 50 + 60 (safe mode) = 110 SUI.
    // Pool = 100 SUI + 0 (safe mode storage rewards) = 100 SUI. 110 > 100 → abort.
    let opts = runner.advance_epoch_opts()
        .computation_charge(100)
        .storage_rebate(50 * MIST_PER_SUI)
        .epoch_start_time(126);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    runner.finish();
}

#[test, expected_failure(abort_code = sui::balance::ENotEnough)]
/// Abort: safe mode stashes non-refundable fees that push the total over.
fun test_storage_fund_safe_mode_non_refundable_overflow() {
    let mut runner = test_runner::new().build();

    // Epoch 1: charge 100 SUI → pool = 100 SUI
    let opts = runner.advance_epoch_opts()
        .storage_charge(100)
        .computation_charge(100)
        .epoch_start_time(42);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    // Safe mode: stash 80 SUI non-refundable fee
    runner.advance_epoch_safe_mode();
    runner.system_tx!(|system, _| {
        system.set_safe_mode_gas_for_testing(
            balance::create_for_testing<SUI>(0),
            balance::create_for_testing<SUI>(0),
            0,
            80 * MIST_PER_SUI,
        );
    });

    // Recovery: charge 0, non_refundable = 30 SUI. Total non_refundable = 30 + 80 = 110 SUI.
    // Pool = 100 SUI. 110 > 100 → abort.
    let opts = runner.advance_epoch_opts()
        .computation_charge(100)
        .non_refundable_storage_fee(30 * MIST_PER_SUI)
        .epoch_start_time(126);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    runner.finish();
}
