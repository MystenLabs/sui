// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module basics::generic_call {
    fun id<T: drop>(x: T): T { x }

    // Type parameter appears in neither the arguments nor the return type, so
    // the call-site instantiation cannot be reconstructed from register types.
    fun phantom_ty<T>(): u64 { 42 }

    public fun do_it(): u64 { id<u64>(phantom_ty<bool>()) }
}
