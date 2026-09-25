// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// `0x1` plays the role of a system package; `0x2` is a user package that calls into it.
module 0x1::sys {
    public fun a() { }
    public fun b() { 0x1::sys::a(); }
}

module 0x2::user {
    public fun calls_sys() {
        0x1::sys::a();
        0x1::sys::b();
    }
}
