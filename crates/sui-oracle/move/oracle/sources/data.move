// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module oracle::data {
    use std::string::String;

    public struct Data<T> has drop, copy {
        value: T,
        metadata: Metadata,
    }

    public struct Metadata has drop, copy {
        ticker: String,
        sequence_number: u64,
        timestamp: u64,
        oracle: address,
        /// An identifier for the reading (for example real time of observation, or sequence number of observation on other chain).
        identifier: String,
    }

    public fun new<T>(
        value: T,
        ticker: String,
        sequence_number: u64,
        timestamp: u64,
        oracle: address,
        identifier: String
    ): Data<T> {
        Data {
            value,
            metadata: Metadata {
                ticker,
                sequence_number,
                timestamp,
                oracle,
                identifier,
            },
        }
    }

    public fun value<T>(data: &Data<T>): &T {
        &data.value
    }

    public fun oracle_address<T>(data: &Data<T>): &address {
        &data.metadata.oracle
    }

    public fun timestamp<T>(data: &Data<T>): u64 {
        data.metadata.timestamp
    }
}
