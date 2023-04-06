// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module nfts::discount_coupon {
    use sui::coin;
    use sui::object::{Self, UID};
    use sui::sui::SUI;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    /// Sending to wrong recipient.
    const EWrongRecipient: u64 = 0;

    /// Percentage discount out of range.
    const EOutOfRangeDiscount: u64 = 1;

    /// Discount coupon NFT.
    struct DiscountCoupon has key {
        id: UID,
        // coupon issuer
        issuer: address,
        // percentage discount [1-100]
        discount: u8,
        // expiration timestamp (UNIX time) - app specific
        expiration: u64,
    }

    /// Simple issuer getter.
    public fun issuer(coupon: &DiscountCoupon): address {
        coupon.issuer
    }

    /// Mint then transfer a new `DiscountCoupon` NFT, and top up recipient with some SUI.
    public entry fun mint_and_topup(
        coin: coin::Coin<SUI>,
        discount: u8,
        expiration: u64,
        recipient: address,
        ctx: &mut TxContext,
    ) {
        assert!(discount > 0 && discount <= 100, EOutOfRangeDiscount);
        let coupon = DiscountCoupon {
            id: object::new(ctx),
            issuer: tx_context::sender(ctx),
            discount,
            expiration,
        };
        transfer::transfer(coupon, recipient);
        transfer::public_transfer(coin, recipient);
    }

    /// Burn DiscountCoupon.
    public entry fun burn(nft: DiscountCoupon) {
        let DiscountCoupon { id, issuer: _, discount: _, expiration: _ } = nft;
        object::delete(id);
    }

    /// Transfer DiscountCoupon to issuer only.
    //  TODO: Consider adding more valid recipients.
    //      If we stick with issuer-as-receiver only, then `recipient` input won't be required).
    public entry fun transfer(coupon: DiscountCoupon, recipient: address) {
        assert!(&coupon.issuer == &recipient, EWrongRecipient);
        transfer::transfer(coupon, recipient);
    }
}
