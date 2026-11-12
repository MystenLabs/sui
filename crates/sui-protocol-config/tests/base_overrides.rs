// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This is the only test in its binary because it changes process-global overrides that other
// tests read.

use sui_protocol_config::ProtocolConfig;

#[test]
fn base_overrides_stack_beneath_test_override() {
    let get = ProtocolConfig::get_for_max_version_UNSAFE;
    let original = get().min_checkpoint_interval_ms();

    let first = ProtocolConfig::apply_base_overrides_for_testing(|_, mut cfg| {
        cfg.set_min_checkpoint_interval_ms_for_testing(1000);
        cfg.set_random_beacon_min_round_interval_ms_for_testing(1000);
        cfg
    });
    let second = ProtocolConfig::apply_base_overrides_for_testing(|_, mut cfg| {
        cfg.set_random_beacon_min_round_interval_ms_for_testing(2000);
        cfg
    });
    assert_eq!(get().min_checkpoint_interval_ms(), 1000);
    assert_eq!(get().random_beacon_min_round_interval_ms(), 2000);

    {
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
            cfg.set_min_checkpoint_interval_ms_for_testing(3000);
            cfg
        });
        assert_eq!(get().min_checkpoint_interval_ms(), 3000);
    }

    drop(second);
    assert_eq!(get().random_beacon_min_round_interval_ms(), 1000);
    drop(first);
    assert_eq!(get().min_checkpoint_interval_ms(), original);
}
