// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module nfts::chat {
    use std::ascii::{Self, String};
    use std::option::{Self, Option, some};
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use std::vector::length;

    /// Max text length.
    const MAX_TEXT_LENGTH: u64 = 512;

    /// Text size overflow.
    const ETextOverflow: u64 = 0;

    /// Sui Chat NFT (i.e., a post, retweet, like, chat message etc).
    struct Chat has key, store {
        id: UID,
        // The ID of the chat app.
        app_id: address,
        // Post's text.
        text: String,
        // Set if referencing an another object (i.e., due to a Like, Retweet, Reply etc).
        // We allow referencing any object type, not only Chat NFTs.
        ref_id: Option<address>,
        // app-specific metadata. We do not enforce a metadata format and delegate this to app layer.
        metadata: vector<u8>,
    }

    /// Simple Chat.text getter.
    public fun text(chat: &Chat): String {
        chat.text
    }

    /// Mint (post) a Chat object.
    fun post_internal(
        app_id: address,
        text: vector<u8>,
        ref_id: Option<address>,
        metadata: vector<u8>,
        ctx: &mut TxContext,
    ) {
        assert!(length(&text) <= MAX_TEXT_LENGTH, ETextOverflow);
        let chat = Chat {
            id: object::new(ctx),
            app_id,
            text: ascii::string(text),
            ref_id,
            metadata,
        };
        transfer::public_transfer(chat, tx_context::sender(ctx));
    }

    /// Mint (post) a Chat object without referencing another object.
    public entry fun post(
        app_identifier: address,
        text: vector<u8>,
        metadata: vector<u8>,
        ctx: &mut TxContext,
    ) {
        post_internal(app_identifier, text, option::none(), metadata, ctx);
    }

    /// Mint (post) a Chat object and reference another object (i.e., to simulate retweet, reply, like, attach).
    /// TODO: Using `address` as `app_identifier` & `ref_identifier` type, because we cannot pass `ID` to entry
    ///     functions. Using `vector<u8>` for `text` instead of `String`  for the same reason.
    public entry fun post_with_ref(
        app_identifier: address,
        text: vector<u8>,
        ref_identifier: address,
        metadata: vector<u8>,
        ctx: &mut TxContext,
    ) {
        post_internal(app_identifier, text, some(ref_identifier), metadata, ctx);
    }

    /// Burn a Chat object.
    public entry fun burn(chat: Chat) {
        let Chat { id, app_id: _, text: _, ref_id: _, metadata: _ } = chat;
        object::delete(id);
    }
}
