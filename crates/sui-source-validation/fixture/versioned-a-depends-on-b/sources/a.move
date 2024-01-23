// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module a::a {
    use b::b::b;
    use b::b::c;
    
    public fun a() : u64 {
        b() + c()
    }
}
