// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module basics::generic_call {
    // Only used as call-site type arguments, never constructed.
    public struct Wrapper<phantom T> has drop {}
    public enum Either<phantom T, phantom U> has drop { Nothing }

    fun id<T: drop>(x: T): T { x }

    // Type parameters appear in neither the arguments nor the return type, so
    // the call-site instantiation cannot be reconstructed from register types.
    fun phantom_ty<T>(): u64 { 42 }
    fun phantom_ty2<T, U>(): u64 { 43 }

    public fun do_it(): u64 { id<u64>(phantom_ty<bool>()) }

    public fun do_more(): u64 {
        phantom_ty<vector<vector<u64>>>()
            + phantom_ty<Wrapper<vector<bool>>>()
            + phantom_ty2<bool, Wrapper<u64>>()
            + phantom_ty<Either<u64, address>>()
    }
}
