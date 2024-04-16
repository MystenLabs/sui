// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module nfts::mint_nft {
    public struct NFT has key {
        id: UID,
        // mimic NFT's of arbitrary size
        contents: vector<u8>,
    }

    /// Create one NFT, send it to `recipient`
    public fun mint_one(recipient: address, contents: vector<u8>, ctx: &mut TxContext) {
        let nft = NFT { id: object::new(ctx), contents };
        transfer::transfer(nft, recipient)
    }

    /// Create one NFT, send it to each of the `recipients`
    public fun batch_mint(recipients: vector<address>, contents: vector<u8>, ctx: &mut TxContext) {
        let mut i = 0;
        let len = recipients.length();
        while (i < len) {
            let nft = NFT { id: object::new(ctx), contents };
            transfer::transfer(nft, recipients[i]);
            i = i + 1
        }
    }

    #[test]
    fun test_mint_one() {
        let mut ctx = tx_context::dummy();
        let addr1 = @0xA;

        mint_one(addr1, b"test", &mut ctx);
    }

    #[test]
    fun test_batch_mint() {
        let mut ctx = tx_context::dummy();
        let addr1 = @0xA;
        let addr2 = @0xB;
        let addrs = vector<address>[addr1, addr2];

        batch_mint(addrs, b"test", &mut ctx);
    }
}