// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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
    struct Rule<phantom Proof: drop> has drop {}

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

