// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module A::warnings {
    public struct S<T> has drop { value: u64 }

    fun unused_function<T>() {
        let mut value = 0u64;
        value = 1;
    }

    const UNUSED: u64 = 0;

    public fun warning(): u64 {
        let unused = 0u64;
        return 1
    }

    public fun reference(value: &mut u64): u64 {
        let reference = &mut *value;
        *reference
    }
}
