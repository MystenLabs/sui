// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    public fun t1_1<T>(_: &T) {}
    public fun t1_2<T>(_: &mut T) {}
    public fun t2_1<T: copy + drop + store>(_: &T) {}
    public fun t2_2<T: copy + drop + store>(_: &mut T) {}
    public fun t3_1<T: key>(_: &T) {}
    public fun t3_2<T: key>(_: &mut T) {}
}
