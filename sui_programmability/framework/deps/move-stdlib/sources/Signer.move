// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Std::Signer {
    // Borrows the address of the signer
    // Conceptually, you can think of the `signer` as being a struct wrapper around an
    // address
    // ```
    // struct Signer has drop { addr: address }
    // ```
    // `borrow_address` borrows this inner field
    native public fun borrow_address(s: &signer): &address;

    // Copies the address of the signer
    public fun address_of(s: &signer): address {
        *borrow_address(s)
    }

    /// Return true only if `s` is a transaction signer. This is a spec function only available in spec.
    spec native fun is_txn_signer(s: signer): bool;

    /// Return true only if `a` is a transaction signer address. This is a spec function only available in spec.
    spec native fun is_txn_signer_addr(a: address): bool;
}
