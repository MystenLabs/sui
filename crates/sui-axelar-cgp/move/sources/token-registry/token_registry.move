// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// The token-linker application. Allows permission-less registering of new
/// tokens and coins and validates the symbol of the token. Only available once
/// the message is received and a new token can be added.
module axelar::token_registry {
    use sui::coin::{Self, Coin, TreasuryCap, CoinMetadata};
    use sui::dynamic_object_field as dof;
    use sui::dynamic_field as df;

    use axelar::channel::Channel;

    /// Trying to register a token with non-zero supply.
    const ESupplyNotEmpty: u64 = 0;

    /// The registry of all tokens and Coins. New tokens can only be added in a
    /// verified way - the name, symbol and decimals are checked against the
    /// received 'deployStandardizedToken' message.
    struct TokenRegistry has key {
        id: UID,
        /// The channel to receive messages. Using the `bool` type as there's
        /// nothing to approve in this channel and it does not wrap any logic.
        channel: Channel<bool>,
    }

    /// The registered token. Contains Axelar-specific identification as well
    /// as the `TreasuryCap` object and the `CoinMetadata`.
    struct RegisteredToken<phantom T> has store {
        treasury_cap: TreasuryCap<T>,
        metadata: CoinMetadata<T>,
        /// TODO: consider making it the df key for easier lookup and validation.
        token_id: vector<u8>
    }

    /// The key for the `RegisteredToken` object.
    struct TokenKey<phantom T> has store, drop {}

    /// Add a new token to the `TokenRegistry`. Once added, the token can be
    /// used by the Interchain Token Service.
    public fun add_token<T>(
        self: &mut TokenRegistry,
        treasury_cap: TreasuryCap<T>,
        metadata: CoinMetadata<T>,
        ctx: &mut TxContext,
    ) {
        assert!(coin::total_supply(&treasury_cap) == 0, ESupplyNotEmpty);
        // perform necessary checks on the metadata
        // compare the type name to the coin Symbol
        // make sure the data in the message matches both

        df::add(&mut self.id, TokenKey<T> {}, RegisteredToken {
            treasury_cap,
            metadata,
            token_id: b"" // pass in the tokenId from the message
        });
    }

    /// Mint a new token and transfer it to the recipient.
    public fun receive_token<T>(
        self: &mut TokenRegistry,
        message: vector<u8>
    ) {
        // parse the message
        // validate the message + match against the channel
        // get the treasury cap
        // mint and transfer to recipient specified in the message
    }

    public fun send_token<T>(
        self: &mut TokenRegistry,
        coin: Coin<T>,
        recipient: address,

        ctx: &mut TxContext,
    ) {

    }

    /// In the module initializer we create a single instance of the
    /// `TokenRegistry` and share it to make publicly available.
    fun init(ctx: &mut TxContext) {
        sui::transfer::share_object(TokenRegistry {
            id: object::new(ctx),
            channel: channel::create_channel(true, ctx)
        });
    }
}
