// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// Module providing testing functionality. Only included for tests.
module std::unit_test;

/// DEPRECATED
public native fun create_signers_for_testing(num_signers: u64): vector<signer>;

/// This function is used to poison modules compiled in `test` mode.
/// This will cause a linking failure if an attempt is made to publish a
/// test module in a VM that isn't in unit test mode.
public native fun poison();

public macro fun assert_eq<$T: drop>($t1: $T, $t2: $T) {
    let t1 = $t1;
    let t2 = $t2;
    assert_ref_eq!(&t1, &t2)
}

public macro fun assert_ref_eq<$T>($t1: &$T, $t2: &$T) {
    let t1 = $t1;
    let t2 = $t2;
    let res = t1 == t2;
    if (!res) {
        std::debug::print(&b"Assertion failed:".to_string());
        std::debug::print(t1);
        std::debug::print(&b"!=".to_string());
        std::debug::print(t2);
        assert!(false);
    }
}
