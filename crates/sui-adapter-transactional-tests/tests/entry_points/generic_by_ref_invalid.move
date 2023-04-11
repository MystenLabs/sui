// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// entry functions cannot take primitives by ref, so the generic must be an object

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    public entry fun no<T>(_: &T) {}
}

//# publish
module test::m1 {
    public entry fun no<T>(_: &mut T) {}
}

//# publish
module test::m1 {
    public entry fun no<T: copy + drop + store>(_: &T) {}
}

//# publish
module test::m1 {
    public entry fun no<T: copy + drop + store>(_: &mut T) {}
}
