module sui_nft::digital_collectible {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use std::string::{Self, String};
    use sui::event;

    /// The NFT struct representing a digital collectible
    struct DigitalCollectible has key, store {
        id: UID,
        name: String,
        description: String,
        url: String,
        creator: address,
    }

    /// Event emitted when a new NFT is minted
    struct NFTMinted has copy, drop {
        nft_id: address,
        creator: address,
        name: String,
    }

    /// Event emitted when an NFT is transferred
    struct NFTTransferred has copy, drop {
        nft_id: address,
        from: address,
        to: address,
    }

    /// Mint a new Digital Collectible NFT
    public entry fun mint_nft(
        name: vector<u8>,
        description: vector<u8>,
        url: vector<u8>,
        ctx: &mut TxContext
    ) {
        let sender = tx_context::sender(ctx);
        let nft = DigitalCollectible {
            id: object::new(ctx),
            name: string::utf8(name),
            description: string::utf8(description),
            url: string::utf8(url),
            creator: sender,
        };

        let nft_id = object::uid_to_address(&nft.id);

        event::emit(NFTMinted {
            nft_id,
            creator: sender,
            name: string::utf8(name),
        });

        transfer::public_transfer(nft, sender);
    }

    /// Transfer an NFT to another address
    public entry fun transfer_nft(
        nft: DigitalCollectible,
        recipient: address,
        ctx: &mut TxContext
    ) {
        let sender = tx_context::sender(ctx);
        let nft_id = object::uid_to_address(&nft.id);

        event::emit(NFTTransferred {
            nft_id,
            from: sender,
            to: recipient,
        });

        transfer::public_transfer(nft, recipient);
    }

    /// Update the description of an NFT (only by creator)
    public entry fun update_description(
        nft: &mut DigitalCollectible,
        new_description: vector<u8>,
        ctx: &TxContext
    ) {
        assert!(nft.creator == tx_context::sender(ctx), 0);
        nft.description = string::utf8(new_description);
    }

    /// Burn/delete an NFT
    public entry fun burn_nft(
        nft: DigitalCollectible,
        _ctx: &TxContext
    ) {
        let DigitalCollectible { id, name: _, description: _, url: _, creator: _ } = nft;
        object::delete(id);
    }

    // === Getter Functions ===

    public fun name(nft: &DigitalCollectible): &String {
        &nft.name
    }

    public fun description(nft: &DigitalCollectible): &String {
        &nft.description
    }

    public fun url(nft: &DigitalCollectible): &String {
        &nft.url
    }

    public fun creator(nft: &DigitalCollectible): address {
        nft.creator
    }

    #[test]
    fun test_mint_nft() {
        use sui::test_scenario;

        let creator = @0xCAFE;
        let scenario_val = test_scenario::begin(creator);
        let scenario = &mut scenario_val;

        test_scenario::next_tx(scenario, creator);
        {
            mint_nft(
                b"My NFT",
                b"A beautiful digital collectible",
                b"https://example.com/nft.png",
                test_scenario::ctx(scenario)
            );
        };

        test_scenario::next_tx(scenario, creator);
        {
            let nft = test_scenario::take_from_sender<DigitalCollectible>(scenario);
            assert!(name(&nft) == &string::utf8(b"My NFT"), 0);
            test_scenario::return_to_sender(scenario, nft);
        };

        test_scenario::end(scenario_val);
    }
}
