// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::witness_policy_tests {
    use sui::witness_policy;
    use sui::tx_context::dummy as ctx;
    use sui::transfer_policy as policy;
    use sui::transfer_policy_tests::{
        Self as test,
        Asset
    };

    /// Confirmation of an action to use in Policy.
    struct Proof has drop {}

    /// Malicious attempt to use a different proof.
    struct Cheat has drop {}

    #[test]
    fun test_default_flow() {
        let ctx = &mut ctx();
        let (policy, cap) = test::prepare(ctx);

        // set the lock policy and require `Proof` on every transfer.
        witness_policy::set<Asset, Proof>(&mut policy, &cap);

        let request = policy::new_request(0, test::fresh_id(ctx), ctx);

        witness_policy::prove(Proof {}, &policy, &mut request);
        policy::confirm_request(&policy, request);
        test::wrapup(policy, cap, ctx);
    }

    #[test]
    #[expected_failure(abort_code = sui::transfer_policy::EPolicyNotSatisfied)]
    fun test_no_proof() {
        let ctx = &mut ctx();
        let (policy, cap) = test::prepare(ctx);

        // set the lock policy and require `Proof` on every transfer.
        witness_policy::set<Asset, Proof>(&mut policy, &cap);
        let request = policy::new_request(0, test::fresh_id(ctx), ctx);

        policy::confirm_request(&policy, request);
        test::wrapup(policy, cap, ctx);
    }

    #[test]
    #[expected_failure(abort_code = sui::witness_policy::ERuleNotFound)]
    fun test_wrong_proof() {
        let ctx = &mut ctx();
        let (policy, cap) = test::prepare(ctx);

        // set the lock policy and require `Proof` on every transfer.
        witness_policy::set<Asset, Proof>(&mut policy, &cap);

        let request = policy::new_request(0, test::fresh_id(ctx), ctx);

        witness_policy::prove(Cheat {}, &policy, &mut request);
        policy::confirm_request(&policy, request);
        test::wrapup(policy, cap, ctx);
    }
}
