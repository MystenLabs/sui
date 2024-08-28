// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module examples::testnet_nft {
    use std::string::String;
    use sui::event;

    /// An example NFT that can be minted by anybody
    public struct TestnetNFT has key, store {
        id: UID,
        /// Name for the token
        name: String,
        /// Description of the token
        description: String,
        /// URL for the token
        url: String,
    }

    // ===== Events =====

    public struct NFTMinted has copy, drop {
        // The Object ID of the NFT
        object_id: ID,
        // The creator of the NFT
        creator: address,
        // The name of the NFT
        name: String,
    }

    // ===== Public view functions =====

    /// Get the NFT's `name`
    public fun name(nft: &TestnetNFT): &String {
        &nft.name
    }

    /// Get the NFT's `description`
    public fun description(nft: &TestnetNFT): &String {
        &nft.description
    }

    /// Get the NFT's `url`
    public fun url(nft: &TestnetNFT): &String {
        &nft.url
    }

    // ===== Entrypoints =====

    #[allow(lint(self_transfer))]
    /// Create a new devnet_nft
    public fun mint_to_sender(
        name: String,
        description: String,
        url: String,
        ctx: &mut TxContext
    ) {
        let sender = ctx.sender();
        let nft = TestnetNFT {
            id: object::new(ctx),
            name,
            description,
            url
        };

        event::emit(NFTMinted {
            object_id: object::id(&nft),
            creator: sender,
            name: nft.name,
        });

        transfer::public_transfer(nft, sender);
    }

    /// Transfer `nft` to `recipient`
    public fun transfer(
        nft: TestnetNFT, recipient: address, _: &mut TxContext
    ) {
        transfer::public_transfer(nft, recipient)
    }

    /// Update the `description` of `nft` to `new_description`
    public fun update_description(
        nft: &mut TestnetNFT,
        new_description: String,
        _: &mut TxContext
    ) {
        nft.description = new_description;
    }

    /// Permanently delete `nft`
    public fun burn(nft: TestnetNFT, _: &mut TxContext) {
        let TestnetNFT { id, name: _, description: _, url: _ } = nft;
        id.delete()
    }
}
