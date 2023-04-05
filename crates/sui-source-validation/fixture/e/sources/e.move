// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module e::e {
    use b::b::b;
    use b::b::c;
    
    public fun e() : u64 {
        b() + c()
    }
}
