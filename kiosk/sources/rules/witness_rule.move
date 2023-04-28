// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Description:
/// This module implements a Rule that requires a "Proof" witness to be
/// presented on every transfer. The "Proof" witness is a type chosen by
/// the owner of the policy.
///
/// Configuration:
/// - The type to require for every transfer.
///
/// Use Cases:
/// - Can be used to link custom logic to the TransferPolicy via the Witness.
/// - Only allow trading on a certain marketplace.
/// - Require a confirmation in a third party module
/// - Implement a custom requirement on the creator side an link the logic.
///
module kiosk::witness_rule {
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
    public fun add<T: key + store, Proof: drop>(
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
