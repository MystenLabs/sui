// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module NFTs::DiscountCoupon {
    use Sui::Coin;
    use Sui::ID::{Self, VersionedID};
    use Sui::SUI::{Self, SUI};
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};

    /// Sending to wrong recipient.
    const EWrongRecipient: u64 = 0;

    /// Percentage discount out of range.
    const EOutOfRangeDiscount: u64 = 1;

    /// Discount coupon NFT.
    struct DiscountCoupon has key, store {
        id: VersionedID,
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
    public(script) fun mint_and_topup(
        coin: Coin::Coin<SUI>,
        discount: u8,
        expiration: u64,
        recipient: address,
        ctx: &mut TxContext,
    ) {
        assert!(discount > 0 && discount <= 100, EOutOfRangeDiscount);
        let coupon = DiscountCoupon {
            id: TxContext::new_id(ctx),
            issuer: TxContext::sender(ctx),
            discount,
            expiration,
        };
        Transfer::transfer(coupon, recipient);
        SUI::transfer(coin, recipient, ctx);
    }

    /// Burn DiscountCoupon.
    public(script) fun burn(nft: DiscountCoupon, _ctx: &mut TxContext) {
        let DiscountCoupon { id, issuer: _, discount: _, expiration: _ } = nft;
        ID::delete(id);
    }

    /// Transfer DiscountCoupon to issuer only.
    //  TODO: Consider adding more valid recipients.
    //      If we stick with issuer-as-receiver only, then `recipient` input won't be required).
    public(script) fun transfer(coupon: DiscountCoupon, recipient: address, _ctx: &mut TxContext) {
        assert!(&coupon.issuer == &recipient, EWrongRecipient);
        Transfer::transfer(coupon, recipient);
    }
}
