// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::recipient {

    friend sui::object;
    friend sui::address;

    /// The recipient is not an address
    const ENotAnAddress: u64 = 0;
    /// Currently unused. The recipient is not an object
    const ENotAnObject: u64 = 1;

    /// The recipient is an address
    const ADDRESS_RECIPIENT_KIND: u8 = 0;
    /// Currently unused. The recipient is an object
    const OBJECT_RECIPIENT_KIND: u8 = 1;

    /// The recipient of a transfer
    struct Recipient has copy, drop, store {
        /// The kind of recipient, currently only an address recipient is supported,
        /// but object recipients will be supported in the future
        kind: u8,
        /// The underlying value for the recipient, ID or address
        value: address,
    }

    /// internal construction of a Recipient
    public(friend) fun new(kind: u8, value: address): Recipient {
        Recipient { kind, value }
    }

    /// internal deconstruction of a Recipient
    public(friend) fun destroy(recipient: Recipient): (u8, address) {
        let Recipient { kind, value } = recipient;
        (kind, value)
    }

}
