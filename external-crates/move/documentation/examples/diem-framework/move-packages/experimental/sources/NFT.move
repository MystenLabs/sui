address 0x1 {
    module NFT {
        use std::event;
        use std::guid;
        use std::option::{Self, Option};
        use std::signer;
        use std::vector;

        /// Struct representing a semi-fungible or non-fungible token (depending on the supply).
        /// There can be multiple tokens with the same id (unless supply is 1). Each token's
        /// corresponding token metadata is stored inside a TokenData inside TokenDataCollection
        /// under the creator's address.
        /// The TokenData might be inlined together with the token in case the token is unique, i.e., its balance is 1
        /// (we might choose to extend inlining for the non-unique NFTs in future).
        /// The TokenData can also be separated out to a separate creator's collection in order to normalize the
        /// data layout: we'd want to keep a single instance of the token data in case its balance is large.
        struct Token<TokenType: copy + store + drop> has key, store {
            id: guid::ID,
            balance: u64,
            token_data: Option<TokenData<TokenType>>,
        }

        /// Struct representing data of a specific token with token_id,
        /// stored under the creator's address inside TokenDataCollection.
        /// For each token_id, there is only one TokenData.
        struct TokenData<TokenType: copy + store + drop> has key, store {
            metadata: TokenType,
            /// Identifier for the token.
            token_id: guid::GUID,
            /// Pointer to where the content and metadata is stored.
            content_uri: vector<u8>,
            supply: u64,
            /// Parent NFT id
            parent_id: Option<guid::ID>
        }

        /// The data of the NFT tokens is either kept inline (in case their balance is 1), or is detached and kept
        /// in the token data collection by the original creator.
        struct TokenDataCollection<TokenType: copy + store + drop> has key {
            tokens: vector<TokenData<TokenType>>,
        }

        struct MintEvent has copy, drop, store {
            id: guid::ID,
            creator: address,
            content_uri: vector<u8>,
            amount: u64,
        }

        struct TransferEvent has copy, drop, store {
            id: guid::ID,
            from: address,
            to: address,
            amount: u64,
        }

        struct Admin has key {
            mint_events: event::EventHandle<MintEvent>,
            transfer_events: event::EventHandle<TransferEvent>,
        }

        /// Indicates that a user allows creation delegation for a given TokenType
        struct CreationDelegation<phantom TokenType: copy + store + drop> has key, store {
            guid_capability: guid::CreateCapability,
        }

        const ADMIN: address = @0xa550c18;
        const MAX_U64: u64 = 18446744073709551615u64;
        // Error codes
        /// Function can only be called by the admin address
        const ENOT_ADMIN: u64  = 0;
        const EWRONG_TOKEN_ID: u64 = 1;
        const ETOKEN_BALANCE_OVERFLOWS: u64 = 2;
        const EAMOUNT_EXCEEDS_TOKEN_BALANCE: u64 = 3;
        const ETOKEN_EXTRACTED: u64 = 4;
        const EINDEX_EXCEEDS_LENGTH: u64 = 5;
        const ETOKEN_PRESENT: u64 = 6;
        const EPARENT_NOT_SAME_ACCOUNT: u64 = 7;
        const ETOKEN_DATA_COLLECTION_ALREADY_PUBLISHED: u64 = 8;
        /// Creation delegation for a given token type is not allowed.
        const ECREATION_DELEGATION_NOT_ALLOWED: u64 = 9;
        /// Trying to merge or split tokens with inlined data.
        const EINLINE_DATA_OP: u64 = 10;

        /// Initialize this module
        public entry fun nft_initialize(account: signer) {
            assert!(signer::address_of(&account) == ADMIN, ENOT_ADMIN);
            move_to(&account, Admin {
                mint_events: event::new_event_handle<MintEvent>(&account),
                transfer_events: event::new_event_handle<TransferEvent>(&account),
            })
        }

        /// Returns the id of given token
        public fun id<TokenType: copy + store + drop>(token: &Token<TokenType>): guid::ID {
            *&token.id
        }

        /// Returns the balance of given token
        public fun get_balance<TokenType: copy + store + drop>(token: &Token<TokenType>): u64 {
            token.balance
        }

        /// Returns the overall supply for the given token
        public fun get_supply<TokenType: copy + store + drop>(token: &Token<TokenType>): u64 acquires TokenDataCollection{
            if (option::is_some(&token.token_data)) {
                option::borrow(&token.token_data).supply
            } else {
                let creator_addr = guid::id_creator_address(&token.id);
                let creator_tokens_data = &borrow_global<TokenDataCollection<TokenType>>(creator_addr).tokens;
                let token_data_idx = *option::borrow(&index_of_token<TokenType>(creator_tokens_data, &token.id));
                vector::borrow(creator_tokens_data, token_data_idx).supply
            }
        }

        /// Returns a copy of the token content uri
        public fun get_content_uri<TokenType: copy + store + drop>(token: &Token<TokenType>): vector<u8> acquires TokenDataCollection {
            if (option::is_some(&token.token_data)) {
                *&option::borrow(&token.token_data).content_uri
            } else {
                let creator_addr = guid::id_creator_address(&token.id);
                let creator_tokens_data = &borrow_global<TokenDataCollection<TokenType>>(creator_addr).tokens;
                let token_data_idx = *option::borrow(&index_of_token<TokenType>(creator_tokens_data, &token.id));
                *&vector::borrow(creator_tokens_data, token_data_idx).content_uri
            }
        }

        /// Returns a copy of the token metadata
        public fun get_metadata<TokenType: copy + store + drop>(token: &Token<TokenType>): TokenType acquires TokenDataCollection {
            if (option::is_some(&token.token_data)) {
                *&option::borrow(&token.token_data).metadata
            } else {
                let creator_addr = guid::id_creator_address(&token.id);
                let creator_tokens_data = &borrow_global<TokenDataCollection<TokenType>>(creator_addr).tokens;
                let token_data_idx = *option::borrow(&index_of_token<TokenType>(creator_tokens_data, &token.id));
                *&vector::borrow(creator_tokens_data, token_data_idx).metadata
            }
        }

        /// Returns a copy of the token metadata
        public fun get_parent_id<TokenType: copy + store + drop>(token: &Token<TokenType>): Option<guid::ID> acquires TokenDataCollection {
            if (option::is_some(&token.token_data)) {
                *&option::borrow(&token.token_data).parent_id
            } else {
                let creator_addr = guid::id_creator_address(&token.id);
                let creator_tokens_data = &borrow_global<TokenDataCollection<TokenType>>(creator_addr).tokens;
                let token_data_idx = *option::borrow(&index_of_token<TokenType>(creator_tokens_data, &token.id));
                *&vector::borrow(creator_tokens_data, token_data_idx).parent_id
            }
        }

        /// Returns true if the token is keeping the token data inlined.
        public fun is_data_inlined<TokenType: copy + store + drop>(token: &Token<TokenType>): bool {
            option::is_some(&token.token_data)
        }

        /// Finds the index of token with the given id in the gallery.
        fun index_of_token<TokenType: copy + store + drop>(gallery: &vector<TokenData<TokenType>>, id: &guid::ID): Option<u64> {
            let i = 0;
            let len = vector::length(gallery);
            while (i < len) {
                if (guid::eq_id(&vector::borrow(gallery, i).token_id, id)) {
                    return option::some(i)
                };
                i = i + 1;
            };
            option::none()
        }

        /// Adds the balance of `TokenID` to the balance of the given `Token`.
        public fun join<TokenType: copy + store + drop>(token: &mut Token<TokenType>, other: Token<TokenType>) {
            let Token { id, balance, token_data } = other;
            // Inlining is allowed for single-token NFTs only.
            option::destroy_none(token_data); // aborts in case token data is not None
            assert!(option::is_none(&token.token_data), EINLINE_DATA_OP);
            assert!(*&token.id == id, EWRONG_TOKEN_ID);
            assert!(MAX_U64 - token.balance >= balance, ETOKEN_BALANCE_OVERFLOWS);
            token.balance = token.balance + balance;
        }

        /// Split out a new token with the given amount from the original token.
        /// Aborts in case amount is greater or equal than the given token balance.
        public fun split_out<TokenType: copy + store + drop>(token: &mut Token<TokenType>, amount: u64): Token<TokenType> {
            assert!(token.balance >= amount, EAMOUNT_EXCEEDS_TOKEN_BALANCE);
            assert!(option::is_none(&token.token_data), EINLINE_DATA_OP);

            token.balance = token.balance - amount;
            Token {
                id: *&token.id,
                balance: amount,
                token_data: option::none(),
            }
        }

        /// Create an NFT on behalf of the given user, in case a user explicitly approved this delegation for the given
        /// NFT type.
        /// Only the entity, which can create an object of `TokenType`, will be able to call this function.
        public fun create_for<TokenType: copy + store + drop>(
            creator: address, metadata: TokenType, content_uri: vector<u8>, amount: u64, parent_id: Option<guid::ID>
        ): Token<TokenType> acquires CreationDelegation, Admin, TokenDataCollection {
            assert! (exists<CreationDelegation<TokenType>>(creator), ECREATION_DELEGATION_NOT_ALLOWED);
            let guid_creation_cap = &borrow_global<CreationDelegation<TokenType>>(creator).guid_capability;
            let guid = guid::create_with_capability(creator, guid_creation_cap);
            create_impl<TokenType>(
                creator,
                guid,
                metadata,
                content_uri,
                amount,
                parent_id
            )
        }

        /// Create a` TokenData<TokenType>` that wraps `metadata` and with balance of `amount`
        public fun create<TokenType: copy + store + drop>(
            account: &signer, metadata: TokenType, content_uri: vector<u8>, amount: u64, parent_id: Option<guid::ID>
        ): Token<TokenType> acquires Admin, TokenDataCollection {
            let guid = guid::create(account);
            if (!exists<TokenDataCollection<TokenType>>(signer::address_of(account))) {
                move_to(account, TokenDataCollection { tokens: vector::empty<TokenData<TokenType>>() });
            };
            create_impl<TokenType>(
                signer::address_of(account),
                guid,
                metadata,
                content_uri,
                amount,
                parent_id
            )
        }

        fun create_impl<TokenType: copy + store + drop>(
            addr: address,
            guid: guid::GUID,
            metadata: TokenType,
            content_uri: vector<u8>,
            amount: u64,
            parent_id: Option<guid::ID>
        ): Token<TokenType> acquires Admin, TokenDataCollection {
            event::emit_event(
                &mut borrow_global_mut<Admin>(ADMIN).mint_events,
                MintEvent {
                    id: guid::id(&guid),
                    creator: addr,
                    content_uri: copy content_uri,
                    amount,
                }
            );
            let id = guid::id(&guid);
            let token_data = TokenData { metadata, token_id: guid, content_uri, supply: amount, parent_id };
            if (amount == 1) {
                // inline token data
                Token { id, balance: amount, token_data: option::some(token_data) }
            } else {
                // keep token data in the collection of the creator
                let token_data_collection = &mut borrow_global_mut<TokenDataCollection<TokenType>>(addr).tokens;
                vector::push_back(token_data_collection, token_data);
                Token { id, balance: amount, token_data: option::none() }
            }
        }

        public fun publish_token_data_collection<TokenType: copy + store + drop>(account: &signer) {
            assert!(
                !exists<TokenDataCollection<TokenType>>(signer::address_of(account)),
                ETOKEN_DATA_COLLECTION_ALREADY_PUBLISHED
            );
            move_to(account, TokenDataCollection<TokenType> { tokens: vector::empty() });
        }

        /// Allow creation delegation for a given TokenType (the entity, which can generate a metadata of a given TokenType
        /// is going to be allowed to create an NFT on behalf of the user).
        /// This is useful in case a user is using a 3rd party app, which can create NFTs on their behalf.
        public fun allow_creation_delegation<TokenType: copy + store + drop>(account: &signer) {
            if (!exists<CreationDelegation<TokenType>>(signer::address_of(account))) {
                move_to(account, CreationDelegation<TokenType> { guid_capability: guid::gen_create_capability(account) });
                // In order to support creation delegation, prepare the token data collection ahead of time.
                if (!exists<TokenDataCollection<TokenType>>(signer::address_of(account))) {
                    move_to(account, TokenDataCollection { tokens: vector::empty<TokenData<TokenType>>() });
                };
            };
        }

        public fun emit_transfer_event(
            guid: &guid::ID,
            account: &signer,
            to: address,
            amount: u64,
        ) acquires Admin {
            event::emit_event(
                &mut borrow_global_mut<Admin>(ADMIN).transfer_events,
                TransferEvent {
                    id: *guid,
                    from: signer::address_of(account),
                    to: to,
                    amount: amount,
                }
            );
        }
    }
}
