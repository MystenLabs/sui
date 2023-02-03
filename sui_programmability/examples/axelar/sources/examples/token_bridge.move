// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A token bridge implementation example.
/// Allows anyone to create a bridge for a unique symbol by publishing a module.
///
/// Guarantees:
///  - there can only be 1 bridge per symbol ("eth" or "btc" or any other)
///  - users can't cheat on the system and create multiple `Supply`'s or `TreasuryCap`'s
///  - supply is empty by default unless some amount is first transferred to Sui
///
/// Message format:
///  - each message must target `Bridge`'s `Channel`
///  - message symbol must match the symbol of the `Bridge`
///  - fields of the incoming messages are: ( amount, symbol, receiver )
///  - fields of the outcoming messages are identical to the incoming
///
/// Flow:
///  1. user publishes a package and requests for a `CreateBridgeCap` in the
///  initializer (see `eth` example).
///
///  2. with this capability, the method `create_bridge` can be called; it makes
///  sure that the symbol is unique in the system and then creates a shared object
///  `Bridge`.
///
///  3. from this moment messages can be received by a specific Bridge, and the
///  receiver specified in the message will receive a Coin once a message is processed
///
/// Potential improvements:
///  - this solution implies decentralization - each Bridge is a separate Channel
///  and only controls a single Coin. It could be changed to a more centralized
///  architecture - a single Channel and a single shared object for all coins.
///
///  - the flow with requesting a Capability first is a requirement to bypass
///  current limitations. This step could be removed if module initializer supported
///  external arguments.
///
///  - due to `balance::Supply` not being destructable; if someone creates a bridge
///  with a symbol that was already registered, they won't be able to destroy their
///  `CreateBridgeCap` - this could either be improved by allowing Supply unpacking in
///  the sui framework or allowing `store` ability on the OTW in the Sui Verifier.
///
module axelar::token_bridge {
    use sui::object::{Self, UID, ID};
    use sui::balance::{Self, Supply};
    use sui::transfer::{share_object, transfer};
    use sui::tx_context::{TxContext};
    use sui::dynamic_field as df;
    use sui::coin::{Self, Coin};
    use sui::bcs;

    use std::ascii::{Self, String};
    use std::type_name;

    use axelar::messenger::{Self, Axelar, Channel};

    /// For when a witness passed is not an OTW.
    const ENotOTW: u64 = 0;
    /// For when message symbol mismatches the Bridge.
    const ESymbolMismatch: u64 = 0;

    /// Centralized token registry. Makes sure there's a single
    /// bridge object for a symbol.
    struct TokenRegistry has key {
        id: UID
    }

    /// A Capability enabling a bridge creation request. Currently
    /// required, because there's no way to pass `TokenRegistry` as
    /// an argument to the module initializer nor there's a way to
    /// store an OTW.
    struct CreateBridgeCap<phantom T: drop> has key, store {
        id: UID,
        /// Symbol for the Coin / Token. Read from the type of the T.
        /// For now (and for simplicity's sake) - lowercased.
        symbol: String,
        /// Supply for the future token T. Unfortunately, can not
        /// be destroyed (even empty) yet. That's a possible flow that
        /// could be changed in the `sui::balance` module.
        supply: Supply<T>
    }

    /// A single Token bridge. Controls minting and burning of new `Coin<T>`'s.
    /// Is guaranteed to be unique in the system for its `symbol` and its `T`.
    struct Bridge<phantom T: drop> has key {
        id: UID,
        /// Symbol kept for visibility and readability off-chain.
        symbol: String,
        /// Total supply of the T on the network. Manages minting and burning
        /// Coins on Sui.
        supply: Supply<T>,
        /// Channel for the Bridge - messages to mint a Coin need to be targeted at
        /// this channel. Also used when sending `burn` events to the Axelar chain.
        /// Contains a boolean value as a placeholder for missing T in the Channel.
        channel: Channel<bool>
    }

    /// Coin sent data for faster BCS serialization.
    struct CoinSent has drop {
        amount: u64,
        symbol: String,
        receiver: vector<u8>
    }

    /// In the module initializer we create a single `TokenRegistry`.
    fun init(ctx: &mut TxContext) {
        share_object(TokenRegistry {
            id: object::new(ctx)
        })
    }

