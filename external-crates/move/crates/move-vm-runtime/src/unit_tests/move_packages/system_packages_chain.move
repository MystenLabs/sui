// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Two system packages: `0x3::pinned_b` calls into `0x1::pinned_a`. Used to verify cross-system
// direct-call rewriting at install time. `pinned_b` lives at `0x3` (not `0x2`) so its address
// doesn't alias the `0x2` user-pkg fixture used by the basic test.
module 0x1::pinned_a {
    public fun a() { }
}

module 0x3::pinned_b {
    public fun b() { 0x1::pinned_a::a(); }
}
