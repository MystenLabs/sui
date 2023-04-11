module 0x1::NFTGallery {
    use std::guid;
    use 0x1::NFT::{Self, Token};
    use std::option::{Self, Option};
    use std::signer;
    use std::vector;

    /// Gallery holding tokens of `TokenType` as well as information of approved operators.
    struct NFTGallery<TokenType: copy + store + drop> has key {
        gallery: vector<Token<TokenType>>
    }

    // Error codes
    const EID_NOT_FOUND: u64 = 0;
    const EGALLERY_NOT_PUBLISHED: u64 = 1;
    const EGALLERY_ALREADY_PUBLISHED: u64 = 2;
    const EINVALID_AMOUNT_OF_TRANSFER: u64 = 3;

    /// Add a token to the owner's gallery.
    /// The specifics of the addition depend on the token data inlining.
    /// In case the token data is inlined, the addition is trivial (join / split operations are not allowed).
    /// Otherwise, the addition might include joining of the two tokens.
    public fun add_to_gallery<TokenType: copy + store + drop>(owner: address, token: Token<TokenType>)
    acquires NFTGallery {
        assert!(exists<NFTGallery<TokenType>>(owner), EGALLERY_NOT_PUBLISHED);
        let gallery = &mut borrow_global_mut<NFTGallery<TokenType>>(owner).gallery;
        if (!NFT::is_data_inlined<TokenType>(&token)) {
            let index_opt = index_of_token<TokenType>(gallery, &NFT::id<TokenType>(&token));
            if (option::is_some(&index_opt)) {
                let prev_token_idx = option::extract(&mut index_opt);
                // The gallery already has the given token: update its balance
                NFT::join<TokenType>(vector::borrow_mut(gallery, prev_token_idx), token);
                return
            }
        };
        vector::push_back(gallery, token)
    }

    /// Returns whether the owner has a token with given id.
    public fun has_token<TokenType: copy + store + drop>(owner: address, token_id: &guid::ID): bool acquires NFTGallery {
        option::is_some(&index_of_token(&borrow_global<NFTGallery<TokenType>>(owner).gallery, token_id))
    }

    public fun get_token_balance<TokenType: copy + store + drop>(owner: address, token_id: &guid::ID
    ): u64 acquires NFTGallery {
        let gallery = &borrow_global<NFTGallery<TokenType>>(owner).gallery;
        let index_opt = index_of_token<TokenType>(gallery, token_id);
        if (option::is_none(&index_opt)) {
            0
        } else {
            let token = vector::borrow(gallery, option::extract(&mut index_opt));
            NFT::get_balance(token)
        }
    }

    /// Returns the overall supply for the given token (across this and potentially other galleries),
    // aborts if token with the given ID is not found.
    public fun get_token_supply<TokenType: copy + store + drop>(owner: address, token_id: &guid::ID): u64 acquires NFTGallery {
        let gallery = &borrow_global<NFTGallery<TokenType>>(owner).gallery;
        let index_opt = index_of_token<TokenType>(gallery, token_id);
        assert!(option::is_some(&index_opt), EID_NOT_FOUND);
        let token = vector::borrow(gallery, option::extract(&mut index_opt));
        NFT::get_supply(token)
    }

    /// Returns a copy of the token content uri
    public fun get_token_content_uri<TokenType: copy + store + drop>(owner: address, token_id: &guid::ID): vector<u8> acquires NFTGallery {
        let gallery = &borrow_global<NFTGallery<TokenType>>(owner).gallery;
        let index_opt = index_of_token<TokenType>(gallery, token_id);
        assert!(option::is_some(&index_opt), EID_NOT_FOUND);
        let token = vector::borrow(gallery, option::extract(&mut index_opt));
        NFT::get_content_uri(token)
    }

    /// Returns a copy of the token metadata
    public fun get_token_metadata<TokenType: copy + store + drop>(owner: address, token_id: &guid::ID): TokenType acquires NFTGallery {
        let gallery = &borrow_global<NFTGallery<TokenType>>(owner).gallery;
        let index_opt = index_of_token<TokenType>(gallery, token_id);
        assert!(option::is_some(&index_opt), EID_NOT_FOUND);
        let token = vector::borrow(gallery, option::extract(&mut index_opt));
        NFT::get_metadata(token)
    }

    /// Returns a copy of the token parent id
    public fun get_token_parent_id<TokenType: copy + store + drop>(owner: address, token_id: &guid::ID): Option<guid::ID> acquires NFTGallery {
        let gallery = &borrow_global<NFTGallery<TokenType>>(owner).gallery;
        let index_opt = index_of_token<TokenType>(gallery, token_id);
        assert!(option::is_some(&index_opt), EID_NOT_FOUND);
        let token = vector::borrow(gallery, option::extract(&mut index_opt));
        NFT::get_parent_id(token)
    }

    /// Transfer `amount` of token with id `GUID::id(creator, creation_num)` from `owner`'s
    /// balance to `to`'s balance. This operation has to be done by either the owner or an
    /// approved operator of the owner.
    public entry fun transfer_token_between_galleries<TokenType: copy + store + drop>(
        account: signer,
        to: address,
        amount: u64,
        creator: address,
        creation_num: u64
    ) acquires NFTGallery {
        transfer_token_between_galleries_impl<TokenType>(&account, to, amount, creator, creation_num)
    }

    /// The implementation, which doesn't consume signer, and thus can be used for testing.
    public fun transfer_token_between_galleries_impl<TokenType: copy + store + drop>(
        account: &signer,
        to: address,
        amount: u64,
        creator: address,
        creation_num: u64
    ) acquires NFTGallery {
        let owner = signer::address_of(account);
        assert!(amount > 0, EINVALID_AMOUNT_OF_TRANSFER);
        let gallery = &mut borrow_global_mut<NFTGallery<TokenType>>(owner).gallery;
        let id = guid::create_id(creator, creation_num);

        let index_opt = index_of_token<TokenType>(gallery, &id);
        assert!(option::is_some(&index_opt), EID_NOT_FOUND);
        let from_token_idx = option::extract(&mut index_opt);

        if (NFT::is_data_inlined(vector::borrow(gallery, from_token_idx)) ||
                NFT::get_balance(vector::borrow(gallery, from_token_idx)) == amount) {
            // Move the token from one gallery to another
            let token = vector::remove(gallery, from_token_idx);
            add_to_gallery<TokenType>(to, token)
        } else {
            // Split the original token and add the splitted part to another gallery
            let split_out_token = NFT::split_out(vector::borrow_mut(gallery, from_token_idx), amount);
            add_to_gallery<TokenType>(to, split_out_token)
        };
        // Emit transfer event
        NFT::emit_transfer_event(
            &id,
            account,
            to,
            amount,
        )
    }

    public fun publish_gallery<TokenType: copy + store + drop>(account: &signer) {
        assert!(!exists<NFTGallery<TokenType>>(signer::address_of(account)), EGALLERY_ALREADY_PUBLISHED);
        move_to(account, NFTGallery<TokenType> { gallery: vector::empty() });
    }

    /// Finds the index of token with the given id in the gallery.
    fun index_of_token<TokenType: copy + store + drop>(gallery: &vector<Token<TokenType>>, id: &guid::ID): Option<u64> {
        let i = 0;
        let len = vector::length(gallery);
        while (i < len) {
            if (NFT::id<TokenType>(vector::borrow(gallery, i)) == *id) {
                return option::some(i)
            };
            i = i + 1;
        };
        option::none()
    }
}
