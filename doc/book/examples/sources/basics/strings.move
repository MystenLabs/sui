// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module examples::strings {
    use sui::id::VersionedID;
    use sui::tx_context::{Self, TxContext};

    // Use this dependency to get a type wrapper for UTF8 Strings
    use sui::utf8::{Self, String};

    /// A dummy Object that holds a String type
    struct Name has key, store {
        id: VersionedID,

        /// Here it is - the String type
        name: String
    }

    /// Create a name Object by passing raw bytes
    public fun issue_name_nft(
        name_bytes: vector<u8>, ctx: &mut TxContext
    ): Name {
        Name {
            id: tx_context::new_id(ctx),
            name: utf8::string_unsafe(name_bytes)
        }
    }
}
