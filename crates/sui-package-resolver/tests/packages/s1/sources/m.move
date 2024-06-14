// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(unused_field)]
module s::m {
    public struct T0 {
        x: u64
    }

    public struct T1 {
        x: u256
    }

    public enum E0 {
        V {
            x: u64
        }
    }

    public enum E1 {
        V {
            x: u256
        }
    }
}
