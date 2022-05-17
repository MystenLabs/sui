// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module NFTs::Chat {
    use Sui::ID::{Self, ID, VersionedID};
    use Std::ASCII::{Self, String};
    use Std::Option::Option;
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};

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
        // app-specific metadata.
        metadata: vector<u8>,
    }

    /// Simple Chat.text getter.
    public fun text(chat: &Chat): String {
        chat.text
    }

    /// Mint (post) a Chat object.
    public(script) fun mint(
            app_id: ID,
            text: String,
            ref_id: Option<ID>,
            metadata: vector<u8>,
            ctx: &mut TxContext,
        ) {
        assert!(ASCII::length(&text) <= MAX_TEXT_LENGTH, ETextOverflow);
        let sender = TxContext::sender(ctx);
        let chat = Chat {
            id: TxContext::new_id(ctx),
            app_id,
            text,
            ref_id,
            metadata,
        };
        Transfer::transfer(chat, sender);
    }

    /// Burn a Chat object.
    public(script) fun burn(chat: Chat, _ctx: &mut TxContext) {
        let Chat { id, app_id: _, text: _, ref_id: _, metadata: _ } = chat;
        ID::delete(id);
    }
}
