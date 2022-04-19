// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A minimalist example to demonstrate how to create an NFT like object
/// on Sui. The user should be able to use the wallet command line tool
/// (https://docs.sui.io/build/wallet) to mint an NFT. For example,
/// `wallet example-nft --name <Name> --description <Description> --url <URL>`
module Sui::ExampleNFT {
    use Sui::Url::{Self, Url};
    use Sui::UTF8;
    use Sui::ID::VersionedID;
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};

    /// An example NFT that can be minted by anybody
    struct ExampleNFT has key, store {
        id: VersionedID,
        /// Name for the token
        name: UTF8::String,
        /// Description of the token
        description: UTF8::String,
        /// URL for the token
        url: Url,
        // TODO: allow custom attributes
    }

    /// Create a new ExampleNFT
    public(script) fun mint(
        name: vector<u8>,
        description: vector<u8>,
        url: vector<u8>,
        ctx: &mut TxContext
    ) {
        let nft = ExampleNFT {
            id: TxContext::new_id(ctx),
            name: UTF8::string_unsafe(name),
            description: UTF8::string_unsafe(description),
            url: Url::new_from_bytes_unsafe(url)
        };
        Transfer::transfer(nft, TxContext::sender(ctx))
    }
}
