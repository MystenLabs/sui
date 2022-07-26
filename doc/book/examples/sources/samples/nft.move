// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module examples::devnet_nft {
    use sui::url::{Self, Url};
    use sui::utf8;
    use sui::object::{Self, ID, Info};
    use sui::event;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    /// An example NFT that can be minted by anybody
    struct DevNetNFT has key, store {
        info: Info,
        /// Name for the token
        name: utf8::String,
        /// Description of the token
        description: utf8::String,
        /// URL for the token
        url: Url,
        // TODO: allow custom attributes
    }

    // ===== Events =====

    struct NFTMinted has copy, drop {
        // The Object ID of the NFT
        object_id: ID,
        // The creator of the NFT
        creator: address,
        // The name of the NFT
        name: utf8::String,
    }

    // ===== Public view functions =====

    /// Get the NFT's `name`
    public fun name(nft: &DevNetNFT): &utf8::String {
        &nft.name
    }

    /// Get the NFT's `description`
    public fun description(nft: &DevNetNFT): &utf8::String {
        &nft.description
    }

    /// Get the NFT's `url`
    public fun url(nft: &DevNetNFT): &Url {
        &nft.url
    }

    // ===== Entrypoints =====

    /// Create a new devnet_nft
    public entry fun mint_to_sender(
        name: vector<u8>,
        description: vector<u8>,
        url: vector<u8>,
        ctx: &mut TxContext
    ) {
        let sender = tx_context::sender(ctx);
        let nft = DevNetNFT {
            info: object::new(ctx),
            name: utf8::string_unsafe(name),
            description: utf8::string_unsafe(description),
            url: url::new_unsafe_from_bytes(url)
        };

        event::emit(NFTMinted {
            object_id: *object::info_id(&nft.info),
            creator: sender,
            name: nft.name,
        });

        transfer::transfer(nft, sender);
    }

    /// Transfer `nft` to `recipient`
    public entry fun transfer(
        nft: DevNetNFT, recipient: address, _: &mut TxContext
    ) {
        transfer::transfer(nft, recipient)
    }

    /// Update the `description` of `nft` to `new_description`
    public entry fun update_description(
        nft: &mut DevNetNFT,
        new_description: vector<u8>,
        _: &mut TxContext
    ) {
        nft.description = utf8::string_unsafe(new_description)
    }

    /// Permanently delete `nft`
    public entry fun burn(nft: DevNetNFT, _: &mut TxContext) {
        let DevNetNFT { info, name: _, description: _, url: _ } = nft;
        object::delete(info)
    }
}
