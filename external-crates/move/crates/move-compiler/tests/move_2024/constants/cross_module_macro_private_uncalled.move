// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A macro body referencing its module's private constant is fine while the macro is never
// called: macro bodies are not eagerly checked

module 0x42::a {

const SECRET: u64 = 42;

public macro fun get_secret(): u64 {
    SECRET
}

}
