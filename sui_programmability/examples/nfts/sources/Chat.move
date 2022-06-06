// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module NFTs::Chat {
    use Std::ASCII::{Self, String};
    use Std::Option::{Self, Option, some};
    use Sui::ID::{Self, ID, VersionedID};
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};
    use Std::Vector::length;

    /// Max text length.
    const MAX_TEXT_LENGTH: u64 = 512;

    /// Text size overflow.
    const ETextOverflow: u64 = 0;

    /// Sui Chat NFT (i.e., a post, retweet, like, chat message etc).
    struct Chat has key, store {
        id: VersionedID,
        // The ID of the chat app.
        app_id: ID,
        // Post's text.
        text: String,
        // Set if referencing an another object (i.e., due to a Like, Retweet, Reply etc).
        // We allow referencing any object type, not ony Chat NFTs.
        ref_id: Option<ID>,
        // app-specific metadata. We do not enforce a metadata format and delegate this to app layer.
        metadata: vector<u8>,
    }

    /// Simple Chat.text getter.
    public fun text(chat: &Chat): String {
        chat.text
    }

    /// Mint (post) a Chat object.
    fun post_internal(
        app_id: ID,
        text: vector<u8>,
        ref_id: Option<ID>,
        metadata: vector<u8>,
        ctx: &mut TxContext,
    ) {
        assert!(length(&text) <= MAX_TEXT_LENGTH, ETextOverflow);
        let chat = Chat {
            id: TxContext::new_id(ctx),
            app_id,
            text: ASCII::string(text),
            ref_id,
            metadata,
        };
        Transfer::transfer(chat, TxContext::sender(ctx));
    }

    /// Mint (post) a Chat object without referencing another object.
    public(script) fun post(
        app_identifier: address,
        text: vector<u8>,
        metadata: vector<u8>,
        ctx: &mut TxContext,
    ) {
        post_internal(ID::new(app_identifier), text, Option::none(), metadata, ctx);
    }

    /// Mint (post) a Chat object and reference another object (i.e., to simulate retweet, reply, like, attach).
    /// TODO: Using `address` as `app_identifier` & `ref_identifier` type, because we cannot pass `ID` to entry
    ///     functions. Using `vector<u8>` for `text` instead of `String`  for the same reason.
    public(script) fun post_with_ref(
        app_identifier: address,
        text: vector<u8>,
        ref_identifier: address,
        metadata: vector<u8>,
        ctx: &mut TxContext,
    ) {
        post_internal(ID::new(app_identifier), text, some(ID::new(ref_identifier)), metadata, ctx);
    }

    /// Burn a Chat object.
    public(script) fun burn(chat: Chat) {
        let Chat { id, app_id: _, text: _, ref_id: _, metadata: _ } = chat;
        ID::delete(id);
    }
}
