module ExperimentalFramework::MultiTokenBalance {
    use std::errors;
    use std::guid;
    use ExperimentalFramework::MultiToken::{Self, Token};
    use std::option::{Self, Option};
    use std::signer;
    use std::vector;

    /// Balance holding tokens of `TokenType` as well as information of approved operators.
    struct TokenBalance<phantom TokenType: store> has key {
        /// Gallery full of multi tokens owned by this balance
        gallery: vector<Token<TokenType>>
    }

    spec TokenBalance {
        invariant forall i1 in range(gallery), i2 in range(gallery) where gallery[i1].id == gallery[i2].id:
        i1 == i2;
    }

    spec fun get_token_balance_gallery<TokenType>(addr: address): vector<Token<TokenType>>{
        global<TokenBalance<TokenType>>(addr).gallery
    }

    spec fun is_in_gallery<TokenType>(gallery: vector<Token<TokenType>>, token_id: guid::ID): bool {
        exists i in range(gallery): gallery[i].id == token_id
    }

    spec fun find_token_index_by_id<TokenType>(gallery: vector<Token<TokenType>>, id: guid::ID): u64 {
        choose i in range(gallery) where gallery[i].id == id
    }

    const MAX_U64: u128 = 18446744073709551615u128;
    // Error codes
    const EID_NOT_FOUND: u64 = 0;
    const EBALANCE_NOT_PUBLISHED: u64 = 1;
    const EBALANCE_ALREADY_PUBLISHED: u64 = 2;
    const EINVALID_AMOUNT_OF_TRANSFER: u64 = 3;
    const EALREADY_IS_OPERATOR: u64 = 4;
    const ENOT_OPERATOR: u64 = 5;
    const EINVALID_APPROVAL_TARGET: u64 = 6;



    /// Add a token to the owner's gallery. If there is already a token of the same id in the
    /// gallery, we combine it with the new one and make a token of greater value.
    public fun add_to_gallery<TokenType: store>(owner: address, token: Token<TokenType>)
    acquires TokenBalance {
        assert!(exists<TokenBalance<TokenType>>(owner), EBALANCE_NOT_PUBLISHED);
        let id = MultiToken::id<TokenType>(&token);
        if (has_token<TokenType>(owner, &id)) {
            // If `owner` already has a token with the same id, remove it from the gallery
            // and join it with the new token.
            let original_token = remove_from_gallery<TokenType>(owner, &id);
            MultiToken::join<TokenType>(&mut token, original_token);
        };
        let gallery = &mut borrow_global_mut<TokenBalance<TokenType>>(owner).gallery;
        vector::push_back(gallery, token);
    }



    spec add_to_gallery {
        let gallery = get_token_balance_gallery<TokenType>(owner);
        let token_bal = token.balance;
        let min_token_idx = find_token_index_by_id(gallery, token.id);
        let balance = gallery[min_token_idx].balance;
        let post post_gallery = get_token_balance_gallery<TokenType>(owner);

        aborts_if !exists<TokenBalance<TokenType>>(owner);
        aborts_if is_in_gallery(gallery, token.id) && MAX_U64 - token.balance < balance;

        ensures is_in_gallery(gallery, token.id) ==> len(gallery) == len(post_gallery);
        ensures !is_in_gallery(gallery, token.id) ==> len(gallery) + 1 == len(post_gallery);

        ensures is_in_gallery(gallery, token.id) ==> post_gallery[len(gallery) - 1].balance ==
            token_bal + gallery[min_token_idx].balance;
        ensures post_gallery[len(post_gallery) - 1].id == token.id;
    }

    /// Remove a token of certain id from the owner's gallery and return it.
    fun remove_from_gallery<TokenType: store>(owner: address, id: &guid::ID): Token<TokenType>
    acquires TokenBalance {
        assert!(exists<TokenBalance<TokenType>>(owner), EBALANCE_NOT_PUBLISHED);
        let gallery = &mut borrow_global_mut<TokenBalance<TokenType>>(owner).gallery;
        let index_opt = index_of_token<TokenType>(gallery, id);
        assert!(option::is_some(&index_opt), errors::limit_exceeded(EID_NOT_FOUND));
        vector::remove(gallery, option::extract(&mut index_opt))
    }

    spec remove_from_gallery {
        let gallery = get_token_balance_gallery<TokenType>(owner);
        aborts_if !exists<TokenBalance<TokenType>>(owner);
        aborts_if !is_in_gallery(gallery, id);
        ensures !is_in_gallery(get_token_balance_gallery<TokenType>(owner), id);
    }

    /// Finds the index of token with the given id in the gallery.
    fun index_of_token<TokenType: store>(gallery: &vector<Token<TokenType>>, id: &guid::ID): Option<u64> {
        let i = 0;
        let len = vector::length(gallery);
        while ({spec {
            invariant i >= 0;
            invariant i <= len(gallery);
            invariant forall k in 0..i: gallery[k].id != id;
        };(i < len)}) {
            if (MultiToken::id<TokenType>(vector::borrow(gallery, i)) == *id) {
                return option::some(i)
            };
            i = i + 1;
        };
        option::none()
    }

    spec index_of_token{
        let min_token_idx = choose min i in range(gallery) where gallery[i].id == id;
        let post res_id = option::borrow(result);
        ensures is_in_gallery(gallery, id) <==> (option::is_some(result) && res_id == min_token_idx);
        ensures result ==  option::spec_none() <==> !is_in_gallery(gallery, id);
    }

