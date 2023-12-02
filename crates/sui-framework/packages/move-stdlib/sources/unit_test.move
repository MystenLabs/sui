// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// Module providing testing functionality. Only included for tests.
module std::unit_test {

    /// DEPRECATED
    native public fun create_signers_for_testing(num_signers: u64): vector<signer>;

    /// This function is used to poison modules compiled in `test` mode.
    /// This will cause a linking failure if an attempt is made to publish a
    /// test module in a VM that isn't in unit test mode.
    native public fun poison();
}
