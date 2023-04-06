// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module archive::archive {
    use sui::object::{Self, UID};
    use sui::transfer;
    use std::string::String;
    use sui::table::{Self, Table};
    use sui::tx_context::{Self, TxContext};
    use sui::clock::{Self, Clock};

    struct Archive has key {
        id: UID,
        records: Table<String, BookRecord>,
        reverse: Table<address, String>,
    }

    struct BookRecord has store {
        owner: address,
        marker: address,
        last_updated: u64
    }

    fun init(ctx: &mut TxContext) {
        transfer::share_object(Archive {
            id: object::new(ctx),
            records: table::new(ctx),
            reverse: table::new(ctx),
        })
    }

    public fun add_record(self: &mut Archive, clock: &Clock, marker: address, name: String, ctx: &TxContext) {
        table::add(&mut self.records, name, BookRecord {
            owner: tx_context::sender(ctx),
            marker,
            last_updated: clock::timestamp_ms(clock)
        });

        table::add(&mut self.reverse, marker, name);
}
}
