// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

module a::m {
    public fun t2(condition: bool) {
        if (condition) @0 else @0;
        if (condition) @0x0 else @0;
        if (condition) @ident else @0;
    }
}
