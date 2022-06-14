// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A minimalist example to demonstrate how to create an NFT like object
/// on Sui. The user should be able to use the wallet command line tool
/// (https://docs.sui.io/build/wallet) to mint an NFT. For example,
/// `wallet example-nft --name <Name> --description <Description> --url <URL>`
module Sui::DevNetNFT {
    use Sui::Url::{Self, Url};
    use Sui::UTF8;
    use Sui::ID::{Self, ID, VersionedID};
    use Sui::Event;
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};

    /// An example NFT that can be minted by anybody
    struct DevNetNFT has key, store {
        id: VersionedID,
        /// Name for the token
        name: UTF8::String,
        /// Description of the token
        description: UTF8::String,
        /// URL for the token
        url: Url,
        // TODO: allow custom attributes
    }

    struct MintNFTEvent has copy, drop {
        // The Object ID of the NFT
        object_id: ID,
        // The creator of the NFT
        creator: address,
        // The name of the NFT
        name: UTF8::String,
    }

    /// Create a new DevNetNFT
    public entry fun mint(
        name: vector<u8>,
        description: vector<u8>,
        url: vector<u8>,
        ctx: &mut TxContext
    ) {
        let nft = DevNetNFT {
            id: TxContext::new_id(ctx),
            name: UTF8::string_unsafe(name),
            description: UTF8::string_unsafe(description),
            url: Url::new_unsafe_from_bytes(url)
        };
        let sender = TxContext::sender(ctx);
        Event::emit(MintNFTEvent {
            object_id: *ID::inner(&nft.id),
            creator: sender,
            name: nft.name,
        });
        Transfer::transfer(nft, sender);
    }

    /// Transfer `nft` to `recipient`
    public entry fun transfer(
        nft: DevNetNFT, recipient: address, _: &mut TxContext
    ) {
        Transfer::transfer(nft, recipient)
    }

    /// Update the `description` of `nft` to `new_description`
    public entry fun update_description(
        nft: &mut DevNetNFT,
        new_description: vector<u8>,
        _: &mut TxContext
    ) {
        nft.description = UTF8::string_unsafe(new_description)
    }

    /// Permanently delete `nft`
    public entry fun burn(nft: DevNetNFT, _: &mut TxContext) {
        let DevNetNFT { id, name: _, description: _, url: _ } = nft;
        ID::delete(id)
    }

    /// Get the NFT's `name`
    public fun name(nft: &DevNetNFT): &UTF8::String {
        &nft.name
    }

    /// Get the NFT's `description`
    public fun description(nft: &DevNetNFT): &UTF8::String {
        &nft.description
    }

    /// Get the NFT's `url`
    public fun url(nft: &DevNetNFT): &Url {
        &nft.url
    }
}

#[test_only]
module Sui::DevNetNFTTests {
    use Sui::DevNetNFT::{Self, DevNetNFT};
    use Sui::TestScenario;
    use Sui::UTF8;

    #[test]
    public entry fun mint_transfer_update() {
        let addr1 = @0xA;
        let addr2 = @0xB;
        // create the NFT
        let scenario = TestScenario::begin(&addr1);
        {
            DevNetNFT::mint(b"test", b"a test", b"https://www.sui.io", TestScenario::ctx(&mut scenario))
        };
        // send it from A to B
        TestScenario::next_tx(&mut scenario, &addr1);
        {
            let nft = TestScenario::take_owned<DevNetNFT>(&mut scenario);
            DevNetNFT::transfer(nft, addr2, TestScenario::ctx(&mut scenario));
        };
        // update its description
        TestScenario::next_tx(&mut scenario, &addr2);
        {
            let nft = TestScenario::take_owned<DevNetNFT>(&mut scenario);
            DevNetNFT::update_description(&mut nft, b"a new description", TestScenario::ctx(&mut scenario)) ;
            assert!(*UTF8::bytes(DevNetNFT::description(&nft)) == b"a new description", 0);
            TestScenario::return_owned(&mut scenario, nft);
        };
        // burn it
        TestScenario::next_tx(&mut scenario, &addr2);
        {
            let nft = TestScenario::take_owned<DevNetNFT>(&mut scenario);
            DevNetNFT::burn(nft, TestScenario::ctx(&mut scenario))
        }
    }
}
