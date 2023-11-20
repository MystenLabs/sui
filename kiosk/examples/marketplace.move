// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// An example of a Marketplace. In Kiosk terms, a Marketplace is an entity similar
/// to Creator - it owns and manages a `TransferPolicy` special to the marketplace
/// and when a marketplace deal happens (eg via the `marketplace_adapter`), the
/// marketplace enforces its own rules on the deal.
///
/// For reference, see the `marketplace_adapter` module.
module kiosk::marketplace_example {
    use sui::tx_context::{sender, TxContext};
    use sui::transfer_policy as policy;
    use sui::transfer;

    /// The One-Time-Witness for the module.
    struct MARKETPLACE_EXAMPLE has drop {}

    /// A type identifying the Marketplace.
    struct MyMarket has drop {}

    #[allow(unused_function)]
    #[lint_allow(share_owned)]
    /// As easy as creating a Publisher; for simplicity's sake we also create
    /// the `TransferPolicy` but this action can be performed offline in a PTB.
    fun init(otw: MARKETPLACE_EXAMPLE, ctx: &mut TxContext) {
        let publisher = sui::package::claim(otw, ctx);
        let (policy, policy_cap) = policy::new<MyMarket>(&publisher, ctx);

        transfer::public_share_object(policy);
        transfer::public_transfer(policy_cap, sender(ctx));
        transfer::public_transfer(publisher, sender(ctx));
    }
}
