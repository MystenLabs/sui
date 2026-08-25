// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Reversed-dependency-order variant of `system_packages_chain.move`: the leaf `pinned_a` lives at
// `0x3` while `0x1::pinned_b` depends on it, so the dependency runs opposite to address order.
// Used to verify the install pipeline honors the host-provided dependency order rather than
// iterating packages by address.
module 0x3::pinned_a {
    public fun a() { }
}

module 0x1::pinned_b {
    public fun b() { 0x3::pinned_a::a(); }
}
