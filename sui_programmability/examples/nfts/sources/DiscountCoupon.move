// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module NFTs::DiscountCoupon {
    use Sui::Coin;
    use Sui::NFT::{Self, NFT};
    use Sui::SUI::SUI;
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};

    /// Sending to wrong recipient.
    const EWRONG_RECIPIENT: u64 = 0;

    /// Percentage discount out of range.
    const EOUT_OF_RANGE_DISCOUNT: u64 = 1;

    /// Discount coupon NFT.
    struct DiscountCoupon has store {
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
    public fun mint_and_topup(
        coin: Coin::Coin<SUI>, 
        discount: u8,
        expiration: u64,
        recipient: address,
        ctx: &mut TxContext,
    ) {
        assert!(discount > 0 && discount <= 100, EOUT_OF_RANGE_DISCOUNT);
        let nft = NFT::mint(
                DiscountCoupon { 
                    issuer: TxContext::sender(ctx),
                    discount, 
                    expiration,
                }, 
                ctx);
        Transfer::transfer(nft, recipient);
        Sui::SUI::transfer(coin, recipient, ctx);
    }

    /// Burn DiscountCoupon.
    public fun burn(nft: NFT<DiscountCoupon>, _ctx: &mut TxContext) {
        let DiscountCoupon { issuer: _, discount: _, expiration: _ } = NFT::burn(nft);
    }

    /// Transfer DiscountCoupon to issuer only.
    //  TODO: Consider adding more valid recipients. 
    //      If we stick with issuer-as-receiver only, then `recipient` input won't be required).
    public fun transfer(nft: NFT<DiscountCoupon>, recipient: address, _ctx: &mut TxContext) {
        assert!(NFT::data(&nft).issuer == recipient, EWRONG_RECIPIENT);
        NFT::transfer(nft, recipient)
    }
}
