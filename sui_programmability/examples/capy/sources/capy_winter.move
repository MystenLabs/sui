// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This module enables Capy Winter Event - the result of a
/// unique collaboration between Capy Labs and Capy Post.
///
/// Once a year, two giants of the Capy world unite their
/// forces to deliver the best Winter Holiday experience to
/// support kindness, generosity and the holiday mood.
///
/// Capy Post takes zero commission for gift parcels.
module capy::capy_winter {
    use sui::sui::SUI;
    use sui::coin::{Self, Coin};
    use sui::object::{Self, ID, UID};
    use sui::balance::{Self, Balance};
    use sui::transfer;
    use sui::tx_context::{TxContext, sender};
    use std::hash::sha3_256 as hash;
    use sui::dynamic_field as df;
    use sui::url::{Self, Url};
    use sui::event::emit;
    use std::vector as vec;
    use sui::pay;
    use sui::bcs;

    use capy::capy::{Self, Attribute, CapyRegistry};

    /// The name for custom attributes.
    const ATTRIBUTE_NAME: vector<u8> = b"special";

    /// Custom attributes assigned randomly when a box is opened.
    const ATTRIBUTE_VALUES: vector<vector<u8>> = vector[
        b"snow globe",
        b"antlers",
        b"garland",
        b"beard",
    ];

    /// Value for the premium attribute.
    const PREMIUM_ATTRIBUTE: vector<u8> = b"winter environment";

    /// Total number of different GiftBoxes available for
    /// sale from the CapyPost.
    const GIFT_TYPES: u8 = 8;

    /// A single price for every GiftBox available this year.
    const GIFT_PRICE: u64 = 2023_0000;

    /// Position of the '0' symbol in ASCII
    const ASCII_OFFSET: u8 = 48;

    /// A gift box; what's inside?
    struct GiftBox has key {
        id: UID,
        type: u8,
        url: Url,
        link: Url,
    }

    /// A ticket granting the permission to buy a premium box.
    struct PremiumTicket has key { id: UID }

    /// A Premium box - can only be purchased by the most genereous givers.
    struct PremiumBox has key {
        id: UID,
        url: Url,
    }

    /// Every parcel must go through here!
    struct CapyPost has key { id: UID, balance: Balance<SUI> }

    // ========= Events =========

    /// Emitted when a box was purchased of a gift box.
    struct GiftPurchased has copy, drop { id: ID, type: u8 }

    /// Emitted when a gift has been sent
    struct GiftSent has copy, drop { id: ID }

    /// Emitted when a gift was opened!
    struct GiftOpened has copy, drop { id: ID }

    /// Emitted when a premium gift was received.
    struct PremiumTicketReceived has copy, drop { id: ID }

    /// Emitted when a premium ticket was used.
    struct PremiumTicketUsed has copy, drop { id: ID }

    /// Emitted when a premium box was purchased.
    struct PremiumPurchased has copy, drop { id: ID }

    /// Emitted when a premium gift was opened.
    struct PremiumOpened has copy, drop { id: ID }

    // ========= Dynamic Parameters Keys =========

    struct SentKey has store, copy, drop { sender: address }
    struct OpenedKey has store, copy, drop { sender: address }

    #[allow(unused_function)]
    /// Build a CapyPost office and offer gifts to send and buy.
    fun init(ctx: &mut TxContext) {
        transfer::share_object(CapyPost { id: object::new(ctx), balance: balance::zero() });
    }

    /// Buy a single `GiftBox` and keep it at the sender's address.
    entry fun buy_gift(post: &mut CapyPost, type: u8, payment: vector<Coin<SUI>>, ctx: &mut TxContext) {
        assert!(type < GIFT_TYPES, 0);

        let (paid, remainder) = merge_and_split(payment, GIFT_PRICE, ctx);
        coin::put(&mut post.balance, paid);
        let id = object::new(ctx);
        let url = get_img_url(type);
        let link = get_link_url(&id, type);

        emit(GiftPurchased { id: object::uid_to_inner(&id), type });
        transfer::transfer(GiftBox { id, type, url, link }, sender(ctx));
        transfer::public_transfer(remainder, sender(ctx))
    }

