// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Simple token swap module for peer-to-peer exchanges.
///
/// This module allows two parties to trustlessly swap tokens.
/// One party creates a swap offer with their tokens, and another
/// party can accept it by providing the requested tokens.
module token_swap::simple_swap {
    use sui::object::{Self, Info, UID};
    use sui::coin::{Self, Coin};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    /// Error codes
    const EInvalidSwap: u64 = 0;
    const ENotOwner: u64 = 1;
    const EInsufficientAmount: u64 = 2;

    /// A swap offer holding tokens from the creator
    struct SwapOffer<phantom OfferedType, phantom RequestedType> has key {
        id: UID,
        creator: address,
        offered_coin: Coin<OfferedType>,
        requested_amount: u64,
    }

    /// Create a new swap offer
    /// The creator deposits their tokens and specifies what they want in return
    public entry fun create_swap<OfferedType, RequestedType>(
        offered: Coin<OfferedType>,
        requested_amount: u64,
        ctx: &mut TxContext
    ) {
        assert!(coin::value(&offered) > 0, EInvalidSwap);
        assert!(requested_amount > 0, EInvalidSwap);

        let swap = SwapOffer<OfferedType, RequestedType> {
            id: object::new(ctx),
            creator: tx_context::sender(ctx),
            offered_coin: offered,
            requested_amount,
        };

        transfer::share_object(swap);
    }

    /// Accept a swap offer by providing the requested tokens
    /// Both parties receive their respective tokens
    public entry fun accept_swap<OfferedType, RequestedType>(
        swap: SwapOffer<OfferedType, RequestedType>,
        payment: Coin<RequestedType>,
        ctx: &mut TxContext
    ) {
        let SwapOffer {
            id,
            creator,
            offered_coin,
            requested_amount
        } = swap;

        assert!(coin::value(&payment) >= requested_amount, EInsufficientAmount);

        // Transfer offered tokens to the acceptor
        transfer::transfer(offered_coin, tx_context::sender(ctx));

        // Transfer payment to the creator
        transfer::transfer(payment, creator);

        object::delete(id);
    }

    /// Cancel a swap offer (only by creator)
    /// Returns the offered tokens to the creator
    public entry fun cancel_swap<OfferedType, RequestedType>(
        swap: SwapOffer<OfferedType, RequestedType>,
        ctx: &mut TxContext
    ) {
        let SwapOffer {
            id,
            creator,
            offered_coin,
            requested_amount: _
        } = swap;

        assert!(tx_context::sender(ctx) == creator, ENotOwner);

        // Return tokens to creator
        transfer::transfer(offered_coin, creator);

        object::delete(id);
    }

    /// View functions

    /// Get the amount of tokens being offered
    public fun offered_amount<OfferedType, RequestedType>(
        swap: &SwapOffer<OfferedType, RequestedType>
    ): u64 {
        coin::value(&swap.offered_coin)
    }

    /// Get the amount of tokens being requested
    public fun requested_amount<OfferedType, RequestedType>(
        swap: &SwapOffer<OfferedType, RequestedType>
    ): u64 {
        swap.requested_amount
    }

    /// Get the creator of the swap
    public fun creator<OfferedType, RequestedType>(
        swap: &SwapOffer<OfferedType, RequestedType>
    ): address {
        swap.creator
    }
}
