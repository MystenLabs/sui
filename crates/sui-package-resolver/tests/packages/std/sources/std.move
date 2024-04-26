// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(unused_field)]
module std::ascii {
    public struct String has copy, drop, store {
        bytes: vector<u8>,
    }
}

#[allow(unused_field)]
module std::option {
    public struct Option<Element> has copy, drop, store {
        vec: vector<Element>
    }
}

#[allow(unused_field)]
module std::string {
    public struct String has copy, drop, store {
        bytes: vector<u8>,
    }
}
