// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module 0x0::M {
    public enum Inner {
        P(u64),
    }

    public enum Outer {
        X(Inner),
        Y(u64),
    }

    fun f(o: Outer): u64 {
        match (o) {
            Outer::X(Inner::P(v0x0::M)) => v,
            Outer::X Outer::X(Inner::P(v)) => v,
            Outer::X(Inner::Q) => 0,
            Outer::Y(v) => v,
        }
    }
}
