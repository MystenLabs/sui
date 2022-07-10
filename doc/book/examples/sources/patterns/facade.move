// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x0::core {
    public fun beep() {}
}

module 0x0::facade {
    use 0x0::core;

    public entry fun beep() {
        core::beep()
    }
}
