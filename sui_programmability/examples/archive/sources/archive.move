// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module archive::archive {
    use sui::object::{Self, UID, ID};
    use std::option::{Self, Option};
    use sui::transfer;
    use sui::vec_map::{Self, VecMap};
    use std::string::{Self, utf8, String};
    use std::vector;
    use sui::table::{Self, Table};
    use sui::tx_context::{Self, TxContext};
    use sui::clock::{Self, Clock};

    // struct Archive has key {
    //     id: UID,
    //     records: Table<String, BookRecord>,
    //     reverse: Table<address, String>,
    // }

    struct Registry has key, store {
        id: UID,
        /// The `registry` table maps `Domain` to `NameRecord`.
        /// Added / replaced in the `add_record` function.
        registry: Table<Domain, NameRecord>,
        /// The `reverse_registry` table maps `address` to `domain_name`.
        /// Updated in the `set_reverse_lookup` function.
        reverse_registry: Table<address, Domain>,
    }

    // struct BookRecord has store {
    //     owner: address,
    //     marker: address,
    //     last_updated: u64
    // }

    struct Domain has copy, drop, store {
        /// Vector of labels that make up a domain.
        ///
        /// Labels are stored in reverse order such that the TLD is always in position `0`.
        /// e.g. domain "pay.name.sui" will be stored in the vector as ["sui", "name", "pay"].
        labels: vector<String>,
    }

    struct NameRecord has copy, store, drop {
        /// The ID of the `RegistrationNFT` assigned to this record.
        ///
        /// The owner of the corrisponding `RegistrationNFT` has the rights to
        /// be able to change and adjust the `target_address` of this domain.
        ///
        /// It is possible that the ID changes if the record expires and is
        /// purchased by someone else.
        nft_id: ID,
        /// Timestamp in milliseconds when the record expires.
        expiration_timestamp_ms: u64,
        /// The target address that this domain points to
        target_address: Option<address>,
        /// Additional data which may be stored in a record
        data: VecMap<String, String>,
    }

    struct RegistrationNFT has key, store, drop {
        id: UID,
        /// The domain name that the NFT is for.
        domain: Domain,
        /// Timestamp in milliseconds when this NFT expires.
        expiration_timestamp_ms: u64,
        /// Short IPFS hash of the image to be displayed for the NFT.
        image_url: String,
    }

    fun init(ctx: &mut TxContext) {
        // transfer::share_object(Archive {
        //     id: object::new(ctx),
        //     records: table::new(ctx),
        //     reverse: table::new(ctx),
        // })
        transfer::share_object(Registry {
            id: object::new(ctx),
            registry: table::new(ctx),
            reverse_registry: table::new(ctx),
        })
    }

    // entry fun add_record(self: &mut Archive, clock: &Clock, marker: address, name: String, ctx: &TxContext) {
    //     table::add(&mut self.records, name, BookRecord {
    //         owner: tx_context::sender(ctx),
    //         marker,
    //         last_updated: clock::timestamp_ms(clock)
    //     });

    //     table::add(&mut self.reverse, marker, name);
    // }

    fun split_by_dot(s: String): vector<String> {
        let dot = utf8(b".");
        let parts: vector<String> = vector[];
        while (!string::is_empty(&s)) {
            let index_of_next_dot = string::index_of(&s, &dot);
            let part = string::sub_string(&s, 0, index_of_next_dot);
            vector::push_back(&mut parts, part);

            let len = string::length(&s);
            let start_of_next_part = if (index_of_next_dot == len) {
                len
            } else {
                index_of_next_dot + 1
            };

            s = string::sub_string(&s, start_of_next_part, len);
        };

        parts
    }

    entry fun add_record(
        self: &mut Registry,
        domain_str: String,
        no_years: u8,
        clock: &Clock,
        ctx: &mut TxContext,
    ) {

        let labels = split_by_dot(domain_str);
        vector::reverse(&mut labels);
        let domain = Domain {
            labels
        };

        let expiration_t = clock::timestamp_ms(clock) + ((no_years as u64) * 365 * 24 * 60 * 60 * 1000);
        let nft = RegistrationNFT {
            id: object::new(ctx),
            domain,
            expiration_timestamp_ms: expiration_t,
            image_url: utf8(b"QmaLFg4tQYansFpyRqmDfABdkUVy66dHtpnkH15v1LPzcY"),
        };
        let name_record = NameRecord {
            nft_id: object::id(&nft),
            expiration_timestamp_ms: expiration_t,
            target_address: option::none(),
            data: vec_map::empty(),
        };
        table::add(&mut self.registry, domain, name_record);
    }

    entry fun set_reverse_lookup(
        self: &mut Registry,
        address: address,
        domain_str: String,
    ) {
        let labels = split_by_dot(domain_str);
        vector::reverse(&mut labels);
        let domain = Domain {
            labels
        };
        table::add(&mut self.reverse_registry, address, domain);
    }
}
