// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(unused_field)]
module std::ascii {
    struct String has copy, drop, store {
        bytes: vector<u8>,
    }
}

#[allow(unused_field)]
module std::option {
    struct Option<Element> has copy, drop, store {
        vec: vector<Element>
    }
}

#[allow(unused_field)]
module std::string {
    struct String has copy, drop, store {
        bytes: vector<u8>,
    }
}