    /// Send a GiftBox to a friend or a stranger through CapyPost.
    /// Kindness and generosity will be rewarded!
    entry fun send_gift(post: &mut CapyPost, box: GiftBox, receiver: address, ctx: &mut TxContext) {
        let sender = sender(ctx);

        // Can't send gifts to yourself...
        assert!(receiver != sender, 0);

        // If there's already a gift-tracking field, we increment the counter;
        // Once it reaches 2 (the third send), we reset the counter and send a PremiumBox;
        let sent = if (df::exists_with_type<SentKey, u8>(&post.id, SentKey { sender })) {
            let sent = df::remove(&mut post.id, SentKey { sender });
            if (sent == 1) {
                let id = object::new(ctx);
                emit(PremiumTicketReceived { id: object::uid_to_inner(&id) });
                transfer::transfer(PremiumTicket { id }, sender(ctx));
                0
            } else { sent + 1 }
        } else { 0 };

        // update the counter with the resulting value
        df::add<SentKey, u8>(&mut post.id, SentKey { sender }, sent);

        emit(GiftSent { id: object::id(&box) });
        transfer::transfer(box, receiver)
    }

    /// Open a box and expect a surprise!
    entry fun open_box(reg: &mut CapyRegistry, box: GiftBox, ctx: &mut TxContext) {
        let GiftBox { id, type: _, url: _, link: _ } = box;
        let sequence = std::hash::sha3_256(object::uid_to_bytes(&id));
        let attribute = get_attribute(&sequence);

        emit(GiftOpened { id: object::uid_to_inner(&id) });
        transfer::public_transfer(capy::create_capy(reg, sequence, vector[ attribute ], ctx), sender(ctx));
        object::delete(id)
    }

    /// Buy a premium box using a ticket!
    entry fun buy_premium(
        post: &mut CapyPost, ticket: PremiumTicket, payment: vector<Coin<SUI>>, ctx: &mut TxContext
    ) {
        let PremiumTicket { id: ticket_id } = ticket;
        let (paid, remainder) = merge_and_split(payment, GIFT_PRICE, ctx);
        coin::put(&mut post.balance, paid);
        let id = object::new(ctx);

        emit(PremiumPurchased { id: object::uid_to_inner(&id) });
        transfer::transfer(PremiumBox { id, url: get_img_url(99) }, sender(ctx));
        transfer::public_transfer(remainder, sender(ctx));
        object::delete(ticket_id)
    }

    /// Open a premium box!
    entry fun open_premium(reg: &mut CapyRegistry, box: PremiumBox, ctx: &mut TxContext) {
        let PremiumBox { id, url: _ } = box;
        let sequence = std::hash::sha3_256(object::uid_to_bytes(&id));
        let premium = capy::create_attribute(ATTRIBUTE_NAME, PREMIUM_ATTRIBUTE);

        emit(PremiumOpened { id: object::uid_to_inner(&id) });
        transfer::public_transfer(capy::create_capy(reg, sequence, vector[ premium ], ctx), sender(ctx));
        object::delete(id)
    }

    /// Merges a vector of Coin then splits the `amount` from it, returns the
    /// Coin with the amount and the remainder.
    fun merge_and_split(
        coins: vector<Coin<SUI>>, amount: u64, ctx: &mut TxContext
    ): (Coin<SUI>, Coin<SUI>) {
        let base = vec::pop_back(&mut coins);
        pay::join_vec(&mut base, coins);
        assert!(coin::value(&base) > amount, 0);
        (coin::split(&mut base, amount, ctx), base)
    }

    /// Get a 'random' attribute based on a seed.
    ///
    /// For fun and exploration we get the number from the BCS bytes.
    /// This function demonstrates the way of getting a `u64` number
    /// from a vector of bytes.
    fun get_attribute(seed: &vector<u8>): Attribute {
        let bcs_bytes = bcs::new(hash(*seed));
        let attr_idx = bcs::peel_u64(&mut bcs_bytes) % vec::length(&ATTRIBUTE_VALUES); // get the index of the attribute
        let attr_value = *vec::borrow(&ATTRIBUTE_VALUES, attr_idx);

        capy::create_attribute(ATTRIBUTE_NAME, attr_value)
    }

    /// Get a URL for the box image.
    /// TODO: specify capy.art here!!!
    fun get_img_url(type: u8): Url {
        let res = b"http://api.capy.art/box_";
        if (type == 99) {
            vec::append(&mut res, b"premium");
        } else {
            vec::push_back(&mut res, ASCII_OFFSET + type);
        };

        vec::append(&mut res, b".svg");

        url::new_unsafe_from_bytes(res)
    }

    /// Get a link to the gift on the capy.art.
    fun get_link_url(id: &UID, type: u8): Url {
        let res = b"http://capy.art/gifts/";
        vec::append(&mut res, sui::hex::encode(object::uid_to_bytes(id)));
        vec::append(&mut res, b"?type=");
        vec::push_back(&mut res, ASCII_OFFSET + type);

        url::new_unsafe_from_bytes(res)
    }
}