    /// Bridge creation requires an OTW - can only be called in a bridge module initializer.
    /// Additionally we only count the name of the T as the token symbol.
    public fun get_bridge_creation_cap<T: drop>(otw: T, ctx: &mut TxContext): CreateBridgeCap<T> {
        assert!(sui::types::is_one_time_witness(&otw), ENotOTW);

        // lowercase name of the module; due to OTW having the name of
        // the module + uppercase and since we check for an OTW above,
        // this way of getting the symbol can be consirered valid.
        let symbol = type_name::get_module(&type_name::get<T>());
        let supply = balance::create_supply(otw);

        CreateBridgeCap { id: object::new(ctx), symbol, supply }
    }

    /// Create a bridge using a `CreateBridgeCap` (previously acquired through
    /// module publishing).
    ///
    /// TODO (DevX):
    ///  does not check if a key exists and aborts with `df::EFieldAlreadyExists` if
    ///  it does; add a custom check + custom abort code for the scenario
    public entry fun create_bridge<T: drop>(
        registry: &mut TokenRegistry,
        cap: CreateBridgeCap<T>,
        ctx: &mut TxContext
    ) {
        let CreateBridgeCap { id: cap_id, symbol, supply } = cap;
        let bridge_id = object::new(ctx);

        df::add<String, ID>(&mut registry.id, symbol, object::uid_to_inner(&bridge_id));
        object::delete(cap_id);

        share_object(Bridge {
            id: bridge_id,
            supply,
            symbol,
            channel: messenger::create_channel(true, ctx)
        })
    }

    /// Process a mint message from the Axelar chain.
    ///
    /// If a message was targeted to this channel's bridge and contains the
    /// correct symbol (matches the `Bridge`), mint some `Coin<T>` based on
    /// the message data (custom payload):
    ///  - amount
    ///  - symbol
    ///  - receiver (20 bytes in Sui)
    public entry fun process_mint_message<T: drop>(
        axelar: &mut Axelar,
        bridge: &mut Bridge<T>,
        msg_id: vector<u8>,
        ctx: &mut TxContext
    ) {
        let (
            _channel_data,
            _source_chain,
            _source_address,
            _payload_hash,
            payload
        ) = messenger::consume_message(axelar, &mut bridge.channel, msg_id);

        let bcs = bcs::new(payload);
        let (amount, symbol, receiver) = (
            bcs::peel_u64(&mut bcs),
            bcs::peel_vec_u8(&mut bcs),
            bcs::peel_address(&mut bcs)
        );

        assert!(&symbol == ascii::as_bytes(&bridge.symbol), ESymbolMismatch);

        let balance = balance::increase_supply(&mut bridge.supply, amount);
        let coin = coin::from_balance(balance, ctx);

        transfer(coin, receiver)
    }

    /// Send the Coin<T> from this `Bridge` to some network X.
    ///
    /// Effectively burns the Coin and emits a message authorized by a bridge Channel.
    /// The message cointains: `amount`, `symbol` and `receiver` (as vector<u8>).
    public entry fun send_coin<T: drop>(
        bridge: &mut Bridge<T>,
        coin: Coin<T>,
        destination: vector<u8>,
        destination_address: vector<u8>,
        receiver: vector<u8>
    ) {
        let balance = coin::into_balance(coin);
        let amount = balance::decrease_supply(&mut bridge.supply, balance);

        messenger::send_message(
            &mut bridge.channel,
            destination,
            destination_address,
            bcs::to_bytes(&CoinSent {
                amount,
                symbol: bridge.symbol,
                receiver
            })
        )
    }
}

/// An example module for adding an ETH bridge on Sui.
/// The module name will be used a symbol in the bridge.
module axelar::eth {
    use sui::tx_context::{TxContext, sender};
    use sui::transfer::transfer;
    use axelar::token_bridge;

    /// The type for the bridge.
    struct ETH has drop {}

    /// Create an OTW (ETH) in the initializer and get a Cap
    /// which then can be used to create an actual bridge.
    fun init(eth: ETH, ctx: &mut TxContext) {
        let cap = token_bridge::get_bridge_creation_cap(eth, ctx);
        transfer(cap, sender(ctx))
    }

    // Second transaction after publishing this module will be:
    // sui client call \
    //      --package axelar \
    //      --module token_bridge \
    //      --function create_bridge \
    //      --args \
    //          <token_registry> \
    //          <bridge_cap_id>
}
