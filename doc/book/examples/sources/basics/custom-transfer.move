// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x0::restricted_transfer {
    use sui::tx_context::{Self, TxContext};
    use sui::balance::{Self, Balance};
    use sui::coin::{Self, Coin};
    use sui::id::VersionedID;
    use sui::transfer;
    use sui::sui::SUI;

    /// For when paid amount is not equal to the transfer price.
    const EWrongAmount: u64 = 0;

    /// An object that marks a property ownership
    struct TitleDeed has key {
        id: VersionedID,
        // ... some additional fields
    }

    /// A centralized registry that approves property ownership
    /// transfers and collects fees.
    struct LandRegistry has key {
        id: VersionedID,
        balance: Balance<SUI>,
        fee: u64
    }

    /// Create a `LandRegistry` on module init.
    fun init(ctx: &mut TxContext) {
        transfer::share_object(LandRegistry {
            id: tx_context::new_id(ctx),
            balance: balance::zero<SUI>(),
            fee: 10000
        })
    }

    // .... some functionality to acquire TitleDeeds ...

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
