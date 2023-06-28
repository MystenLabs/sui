// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module examples::restricted_transfer {
    use sui::tx_context::{Self, TxContext};
    use sui::balance::{Self, Balance};
    use sui::coin::{Self, Coin};
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::sui::SUI;

    /// For when paid amount is not equal to the transfer price.
    const EWrongAmount: u64 = 0;

    /// A Capability that allows bearer to create new `TitleDeed`s.
    struct GovernmentCapability has key { id: UID }

    /// An object that marks a property ownership. Can only be issued
    /// by an authority.
    struct TitleDeed has key {
        id: UID,
        // ... some additional fields
    }

    /// A centralized registry that approves property ownership
    /// transfers and collects fees.
    struct LandRegistry has key {
        id: UID,
        balance: Balance<SUI>,
        fee: u64
    }

    #[allow(unused_function)]
    /// Create a `LandRegistry` on module init.
    fun init(ctx: &mut TxContext) {
        transfer::transfer(GovernmentCapability {
            id: object::new(ctx)
        }, tx_context::sender(ctx));

        transfer::share_object(LandRegistry {
            id: object::new(ctx),
            balance: balance::zero<SUI>(),
            fee: 10000
        })
    }

    /// Create `TitleDeed` and transfer it to the property owner.
    /// Only owner of the `GovernmentCapability` can perform this action.
    public entry fun issue_title_deed(
        _: &GovernmentCapability,
        for: address,
        ctx: &mut TxContext
    ) {
        transfer::transfer(TitleDeed {
            id: object::new(ctx)
        }, for)
    }

    /// A custom transfer function. Required due to `TitleDeed` not having
    /// a `store` ability. All transfers of `TitleDeed`s have to go through
    /// this function and pay a fee to the `LandRegistry`.
    public entry fun transfer_ownership(
        registry: &mut LandRegistry,
        paper: TitleDeed,
        fee: Coin<SUI>,
        to: address,
    ) {
        assert!(coin::value(&fee) == registry.fee, EWrongAmount);

        // add a payment to the LandRegistry balance
        balance::join(&mut registry.balance, coin::into_balance(fee));

        // finally call the transfer function
        transfer::transfer(paper, to)
    }
}
