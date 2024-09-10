// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module oracle::simple_oracle {
    use std::string;
    use std::string::String;

    use oracle::data::{Self, Data};
    use sui::clock::{Self, Clock};
    use sui::dynamic_field as df;
    use sui::table;
    use sui::table::Table;

    const ESenderNotOracle: u64 = 0;
    const ETickerNotExists: u64 = 1;

    public struct SimpleOracle has store, key {
        id: UID,
        /// The address of the oracle.
        address: address,
        /// The name of the oracle.
        name: String,
        /// The description of the oracle.
        description: String,
        /// The URL of the oracle.
        url: String,
    }

    public struct StoredData<T: store> has copy, store, drop {
        value: T,
        sequence_number: u64,
        timestamp: u64,
        /// An identifier for the reading (for example real time of observation, or sequence number of observation on other chain).
        identifier: String,
    }

    public fun get_historical_data<K: copy + drop + store, V: store + copy>(
        oracle: &SimpleOracle,
        ticker: String,
        archival_key: K
    ): Option<Data<V>> {
        string::append(&mut string::utf8(b"[historical] "), ticker);
        let historical_data: &Table<K, StoredData<V>> = df::borrow(&oracle.id, ticker);
        let StoredData { value, sequence_number, timestamp, identifier } = *table::borrow(
            historical_data,
            archival_key
        );
        option::some(data::new(value, ticker, sequence_number, timestamp, oracle.address, identifier))
    }

    public fun get_latest_data<T: store + copy>(oracle: &SimpleOracle, ticker: String): Option<Data<T>> {
        if (!df::exists_(&oracle.id, ticker)) {
            return option::none()
        };
        let data: &StoredData<T> = df::borrow(&oracle.id, ticker);
        let StoredData { value, sequence_number, timestamp, identifier } = *data;
        option::some(data::new(value, ticker, sequence_number, timestamp, oracle.address, identifier))
    }

    /// Create a new shared SimpleOracle object for publishing data.
    public entry fun create(name: String, url: String, description: String, ctx: &mut TxContext) {
        let oracle = SimpleOracle { id: object::new(ctx), address: tx_context::sender(ctx), name, description, url };
        transfer::share_object(oracle)
    }

    public entry fun submit_data<T: store + copy + drop>(
        oracle: &mut SimpleOracle,
        clock: &Clock,
        ticker: String,
        value: T,
        identifier: String,
        ctx: &mut TxContext
    ) {
        assert!(oracle.address == tx_context::sender(ctx), ESenderNotOracle);

        let old_data: Option<StoredData<T>> = df::remove_if_exists(&mut oracle.id, ticker);

        let sequence_number = if (option::is_some(&old_data)) {
            let seq = option::borrow(&old_data).sequence_number + 1;
            let _ = option::destroy_some(old_data);
            seq
        } else {
            option::destroy_none(old_data);
            0
        };

        let new_data = StoredData {
            value,
            sequence_number,
            timestamp: clock::timestamp_ms(clock),
            identifier,
        };
        df::add(&mut oracle.id, ticker, new_data);
    }

    public entry fun archive_data<K: store + copy + drop, V: store + copy + drop>(
        oracle: &mut SimpleOracle,
        ticker: String,
        archival_key: K,
        ctx: &mut TxContext
    ) {
        assert!(oracle.address == tx_context::sender(ctx), ESenderNotOracle);
        assert!(df::exists_(&oracle.id, ticker), ETickerNotExists);

        let latest_data: StoredData<V> = *df::borrow_mut(&mut oracle.id, ticker);

        string::append(&mut string::utf8(b"[historical] "), ticker);
        if (!df::exists_(&oracle.id, ticker)) {
            let data_source = table::new<K, StoredData<V>>(ctx);
            df::add(&mut oracle.id, ticker, data_source);
        };
        let historical_data: &mut Table<K, StoredData<V>> = df::borrow_mut(&mut oracle.id, ticker);
        // Replace the old data in historical data if any.
        if (table::contains(historical_data, archival_key)) {
            table::remove(historical_data, archival_key);
        };
        table::add(historical_data, archival_key, latest_data);
    }
}
