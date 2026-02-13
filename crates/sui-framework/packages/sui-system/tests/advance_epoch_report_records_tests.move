// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Tests for `advance_epoch` focusing on validator report record cleanup.
///
/// When `compute_slashed_validators` runs during `advance_epoch`, it asserts that
/// every address in `validator_report_records` is an active validator
/// (`ENonValidatorInReportRecords`). It also calls `sum_voting_power_by_addresses`
/// on the reporters, which aborts with `ENotAValidator` if any reporter is not active.
///
/// Both checks run BEFORE validators are removed (`process_pending_removals` and
/// `update_validator_positions_and_calculate_total_stake`). The cleanup function
/// `clean_report_records_leaving_validator` runs DURING those removals and handles
/// both directions: reports ABOUT the departing validator and reports BY them.
///
/// These tests verify that `advance_epoch` succeeds across various departure
/// scenarios and that report records are properly cleaned.
#[test_only]
module sui_system::advance_epoch_report_records_tests;

use sui_system::test_runner;
use sui_system::validator_builder;

// ========================================================================
// Voluntary departure tests
// ========================================================================

#[test]
/// Reported validator voluntarily leaves. Records should be cleaned during
/// departure, and both the departing epoch and subsequent epochs should succeed.
fun test_reported_validator_voluntarily_leaves() {
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::new().sui_address(@1),
            validator_builder::new().sui_address(@2),
            validator_builder::new().sui_address(@3),
        ])
        .build();

    // @1 and @3 report @2
    runner.set_sender(@1).report_validator(@2);
    runner.set_sender(@3).report_validator(@2);

    // @2 requests voluntary removal
    runner.set_sender(@2).remove_validator();

    // Epoch 1: advance_epoch should succeed. compute_slashed_validators runs
    // first (when @2 is still active), then process_pending_removals removes @2
    // and cleans reports about @2.
    runner.advance_epoch(option::none()).destroy_for_testing();

    // Verify: @2's report records are gone
    runner.system_tx!(|system, _| {
        assert!(system.get_reporters_of(@2).is_empty());
    });

    // Epoch 2: subsequent advance_epoch should also succeed (no stale records)
    runner.advance_epoch(option::none()).destroy_for_testing();

    runner.finish();
}

#[test]
/// Reporter voluntarily leaves. Their reports on other validators should be
/// cleaned, so the reported validator no longer has stale reporter entries.
fun test_reporter_voluntarily_leaves() {
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::new().sui_address(@1),
            validator_builder::new().sui_address(@2),
            validator_builder::new().sui_address(@3),
        ])
        .build();

    // @1 reports @2
    runner.set_sender(@1).report_validator(@2);

    // @1 (the reporter) voluntarily leaves
    runner.set_sender(@1).remove_validator();

    // Epoch 1: advance_epoch should succeed. @1 is still active during
    // compute_slashed_validators (sum_voting_power_by_addresses won't abort).
    // Then @1 departs and clean_report_records_leaving_validator removes @1's
    // report on @2.
    runner.advance_epoch(option::none()).destroy_for_testing();

    // Verify: @2 has no reporters left (only @1 reported, and @1 left)
    runner.system_tx!(|system, _| {
        assert!(system.get_reporters_of(@2).is_empty());
    });

    // Epoch 2: subsequent advance_epoch with clean records
    runner.advance_epoch(option::none()).destroy_for_testing();

    runner.finish();
}

#[test]
/// Both the reporter and the reported validator leave in the same epoch.
/// All report records involving either should be cleaned.
fun test_reporter_and_reportee_both_leave_same_epoch() {
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::new().sui_address(@1),
            validator_builder::new().sui_address(@2),
            validator_builder::new().sui_address(@3),
            validator_builder::new().sui_address(@4),
        ])
        .build();

    // @1 reports @2, and @3 also reports @2
    runner.set_sender(@1).report_validator(@2);
    runner.set_sender(@3).report_validator(@2);

    // Both @1 (reporter) and @2 (reported) leave in the same epoch
    runner.set_sender(@1).remove_validator();
    runner.set_sender(@2).remove_validator();

    // Epoch 1: advance_epoch succeeds. Both are still active during
    // compute_slashed_validators. Then both depart and records are cleaned.
    runner.advance_epoch(option::none()).destroy_for_testing();

    // Verify: all report records involving @1 and @2 are gone
    runner.system_tx!(|system, _| {
        assert!(system.get_reporters_of(@1).is_empty());
        assert!(system.get_reporters_of(@2).is_empty());
    });

    // Epoch 2: clean state
    runner.advance_epoch(option::none()).destroy_for_testing();

    runner.finish();
}

#[test]
/// Reports persist across epochs when no validator leaves.
/// advance_epoch must succeed each time.
fun test_reports_persist_advance_epoch_succeeds() {
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::new().sui_address(@1),
            validator_builder::new().sui_address(@2),
            validator_builder::new().sui_address(@3),
        ])
        .build();

    // @1 reports @2
    runner.set_sender(@1).report_validator(@2);

    // Advance through 3 epochs — records should persist and advance_epoch
    // should succeed each time.
    runner.advance_epoch(option::none()).destroy_for_testing();
    runner.system_tx!(|system, _| {
        assert!(!system.get_reporters_of(@2).is_empty());
    });

    runner.advance_epoch(option::none()).destroy_for_testing();
    runner.system_tx!(|system, _| {
        assert!(!system.get_reporters_of(@2).is_empty());
    });

    runner.advance_epoch(option::none()).destroy_for_testing();
    runner.system_tx!(|system, _| {
        assert!(!system.get_reporters_of(@2).is_empty());
    });

    runner.finish();
}

