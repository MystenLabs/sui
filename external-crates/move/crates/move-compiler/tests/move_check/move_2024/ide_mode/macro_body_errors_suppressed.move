// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// IDE mode speculatively types macro bodies for annotations. The invalid return
// expression below should not leak diagnostics from that speculative pass.
module a::m {
    public macro fun returns_u64(): u64 {
        true
    }
}
