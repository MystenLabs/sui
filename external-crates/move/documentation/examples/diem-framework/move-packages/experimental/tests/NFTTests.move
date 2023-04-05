#[test_only]
module 0x1::NFTTests {
    use std::guid;
    use 0x1::NFT;
    use 0x1::NFTGallery;
    use std::option;

    /// A test token type to instantiate generic Tokens with.
    struct Game has copy, store, drop {
        name: vector<u8>,
        edition: u64,
    }

    struct Collection has copy, store, drop {
        name: vector<u8>,
    }

    struct Pokemon has copy, store, drop {
        name: vector<u8>,
        type: vector<u8>,
    }

    const EMINT_FAILED: u64 = 0;
    const ETRANSFER_FAILED: u64 = 1;
    const ECOLLECTION_FAILED: u64 = 2;

    #[test(admin=@0xa550c18, creator=@0x42, user=@0x43)]
    public entry fun test_all(admin: signer, creator: signer, user: signer) {
        /*
        ===============================================================
            Initialization + preparation
        ===============================================================
        */

        let creator_addr = @0x42;
        let user_addr = @0x43;

        NFT::nft_initialize(admin);
        NFTGallery::publish_gallery<Game>(&creator);
        NFTGallery::publish_gallery<Collection>(&creator);
        NFTGallery::publish_gallery<Pokemon>(&creator);
        NFTGallery::publish_gallery<Game>(&user);

        let token1_id = guid::create_id(creator_addr, 0);
        let token2_id = guid::create_id(creator_addr, 1);

        /*
        ===============================================================
            Test minting
        ===============================================================
        */

        let token1 = NFT::create<Game>(
            &creator,
            Game { name: b"Mario", edition: 2008 },
            b"nintendo.com",
            10,
            option::none(),
        );
        assert!(NFT::get_balance(&token1) == 10, EMINT_FAILED);
        assert!(NFT::get_supply(&token1) == 10, EMINT_FAILED);
        assert!(NFT::get_content_uri(&token1) == b"nintendo.com", EMINT_FAILED);
        assert!(NFT::get_metadata(&token1) == Game { name: b"Mario", edition: 2008, }, EMINT_FAILED);
        assert!(NFT::get_parent_id(&token1) == option::none(), EMINT_FAILED);


        // Add all 10 tokens to creator's own account
        NFTGallery::add_to_gallery<Game>(creator_addr, token1);

        // assert! creator has the right number of tokens and supply is 10.
        assert!(NFTGallery::has_token<Game>(creator_addr, &token1_id), EMINT_FAILED);
        assert!(NFTGallery::get_token_balance<Game>(creator_addr, &token1_id) == 10, EMINT_FAILED);
        assert!(NFTGallery::get_token_balance<Game>(creator_addr, &token1_id) == 10, EMINT_FAILED);

        let token2 = NFT::create<Game>(
            &creator,
            Game { name: b"ChromeDino", edition: 2015 },
            b"google.com",
            233,
            option::none(),
        );
        NFTGallery::add_to_gallery<Game>(creator_addr, token2);
        assert!(NFTGallery::has_token<Game>(creator_addr, &token2_id), EMINT_FAILED);
        assert!(NFTGallery::get_token_balance<Game>(creator_addr, &token2_id) == 233, EMINT_FAILED);



        /*
        ===============================================================
            Test collections
        ===============================================================
        */

        // Create collection first
        let collection = NFT::create<Collection>(
            &creator,
            Collection { name: b"Pokemon" },
            b"nintendo.com",
            1,
            option::none(),
        );

        let pikachu = NFT::create<Pokemon>(
            &creator,
            Pokemon { name: b"Pikachu", type: b"electric", },
            b"nintendo.com",
            10,
            option::some(NFT::id(&collection)),
        );
        let charmander = NFT::create<Pokemon>(
            &creator,
            Pokemon { name: b"Charmander", type: b"fire", },
            b"nintendo.com",
            10,
            option::some(NFT::id(&collection)),
        );

        let pikachu_id = NFT::id(&pikachu);
        NFTGallery::add_to_gallery<Pokemon>(creator_addr, pikachu);
        assert!(NFTGallery::get_token_balance<Pokemon>(creator_addr, &pikachu_id) == 10, ECOLLECTION_FAILED);
        assert!(NFTGallery::get_token_supply<Pokemon>(creator_addr, &pikachu_id) == 10, ECOLLECTION_FAILED);
        assert!(NFTGallery::get_token_content_uri<Pokemon>(creator_addr, &pikachu_id) == b"nintendo.com", ECOLLECTION_FAILED);
        assert!(NFTGallery::get_token_metadata<Pokemon>(creator_addr, &pikachu_id) == Pokemon { name: b"Pikachu", type: b"electric", }, ECOLLECTION_FAILED);
        assert!(NFTGallery::get_token_parent_id<Pokemon>(creator_addr, &pikachu_id) == option::some(NFT::id(&collection)), ECOLLECTION_FAILED);

        NFTGallery::add_to_gallery<Pokemon>(creator_addr, charmander);
        NFTGallery::add_to_gallery<Collection>(creator_addr, collection);

        /*
        ===============================================================
            Test transferring tokens without splitting of tokens
        ===============================================================
        */

        // Transfer 6 units of token1 from creator to user
        NFTGallery::transfer_token_between_galleries_impl<Game>(
            &creator, // from
            user_addr, // to
            6, // amount
            creator_addr, // token.id.addr
            0, // token.id.creation_num
        );

        assert!(NFTGallery::has_token<Game>(creator_addr, &token1_id), ETRANSFER_FAILED);
        assert!(NFTGallery::get_token_balance<Game>(creator_addr, &token1_id) == 4, ETRANSFER_FAILED);
        assert!(NFTGallery::has_token<Game>(user_addr, &token1_id), ETRANSFER_FAILED);
        assert!(NFTGallery::get_token_balance<Game>(user_addr, &token1_id) == 6, ETRANSFER_FAILED);
        assert!(NFTGallery::get_token_supply<Game>(user_addr, &token1_id) == 10, ETRANSFER_FAILED); // supply should not change


        /*
        ===============================================================
            Test transferring tokens with splitting of tokens
        ===============================================================
        */

        // Transfer all 6 units of token1 from user to creator
        NFTGallery::transfer_token_between_galleries_impl<Game>(
            &user, creator_addr, 6, creator_addr, 0,
        );
        assert!(!NFTGallery::has_token<Game>(user_addr, &token1_id), ETRANSFER_FAILED); // user doesn't have token1 anymore
        assert!(NFTGallery::get_token_balance<Game>(user_addr, &token1_id) == 0, ETRANSFER_FAILED);
        assert!(NFTGallery::has_token<Game>(creator_addr, &token1_id), ETRANSFER_FAILED);
        assert!(NFTGallery::get_token_balance<Game>(creator_addr, &token1_id) == 10, ETRANSFER_FAILED);

        /*
        ===============================================================
            Test tokens with inline data
        ===============================================================
        */
        let nft = NFT::create<Game>(
            &creator,
            Game { name: b"Mario", edition: 2008 },
            b"nintendo.com",
            1,
            option::none(),
        );
        assert!(NFT::is_data_inlined(&nft), EMINT_FAILED);
        assert!(NFT::get_balance(&nft) == 1, EMINT_FAILED);
        assert!(NFT::get_supply(&nft) == 1, EMINT_FAILED);
        assert!(NFT::get_content_uri(&nft) == b"nintendo.com", EMINT_FAILED);
        assert!(NFT::get_metadata(&nft) == Game { name: b"Mario", edition: 2008, }, EMINT_FAILED);
        assert!(NFT::get_parent_id(&nft) == option::none(), EMINT_FAILED);

        let nft_id = NFT::id(&nft);
        let nft_creator_addr = guid::id_creator_address(&nft_id);
        let nft_creation_num = guid::id_creation_num(&nft_id);
        NFTGallery::add_to_gallery<Game>(creator_addr, nft);
        assert!(NFTGallery::has_token<Game>(creator_addr, &nft_id), EMINT_FAILED);


        NFTGallery::transfer_token_between_galleries_impl<Game>(
            &creator, // from
            user_addr, // to
            1, // amount
            nft_creator_addr, // token.id.addr
            nft_creation_num, // token.id.creation_num
        );
        assert!(!NFTGallery::has_token<Game>(creator_addr, &nft_id), ETRANSFER_FAILED);
        assert!(NFTGallery::has_token<Game>(user_addr, &nft_id), ETRANSFER_FAILED);
        assert!(NFTGallery::get_token_balance<Game>(user_addr, &nft_id) == 1, ETRANSFER_FAILED);
        assert!(NFTGallery::get_token_supply<Game>(user_addr, &nft_id) == 1, ETRANSFER_FAILED);
        assert!(NFTGallery::get_token_metadata<Game>(user_addr, &nft_id) == Game { name: b"Mario", edition: 2008, }, ETRANSFER_FAILED);
    }
}