    /// Returns whether the owner has a token with given id.
    public fun has_token<TokenType: store>(owner: address, token_id: &guid::ID): bool acquires TokenBalance {
        option::is_some(&index_of_token(&borrow_global<TokenBalance<TokenType>>(owner).gallery, token_id))
    }

    spec has_token {
        let gallery = global<TokenBalance<TokenType>>(owner).gallery;
        ensures result <==> is_in_gallery(gallery, token_id);
    }

    public fun get_token_balance<TokenType: store>(owner: address, token_id: &guid::ID
    ): u64 acquires TokenBalance {
        let gallery = &borrow_global<TokenBalance<TokenType>>(owner).gallery;
        let index_opt = index_of_token<TokenType>(gallery, token_id);

        if (option::is_none(&index_opt)) {
            0
        } else {
            let index = option::extract(&mut index_opt);
            MultiToken::balance(vector::borrow(gallery, index))
        }
    }

    spec get_token_balance {
        let gallery = get_token_balance_gallery<TokenType>(owner);
        let min_token_idx = find_token_index_by_id(gallery, token_id);
        ensures !is_in_gallery(gallery, token_id) ==> result == 0;
        ensures is_in_gallery(gallery, token_id) ==> result == gallery[min_token_idx].balance;
    }

    /// Transfer `amount` of token with id `GUID::id(creator, creation_num)` from `owner`'s
    /// balance to `to`'s balance. This operation has to be done by either the owner or an
    /// approved operator of the owner.
    public entry fun transfer_multi_token_between_galleries<TokenType: store>(
        account: signer,
        to: address,
        amount: u64,
        creator: address,
        creation_num: u64
    ) acquires TokenBalance {
        let owner = signer::address_of(&account);

        assert!(amount > 0, EINVALID_AMOUNT_OF_TRANSFER);

        // Remove NFT from `owner`'s gallery
        let id = guid::create_id(creator, creation_num);
        let token = remove_from_gallery<TokenType>(owner, &id);

        assert!(amount <= MultiToken::balance(&token), EINVALID_AMOUNT_OF_TRANSFER);

        if (amount == MultiToken::balance(&token)) {
            // Owner does not have any token left, so add token to `to`'s gallery.
            add_to_gallery<TokenType>(to, token);
        } else {
            // Split owner's token into two
            let (owner_token, to_token) = MultiToken::split<TokenType>(token, amount);

            // Add tokens to owner's gallery.
            add_to_gallery<TokenType>(owner, owner_token);

            // Add tokens to `to`'s gallery
            add_to_gallery<TokenType>(to, to_token);
        };

        // TODO: add event emission
    }

    spec transfer_multi_token_between_galleries {
        let owner = signer::address_of(account);
        let gallery_owner = get_token_balance_gallery<TokenType>(owner);
        let gallery_to = get_token_balance_gallery<TokenType>(to);
        let post post_gallery_owner = get_token_balance_gallery<TokenType>(owner);
        let post post_gallery_to = get_token_balance_gallery<TokenType>(to);

        let id = guid::create_id(creator, creation_num);

        let min_token_idx = find_token_index_by_id(gallery_owner, id);
        let min_token_idx_to = find_token_index_by_id(gallery_to, id);

        aborts_if amount <= 0;
        aborts_if !exists<TokenBalance<TokenType>>(owner);
        aborts_if !exists<TokenBalance<TokenType>>(to);
        aborts_if !is_in_gallery(gallery_owner, id);
        aborts_if amount > gallery_owner[min_token_idx].balance;
        aborts_if owner != to && is_in_gallery(gallery_to, id) && MAX_U64 - amount < gallery_to[min_token_idx_to].balance;

        ensures (gallery_owner[min_token_idx].balance == amount && to != owner) ==>
                !is_in_gallery(post_gallery_owner, id);

        ensures gallery_owner[min_token_idx].balance > amount ==>
                post_gallery_owner[len(post_gallery_owner) - 1].id == id;
        ensures post_gallery_to[len(post_gallery_to) - 1].id == id;

        ensures (gallery_owner[min_token_idx].balance > amount && to != owner) ==>
                post_gallery_owner[len(post_gallery_owner) - 1].balance ==
                  gallery_owner[min_token_idx].balance - amount;

        ensures (to != owner && !is_in_gallery(gallery_to, id) )==>
                post_gallery_to[len(post_gallery_to) - 1].balance == amount;
        ensures (to != owner && is_in_gallery(gallery_to, id) )==>
                post_gallery_to[len(post_gallery_to) - 1].balance ==
                   gallery_to[min_token_idx_to].balance + amount;

        ensures to == owner ==> post_gallery_owner[len(post_gallery_owner) - 1].balance ==
                                gallery_owner[min_token_idx].balance;

    }

    public fun publish_balance<TokenType: store>(account: &signer) {
        assert!(!exists<TokenBalance<TokenType>>(signer::address_of(account)), EBALANCE_ALREADY_PUBLISHED);
        move_to(account, TokenBalance<TokenType> { gallery: vector::empty() });
    }

    spec publish_balance {
        let addr = signer::address_of(account);
        aborts_if exists<TokenBalance<TokenType>>(addr);
        ensures exists<TokenBalance<TokenType>>(addr);
    }
}
