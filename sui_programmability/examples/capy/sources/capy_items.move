// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Items for Capys.
/// Every capy can have up to X items at the same time.
module capy::capy_items {
    use sui::event::emit;
    use sui::url::{Self, Url};
    use sui::object::{Self, UID, ID};
    use sui::tx_context::TxContext;
    use std::string::{Self, String};
    use std::vector as vec;

    use capy::hex;
    use capy::capy::CapyManagerCap;

    /// Base path for `CapyItem.url` attribute. Is temporary and improves
    /// explorer / wallet display. Always points to the dev/testnet server.
    const IMAGE_URL: vector<u8> = b"https://api.capy.art/items/";

    /// Need a way to submit actual items.
    /// Perhaps, we could categorize them or change types.
    struct CapyItem has key, store {
        id: UID,
        url: Url,
        type: String,
        name: String,
    }

    /// Emitted when new item is created.
    struct ItemCreated has copy, drop {
        id: ID,
        type: vector<u8>,
        name: vector<u8>,
    }

    /// Create new item and send it to sender. Only available to Capy Admin.
    public entry fun create_and_take(
        cap: &CapyManagerCap,
        type: vector<u8>,
        name: vector<u8>,
        ctx: &mut TxContext
    ) {
        sui::transfer::transfer(
            create_item(cap, type, name, ctx),
            sui::tx_context::sender(ctx)
        );
    }

    /// Admin-only action - create an item. Ideally to place it later to the marketplace or send to someone.
    public fun create_item(
        _: &CapyManagerCap,
        type: vector<u8>,
        name: vector<u8>,
        ctx: &mut TxContext
    ): CapyItem {
        let id = object::new(ctx);
        let id_copy = object::uid_to_inner(&id);

        emit(ItemCreated { id: id_copy, type, name });

        CapyItem {
            url: img_url(&id),
            id,
            type: string::utf8(type),
            name: string::utf8(name)
        }
    }

    /// Construct an image URL for the `CapyItem`.
    fun img_url(c: &UID): Url {
        let capy_url = *&IMAGE_URL;
        vec::append(&mut capy_url, hex::to_hex(object::uid_to_bytes(c)));
        vec::append(&mut capy_url, b"/svg");

        url::new_unsafe_from_bytes(capy_url)
    }
}