// ========================================================================
// Involuntary departure (low stake kick) tests
// ========================================================================

#[test]
/// A reported validator is involuntarily kicked due to very low stake.
/// The report records should be cleaned during the kick, and the next
/// advance_epoch should succeed.
fun test_reported_validator_low_stake_kicked() {
    // @4 has 1 SUI stake while others have 10000 SUI each.
    // With total = 30001 SUI, @4's voting power ≈ 0, which is below the
    // very_low threshold (4 in phase 1), triggering immediate removal
    // regardless of grace period.
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::new().sui_address(@1).initial_stake(10_000),
            validator_builder::new().sui_address(@2).initial_stake(10_000),
            validator_builder::new().sui_address(@3).initial_stake(10_000),
            validator_builder::new().sui_address(@4).initial_stake(1),
        ])
        .build();

    // @1 reports @4
    runner.set_sender(@1).report_validator(@4);

    // Epoch 1: advance_epoch succeeds. compute_slashed_validators sees @4 as
    // active. Then update_validator_positions kicks @4 for very low stake and
    // cleans report records about @4.
    runner.advance_epoch(option::none()).destroy_for_testing();

    // Verify @4 is no longer active and records are clean
    runner.system_tx!(|system, _| {
        assert!(system.get_reporters_of(@4).is_empty());
    });

    // Epoch 2: subsequent advance_epoch with clean state
    runner.advance_epoch(option::none()).destroy_for_testing();

    runner.finish();
}

#[test]
/// A reporter is involuntarily kicked due to very low stake.
/// Their reports on other validators should be cleaned.
fun test_reporter_low_stake_kicked() {
    // @4 has 1 SUI stake, will be kicked for very low voting power
    // (VP ≈ 0 < very_low threshold of 4) — immediate removal.
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::new().sui_address(@1).initial_stake(10_000),
            validator_builder::new().sui_address(@2).initial_stake(10_000),
            validator_builder::new().sui_address(@3).initial_stake(10_000),
            validator_builder::new().sui_address(@4).initial_stake(1),
        ])
        .build();

    // @4 (soon to be kicked) reports @1
    runner.set_sender(@4).report_validator(@1);

    // Epoch 1: advance_epoch succeeds. compute_slashed_validators sees @4 as
    // active (as a reporter in @1's record). Then @4 is kicked for low stake
    // and clean_report_records removes @4's report on @1.
    runner.advance_epoch(option::none()).destroy_for_testing();

    // Verify: @1 no longer has any reporters
    runner.system_tx!(|system, _| {
        assert!(system.get_reporters_of(@1).is_empty());
    });

    // Epoch 2: clean state
    runner.advance_epoch(option::none()).destroy_for_testing();

    runner.finish();
}

// ========================================================================
// Cross-epoch report record lifecycle
// ========================================================================

#[test]
/// Report in epoch N, validator leaves in epoch N+1, advance in epoch N+2.
/// Verifies records survive one epoch and are cleaned in the next.
fun test_report_survives_epoch_then_cleaned_on_departure() {
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::new().sui_address(@1),
            validator_builder::new().sui_address(@2),
            validator_builder::new().sui_address(@3),
        ])
        .build();

    // Epoch 0: @1 reports @2
    runner.set_sender(@1).report_validator(@2);

    // Epoch 1: advance — records persist
    runner.advance_epoch(option::none()).destroy_for_testing();
    runner.system_tx!(|system, _| {
        assert!(!system.get_reporters_of(@2).is_empty());
    });

    // Epoch 1: @2 requests removal
    runner.set_sender(@2).remove_validator();

    // Epoch 2: advance — @2 leaves and records are cleaned
    runner.advance_epoch(option::none()).destroy_for_testing();
    runner.system_tx!(|system, _| {
        assert!(system.get_reporters_of(@2).is_empty());
    });

    // Epoch 3: clean state, advance succeeds
    runner.advance_epoch(option::none()).destroy_for_testing();

    runner.finish();
}

#[test]
/// Multiple reporters, one leaves, the other's report persists.
/// Then the reported validator leaves and all records are cleaned.
fun test_partial_reporter_departure() {
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::new().sui_address(@1),
            validator_builder::new().sui_address(@2),
            validator_builder::new().sui_address(@3),
            validator_builder::new().sui_address(@4),
        ])
        .build();

    // @1 and @3 both report @2
    runner.set_sender(@1).report_validator(@2);
    runner.set_sender(@3).report_validator(@2);

    // @1 leaves — only @1's report on @2 should be cleaned
    runner.set_sender(@1).remove_validator();
    runner.advance_epoch(option::none()).destroy_for_testing();

    // @3's report on @2 should still be present
    runner.system_tx!(|system, _| {
        let reporters = system.get_reporters_of(@2).into_keys();
        assert!(reporters == vector[@3]);
    });

    // advance_epoch succeeds with the remaining report
    runner.advance_epoch(option::none()).destroy_for_testing();

    // @2 now leaves — @3's report on @2 should be cleaned
    runner.set_sender(@2).remove_validator();
    runner.advance_epoch(option::none()).destroy_for_testing();

    runner.system_tx!(|system, _| {
        assert!(system.get_reporters_of(@2).is_empty());
    });

    // Final advance — fully clean state
    runner.advance_epoch(option::none()).destroy_for_testing();

    runner.finish();
}
