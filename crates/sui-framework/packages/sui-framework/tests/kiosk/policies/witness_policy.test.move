// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// Requires a Witness on every transfer. Witness needs to be generated
/// in some way and presented to the `prove` method for the TransferRequest
/// to receive a matching receipt.
///
/// One important use case for this policy is the ability to lock something
/// in the `Kiosk`. When an item is placed into the Kiosk, a `PlacedWitness`
/// struct is created which can be used to prove that the `T` was placed
/// to the `Kiosk`.
module sui::witness_policy {
    use sui::transfer_policy::{
        Self as policy,
        TransferPolicy,
        TransferPolicyCap,
        TransferRequest
    };

    /// When a Proof does not find its Rule<Proof>.
    const ERuleNotFound: u64 = 0;

    /// Custom witness-key for the "proof policy".
    public struct Rule<phantom Proof: drop> has drop {}

    /// Creator action: adds the Rule.
    /// Requires a "Proof" witness confirmation on every transfer.
    public fun set<T: key + store, Proof: drop>(
        policy: &mut TransferPolicy<T>,
        cap: &TransferPolicyCap<T>
    ) {
        policy::add_rule(Rule<Proof> {}, policy, cap, true);
    }

    /// Buyer action: follow the policy.
    /// Present the required "Proof" instance to get a receipt.
    public fun prove<T: key + store, Proof: drop>(
        _proof: Proof,
        policy: &TransferPolicy<T>,
        request: &mut TransferRequest<T>
    ) {
        assert!(policy::has_rule<T, Rule<Proof>>(policy), ERuleNotFound);
        policy::add_receipt(Rule<Proof> {}, request)
    }
}

#[test_only]
module sui::witness_policy_tests {
    use sui::witness_policy;
    use sui::transfer_policy as policy;
    use sui::transfer_policy_tests::{
        Self as test,
        Asset
    };

    /// Confirmation of an action to use in Policy.
    public struct Proof has drop {}

    /// Malicious attempt to use a different proof.
    public struct Cheat has drop {}

    #[test]
    fun test_default_flow() {
        let ctx = &mut tx_context::dummy();
        let (mut policy, cap) = test::prepare(ctx);

        // set the lock policy and require `Proof` on every transfer.
        witness_policy::set<Asset, Proof>(&mut policy, &cap);

        let mut request = policy::new_request(test::fresh_id(ctx), 0, test::fresh_id(ctx));

        witness_policy::prove(Proof {}, &policy, &mut request);
        policy.confirm_request(request);
        test::wrapup(policy, cap, ctx);
    }

    #[test]
    #[expected_failure(abort_code = sui::transfer_policy::EPolicyNotSatisfied)]
    fun test_no_proof() {
        let ctx = &mut tx_context::dummy();
        let (mut policy, cap) = test::prepare(ctx);

        // set the lock policy and require `Proof` on every transfer.
        witness_policy::set<Asset, Proof>(&mut policy, &cap);
        let request = policy::new_request(test::fresh_id(ctx), 0, test::fresh_id(ctx));

        policy.confirm_request(request);
        test::wrapup(policy, cap, ctx);
    }

    #[test]
    #[expected_failure(abort_code = sui::witness_policy::ERuleNotFound)]
    fun test_wrong_proof() {
        let ctx = &mut tx_context::dummy();
        let (mut policy, cap) = test::prepare(ctx);

        // set the lock policy and require `Proof` on every transfer.
        witness_policy::set<Asset, Proof>(&mut policy, &cap);

        let mut request = policy::new_request(test::fresh_id(ctx), 0, test::fresh_id(ctx));

        witness_policy::prove(Cheat {}, &policy, &mut request);
        policy.confirm_request(request);
        test::wrapup(policy, cap, ctx);
    }
}
